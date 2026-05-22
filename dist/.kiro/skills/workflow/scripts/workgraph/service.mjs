import { generateItemId, resolveItemId } from "./ids.mjs";
import {
  ITEM_KIND_VALUES,
  cloneItemRecord,
  normalizeNullableString,
  normalizeStringArray,
  parsePriority,
  utcNow,
} from "./model.mjs";
import { applyCanonicalPaths, sanitizeSlug } from "./paths.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  inspectViewDrift,
  loadItems,
  runMutation,
} from "./store.mjs";
import { ensureItemFiles, moveItemContent, scaffoldItemFiles } from "./templates.mjs";
import {
  assertItemClosable,
  assertItemReopenable,
  assertValidGenericStatusTransition,
  collectGraphIssues,
} from "./validate.mjs";
import { buildGraphView, deriveViewState } from "./views.mjs";
import { inspectLock, removeStaleLock } from "./lock.mjs";

export const WORKGRAPH_COMMANDS = [
  "create",
  "show",
  "list",
  "ready",
  "update",
  "close",
  "reopen",
  "dep add",
  "dep rm",
  "link add",
  "link rm",
  "children",
  "graph",
  "doctor",
];

function uppercaseList(values) {
  return normalizeStringArray((values || []).map((value) => String(value).trim().toUpperCase()));
}

export function buildDisplayItem(item) {
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    status: item.status,
    priority: item.priority,
    owner: item.owner,
    ready: item.ready,
    blocked_by_dependencies: item.blocked_by_dependencies,
    linked_items: item.linked_items,
    reverse_links: item.reverse_links,
    content_path: item.content_path,
    verification_path: item.verification_path,
  };
}

export function getDecoratedItems(repoRoot) {
  ensureWorkgraphFilesystem(repoRoot);
  return deriveViewState(loadItems(repoRoot));
}

export async function createItem(repoRoot, options = {}) {
  const kind = String(options.kind || "").trim().toUpperCase();
  const title = normalizeNullableString(options.title);
  const parentLookup = normalizeNullableString(options.parent);
  const owner = normalizeNullableString(options.owner);
  const priority = parsePriority(options.priority, 2);
  const labels = normalizeStringArray(options.labels || []);
  const riskFlags = uppercaseList(options.riskFlags || []);

  if (!ITEM_KIND_VALUES.includes(kind)) {
    throw new Error(`--kind must be one of: ${ITEM_KIND_VALUES.join(", ")}`);
  }
  if (!title) {
    throw new Error("--title is required.");
  }
  if (kind === "EPIC" && parentLookup) {
    throw new Error("EPIC items must not use --parent.");
  }
  if (kind !== "EPIC" && !parentLookup) {
    throw new Error(`${kind} items require --parent.`);
  }

  const outcome = await runMutation(repoRoot, "workgraph create", ({ items }) => {
    const existingIds = items.map((item) => item.id);
    const id = generateItemId(kind, existingIds);
    const now = utcNow();

    let parentId = null;
    let epicId = id;
    if (parentLookup) {
      parentId = resolveItemId(items, parentLookup);
      const parent = items.find((item) => item.id === parentId);
      if (kind === "STORY" && parent.kind !== "EPIC") {
        throw new Error("STORY items must be created under an EPIC.");
      }
      if ((kind === "TASK" || kind === "BUG") && parent.kind !== "STORY") {
        throw new Error(`${kind} items must be created under a STORY.`);
      }
      epicId = parent.kind === "EPIC" ? parent.id : parent.epic_id;
    }

    const draft = {
      id,
      kind,
      title,
      slug: sanitizeSlug(title),
      status: "OPEN",
      parent_id: parentId,
      epic_id: epicId,
      depends_on: [],
      linked_items: [],
      priority,
      owner,
      labels,
      risk_flags: riskFlags,
      blocked_reason: null,
      content_path: "works/pending/README.md",
      verification_path: kind === "TASK" || kind === "BUG" ? "works/pending/verification.md" : null,
      created_at: now,
      updated_at: now,
      closed_at: null,
    };

    const nextItems = applyCanonicalPaths([...items, draft]);
    const created = nextItems.find((item) => item.id === id);

    return {
      items: nextItems,
      item: created,
      beforeWrite: ({ repoRoot: currentRoot }) => {
        scaffoldItemFiles(currentRoot, created);
      },
    };
  });

  return {
    command: "create",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
  };
}

export async function showItem(repoRoot, { id: lookup } = {}) {
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }
  const items = getDecoratedItems(repoRoot);
  const id = resolveItemId(items, lookup);
  const item = items.find((candidate) => candidate.id === id);
  return {
    command: "show",
    item,
  };
}

export async function listItems(repoRoot, options = {}) {
  const items = getDecoratedItems(repoRoot);
  let filtered = [...items];

  const kind = normalizeNullableString(options.kind);
  const status = normalizeNullableString(options.status);
  const epicLookup = normalizeNullableString(options.epic);
  const parentLookup = normalizeNullableString(options.parent);
  const owner = options.ownerProvided ? normalizeNullableString(options.owner) : null;
  const label = normalizeNullableString(options.label);

  if (kind) {
    filtered = filtered.filter((item) => item.kind === kind.toUpperCase());
  }
  if (status) {
    filtered = filtered.filter((item) => item.status === status.toUpperCase());
  }
  if (epicLookup) {
    const epicId = resolveItemId(items, epicLookup);
    filtered = filtered.filter((item) => item.epic_id === epicId);
  }
  if (parentLookup) {
    const parentId = resolveItemId(items, parentLookup);
    filtered = filtered.filter((item) => item.parent_id === parentId);
  }
  if (options.ownerProvided) {
    filtered = filtered.filter((item) => item.owner === owner);
  }
  if (label) {
    filtered = filtered.filter((item) => item.labels.includes(label));
  }

  return {
    command: "list",
    items: filtered.map((item) => buildDisplayItem(item)),
  };
}

export async function readyItems(repoRoot) {
  const items = getDecoratedItems(repoRoot)
    .filter((item) => item.ready)
    .map((item) => buildDisplayItem(item));
  return {
    command: "ready",
    items,
  };
}

export async function updateItem(repoRoot, options = {}) {
  const lookup = options.id;
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }

  const title = normalizeNullableString(options.title);
  const slugValue = normalizeNullableString(options.slug);
  const status = normalizeNullableString(options.status);
  const owner = options.ownerProvided ? normalizeNullableString(options.owner) : undefined;
  const blockedReason = options.blockedReasonProvided
    ? normalizeNullableString(options.blockedReason)
    : undefined;
  const addLabels = normalizeStringArray(options.addLabels || []);
  const removeLabels = new Set(normalizeStringArray(options.removeLabels || []));
  const addRisks = uppercaseList(options.addRisks || []);
  const removeRisks = new Set(uppercaseList(options.removeRisks || []));

  const outcome = await runMutation(repoRoot, "workgraph update", ({ items }) => {
    const id = resolveItemId(items, lookup);
    const previousItem = items.find((item) => item.id === id);
    const nextItem = cloneItemRecord(previousItem);

    if (title) {
      nextItem.title = title;
    }
    if (slugValue) {
      nextItem.slug = sanitizeSlug(slugValue);
    }
    if (options.priorityProvided) {
      nextItem.priority = parsePriority(options.priority, previousItem.priority);
    }
    if (owner !== undefined) {
      nextItem.owner = owner;
    }
    if (options.clearOwner) {
      nextItem.owner = null;
    }
    if (status) {
      const nextStatus = status.toUpperCase();
      assertValidGenericStatusTransition(previousItem, nextStatus);
      nextItem.status = nextStatus;
      if (nextStatus !== "BLOCKED") {
        nextItem.blocked_reason = null;
      }
    }
    if (blockedReason !== undefined) {
      nextItem.blocked_reason = blockedReason;
    }
    if (options.clearBlockedReason) {
      nextItem.blocked_reason = null;
    }

    nextItem.labels = normalizeStringArray([
      ...nextItem.labels.filter((value) => !removeLabels.has(value)),
      ...addLabels,
    ]);
    nextItem.risk_flags = uppercaseList([
      ...nextItem.risk_flags.filter((value) => !removeRisks.has(value)),
      ...addRisks,
    ]);

    if (nextItem.status === "BLOCKED" && !nextItem.blocked_reason) {
      throw new Error("BLOCKED items require --blocked-reason.");
    }
    if (nextItem.status !== "BLOCKED") {
      nextItem.blocked_reason = null;
    }

    nextItem.updated_at = utcNow();
    const nextItems = applyCanonicalPaths(
      items.map((item) => (item.id === id ? nextItem : cloneItemRecord(item))),
    );
    const updated = nextItems.find((item) => item.id === id);

    return {
      items: nextItems,
      item: updated,
      beforeWrite: ({ repoRoot: currentRoot }) => {
        moveItemContent(currentRoot, previousItem, updated);
      },
    };
  });

  return {
    command: "update",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
  };
}

export async function closeItem(repoRoot, { id: lookup } = {}) {
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }

  const outcome = await runMutation(repoRoot, "workgraph close", ({ items }) => {
    const id = resolveItemId(items, lookup);
    const previousItem = items.find((item) => item.id === id);
    if (previousItem.status === "CLOSED") {
      throw new Error(`Item ${id} is already CLOSED.`);
    }

    assertItemClosable(previousItem, items, repoRoot);
    const nextItem = {
      ...cloneItemRecord(previousItem),
      status: "CLOSED",
      blocked_reason: null,
      closed_at: utcNow(),
      updated_at: utcNow(),
    };

    return {
      items: items.map((item) => (item.id === id ? nextItem : cloneItemRecord(item))),
      item: nextItem,
    };
  });

  return {
    command: "close",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
  };
}

export async function reopenItem(repoRoot, { id: lookup } = {}) {
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }

  const outcome = await runMutation(repoRoot, "workgraph reopen", ({ items }) => {
    const id = resolveItemId(items, lookup);
    const previousItem = items.find((item) => item.id === id);
    assertItemReopenable(previousItem);
    const nextItem = {
      ...cloneItemRecord(previousItem),
      status: "OPEN",
      closed_at: null,
      blocked_reason: null,
      updated_at: utcNow(),
    };

    return {
      items: items.map((item) => (item.id === id ? nextItem : cloneItemRecord(item))),
      item: nextItem,
    };
  });

  return {
    command: "reopen",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
  };
}

export async function mutateDependencies(repoRoot, options = {}) {
  const { mode, id: lookup, dependencyId: dependencyLookup } = options;
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }
  if (!dependencyLookup) {
    throw new Error("Missing required argument: depends-on");
  }
  if (!mode || !["add", "rm"].includes(mode)) {
    throw new Error("dep requires add or rm.");
  }

  const outcome = await runMutation(repoRoot, `workgraph dep ${mode}`, ({ items }) => {
    const id = resolveItemId(items, lookup);
    const dependencyId = resolveItemId(items, dependencyLookup);
    const previousItem = items.find((item) => item.id === id);
    const nextItem = cloneItemRecord(previousItem);

    if (mode === "add") {
      nextItem.depends_on = normalizeStringArray([...nextItem.depends_on, dependencyId]);
    } else {
      nextItem.depends_on = nextItem.depends_on.filter((candidate) => candidate !== dependencyId);
    }
    nextItem.updated_at = utcNow();

    return {
      items: items.map((item) => (item.id === id ? nextItem : cloneItemRecord(item))),
      item: nextItem,
      dependency_id: dependencyId,
    };
  });

  return {
    command: mode === "add" ? "dep_add" : "dep_rm",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
    dependency_id: outcome.dependency_id,
  };
}

export async function mutateLinks(repoRoot, options = {}) {
  const { mode, id: lookup, linkedItemId: linkedItemLookup } = options;
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }
  if (!linkedItemLookup) {
    throw new Error("Missing required argument: linked-item");
  }
  if (!mode || !["add", "rm"].includes(mode)) {
    throw new Error("link requires add or rm.");
  }

  const outcome = await runMutation(repoRoot, `workgraph link ${mode}`, ({ items }) => {
    const id = resolveItemId(items, lookup);
    const linkedItemId = resolveItemId(items, linkedItemLookup);
    if (id === linkedItemId) {
      throw new Error(`Item ${id} cannot link to itself.`);
    }
    const previousItem = items.find((item) => item.id === id);
    const nextItem = cloneItemRecord(previousItem);

    if (mode === "add") {
      nextItem.linked_items = normalizeStringArray([...nextItem.linked_items, linkedItemId]);
    } else {
      nextItem.linked_items = nextItem.linked_items.filter((candidate) => candidate !== linkedItemId);
    }
    nextItem.updated_at = utcNow();

    return {
      items: items.map((item) => (item.id === id ? nextItem : cloneItemRecord(item))),
      item: nextItem,
      linked_item_id: linkedItemId,
    };
  });

  return {
    command: mode === "add" ? "link_add" : "link_rm",
    item: deriveViewState(outcome.items).find((item) => item.id === outcome.item.id),
    linked_item_id: outcome.linked_item_id,
  };
}

export async function childrenOf(repoRoot, { id: lookup } = {}) {
  if (!lookup) {
    throw new Error("Missing required argument: id");
  }

  const items = getDecoratedItems(repoRoot);
  const id = resolveItemId(items, lookup);
  return {
    command: "children",
    items: items.filter((item) => item.parent_id === id).map((item) => buildDisplayItem(item)),
  };
}

export async function graph(repoRoot) {
  const items = loadItems(repoRoot);
  return {
    command: "graph",
    graph: buildGraphView(items),
  };
}

export async function doctor(repoRoot, options = {}) {
  const fix = Boolean(options.fix);
  const paths = getWorkgraphPaths(repoRoot);
  ensureWorkgraphFilesystem(repoRoot);
  const items = loadItems(repoRoot);
  const issues = collectGraphIssues(items, { repoRoot });
  const lockState = inspectLock(paths.lockPath);
  const viewState = inspectViewDrift(repoRoot, items);

  for (const drift of viewState.drifts) {
    issues.push({
      code: drift.reason === "missing" ? "missing_view" : "stale_view",
      message: `View ${drift.view}.json is ${drift.reason}.`,
      view: drift.view,
    });
  }
  if (lockState.exists && lockState.stale) {
    issues.push({
      code: "stale_lock",
      message: `Workgraph lock at ${paths.lockPath} is stale.`,
      metadata: lockState.metadata,
    });
  }

  const result = {
    command: "doctor",
    ok: issues.length === 0,
    issues,
    fixed_actions: [],
    lock: lockState,
  };

  if (!fix) {
    return result;
  }

  if (lockState.exists && lockState.stale) {
    removeStaleLock(paths.lockPath);
    result.fixed_actions.push("remove_stale_lock");
  }

  const canonicalItems = applyCanonicalPaths(items);
  const canonicalById = new Map(canonicalItems.map((item) => [item.id, item]));
  const fixOutcome = await runMutation(repoRoot, "workgraph doctor --fix", ({ items: currentItems }) => ({
    items: currentItems,
    beforeWrite: ({ repoRoot: currentRoot }) => {
      for (const item of currentItems) {
        const canonical = canonicalById.get(item.id);
        if (
          canonical &&
          item.content_path === canonical.content_path &&
          (item.verification_path || null) === (canonical.verification_path || null)
        ) {
          const written = ensureItemFiles(currentRoot, item);
          if (written.length > 0) {
            result.fixed_actions.push(...written.map((filePath) => `create_missing_file:${filePath}`));
          }
        }
      }
    },
  }));

  if (viewState.drifts.length > 0) {
    result.fixed_actions.push("rebuild_views");
  }
  if (items.length > 0) {
    result.fixed_actions.push("normalize_items_ordering");
  }

  const nextIssues = collectGraphIssues(fixOutcome.items, { repoRoot });
  const nextViewState = inspectViewDrift(repoRoot, fixOutcome.items);
  for (const drift of nextViewState.drifts) {
    nextIssues.push({
      code: drift.reason === "missing" ? "missing_view" : "stale_view",
      message: `View ${drift.view}.json is ${drift.reason}.`,
      view: drift.view,
    });
  }

  result.ok = nextIssues.length === 0;
  result.issues = nextIssues;
  return result;
}
