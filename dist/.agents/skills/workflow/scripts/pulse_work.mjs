#!/usr/bin/env node

/**
 * Purpose: Runtime CLI for canonical workgraph metadata mutations and reads.
 * Caller/flow: Invoked by pulse_work.mjs during execute/swarm/review lifecycle work.
 * Reads/Writes: Reads/writes .pulse/workgraph/items.jsonl, views, lock, and work item files.
 * CLI args: create|show|list|ready|update|close|reopen|dep|children|graph|doctor plus --repo-root/--json.
 * Ownership: Owns workgraph mutation contract; not a conversational router.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs.
 */

import fs from "node:fs";
import path from "node:path";

import { generateItemId, resolveItemId } from "./workgraph_ids.mjs";
import {
  ITEM_KIND_VALUES,
  cloneItemRecord,
  normalizeNullableString,
  normalizeStringArray,
  parsePriority,
  utcNow,
} from "./workgraph_model.mjs";
import { resolveRepoRoot as resolveSharedRepoRoot } from "./pulse_paths.mjs";
import { applyCanonicalPaths, sanitizeSlug } from "./workgraph_paths.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  inspectViewDrift,
  loadItems,
  runMutation,
} from "./workgraph_store.mjs";
import { ensureItemFiles, moveItemContent, scaffoldItemFiles } from "./workgraph_templates.mjs";
import {
  assertItemClosable,
  assertItemReopenable,
  assertValidGenericStatusTransition,
  collectGraphIssues,
} from "./workgraph_validate.mjs";
import { buildGraphView, deriveViewState } from "./workgraph_views.mjs";
import { inspectLock, removeStaleLock } from "./workgraph_lock.mjs";
import { isDirectExecution } from "./cli_execution.mjs";

function resolveRepoRoot(explicitRoot) {
  return resolveSharedRepoRoot({ explicitRoot });
}

function parseArgv(argv) {
  const parsed = {
    options: new Map(),
    positionals: [],
  };

  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith("--")) {
      parsed.positionals.push(token);
      continue;
    }

    const [flagName, inlineValue] = token.slice(2).split("=", 2);
    let value = inlineValue;
    if (value === undefined) {
      const next = argv[index + 1];
      if (next && !next.startsWith("--")) {
        value = next;
        index += 1;
      } else {
        value = true;
      }
    }

    const current = parsed.options.get(flagName);
    if (current === undefined) {
      parsed.options.set(flagName, value);
    } else if (Array.isArray(current)) {
      current.push(value);
    } else {
      parsed.options.set(flagName, [current, value]);
    }
  }

  return parsed;
}

function takeOption(parsed, name, fallback = undefined) {
  return parsed.options.has(name) ? parsed.options.get(name) : fallback;
}

function takeListOption(parsed, name) {
  const value = takeOption(parsed, name);
  if (value === undefined) {
    return [];
  }
  return Array.isArray(value) ? value : [value];
}

function takeBooleanOption(parsed, name) {
  return Boolean(takeOption(parsed, name, false));
}

function requirePositional(parsed, index, label) {
  const value = parsed.positionals[index];
  if (!value) {
    throw new Error(`Missing required argument: ${label}`);
  }
  return value;
}

function uppercaseList(values) {
  return normalizeStringArray(values.map((value) => String(value).trim().toUpperCase()));
}

function buildDisplayItem(item) {
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    status: item.status,
    priority: item.priority,
    owner: item.owner,
    ready: item.ready,
    blocked_by_dependencies: item.blocked_by_dependencies,
    content_path: item.content_path,
    verification_path: item.verification_path,
  };
}

function renderHumanSummary(result) {
  if (result.command === "create") {
    return [
      `Created ${result.item.kind} ${result.item.id}`,
      `Title: ${result.item.title}`,
      `Path: ${result.item.content_path}`,
    ].join("\n");
  }

  if (result.command === "show") {
    const item = result.item;
    return [
      `${item.kind} ${item.id}`,
      `Title: ${item.title}`,
      `Status: ${item.status}`,
      `Ready: ${item.ready ? "yes" : "no"}`,
      `Children: ${item.children.join(", ") || "(none)"}`,
      `Dependencies: ${item.depends_on.join(", ") || "(none)"}`,
    ].join("\n");
  }

  if (result.command === "list" || result.command === "ready" || result.command === "children") {
    return result.items.length === 0
      ? "No matching items."
      : result.items.map((item) => `${item.id} [${item.status}] ${item.title}`).join("\n");
  }

  if (result.command === "update" || result.command === "close" || result.command === "reopen") {
    return [`Updated ${result.item.id}`, `Status: ${result.item.status}`, `Path: ${result.item.content_path}`].join("\n");
  }

  if (result.command === "dep_add") {
    return `Added dependency ${result.dependency_id} -> ${result.item.id}`;
  }

  if (result.command === "dep_rm") {
    return `Removed dependency ${result.dependency_id} from ${result.item.id}`;
  }

  if (result.command === "graph") {
    return `Graph: ${result.graph.nodes.length} nodes, ${result.graph.edges.hierarchy.length} hierarchy edges, ${result.graph.edges.dependencies.length} dependency edges`;
  }

  if (result.command === "doctor") {
    if (result.ok) {
      return result.fixed_actions.length > 0
        ? `Doctor fixed ${result.fixed_actions.length} issue(s).`
        : "Doctor found no issues.";
    }
    return [
      `Doctor found ${result.issues.length} issue(s).`,
      ...result.issues.map((issue) => `- [${issue.code}] ${issue.message}`),
    ].join("\n");
  }

  if (result.command === "help") {
    return [
      "Usage: pulse_work.mjs <command> [options]",
      "",
      ...result.commands.map((commandName) => `  ${commandName}`),
    ].join("\n");
  }

  return JSON.stringify(result, null, 2);
}

function getDecoratedItems(repoRoot) {
  ensureWorkgraphFilesystem(repoRoot);
  return deriveViewState(loadItems(repoRoot));
}

async function handleCreate(repoRoot, parsed) {
  const kind = String(takeOption(parsed, "kind", "")).trim().toUpperCase();
  const title = normalizeNullableString(takeOption(parsed, "title"));
  const parentLookup = normalizeNullableString(takeOption(parsed, "parent"));
  const owner = normalizeNullableString(takeOption(parsed, "owner"));
  const priority = parsePriority(takeOption(parsed, "priority"), 2);
  const labels = normalizeStringArray(takeListOption(parsed, "label"));
  const riskFlags = uppercaseList(takeListOption(parsed, "risk"));

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

  const outcome = await runMutation(repoRoot, "pulse_work.mjs create", ({ items }) => {
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

async function handleShow(repoRoot, parsed) {
  const lookup = requirePositional(parsed, 1, "id");
  const items = getDecoratedItems(repoRoot);
  const id = resolveItemId(items, lookup);
  const item = items.find((candidate) => candidate.id === id);
  return {
    command: "show",
    item,
  };
}

async function handleList(repoRoot, parsed) {
  const items = getDecoratedItems(repoRoot);
  let filtered = [...items];

  const kind = normalizeNullableString(takeOption(parsed, "kind"));
  const status = normalizeNullableString(takeOption(parsed, "status"));
  const epicLookup = normalizeNullableString(takeOption(parsed, "epic"));
  const parentLookup = normalizeNullableString(takeOption(parsed, "parent"));
  const owner = normalizeNullableString(takeOption(parsed, "owner"));
  const label = normalizeNullableString(takeOption(parsed, "label"));

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
  if (owner !== null) {
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

async function handleReady(repoRoot) {
  const items = getDecoratedItems(repoRoot)
    .filter((item) => item.ready)
    .map((item) => buildDisplayItem(item));
  return {
    command: "ready",
    items,
  };
}

async function handleUpdate(repoRoot, parsed) {
  const lookup = requirePositional(parsed, 1, "id");
  const title = normalizeNullableString(takeOption(parsed, "title"));
  const slugValue = normalizeNullableString(takeOption(parsed, "slug"));
  const status = normalizeNullableString(takeOption(parsed, "status"));
  const owner = parsed.options.has("owner") ? normalizeNullableString(takeOption(parsed, "owner")) : undefined;
  const clearOwner = takeBooleanOption(parsed, "clear-owner");
  const blockedReason = parsed.options.has("blocked-reason")
    ? normalizeNullableString(takeOption(parsed, "blocked-reason"))
    : undefined;
  const clearBlockedReason = takeBooleanOption(parsed, "clear-blocked-reason");
  const addLabels = normalizeStringArray(takeListOption(parsed, "add-label"));
  const removeLabels = new Set(normalizeStringArray(takeListOption(parsed, "rm-label")));
  const addRisks = uppercaseList(takeListOption(parsed, "add-risk"));
  const removeRisks = new Set(uppercaseList(takeListOption(parsed, "rm-risk")));
  const priorityProvided = parsed.options.has("priority");

  const outcome = await runMutation(repoRoot, "pulse_work.mjs update", ({ items }) => {
    const id = resolveItemId(items, lookup);
    const previousItem = items.find((item) => item.id === id);
    const nextItem = cloneItemRecord(previousItem);

    if (title) {
      nextItem.title = title;
    }
    if (slugValue) {
      nextItem.slug = sanitizeSlug(slugValue);
    }
    if (priorityProvided) {
      nextItem.priority = parsePriority(takeOption(parsed, "priority"), previousItem.priority);
    }
    if (owner !== undefined) {
      nextItem.owner = owner;
    }
    if (clearOwner) {
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
    if (clearBlockedReason) {
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

async function handleClose(repoRoot, parsed) {
  const lookup = requirePositional(parsed, 1, "id");
  const outcome = await runMutation(repoRoot, "pulse_work.mjs close", ({ items }) => {
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

async function handleReopen(repoRoot, parsed) {
  const lookup = requirePositional(parsed, 1, "id");
  const outcome = await runMutation(repoRoot, "pulse_work.mjs reopen", ({ items }) => {
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

async function handleDependencyMutation(repoRoot, parsed, mode) {
  const lookup = requirePositional(parsed, 2, "id");
  const dependencyLookup = requirePositional(parsed, 3, "depends-on");

  const outcome = await runMutation(repoRoot, `pulse_work.mjs dep ${mode}`, ({ items }) => {
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

async function handleChildren(repoRoot, parsed) {
  const lookup = requirePositional(parsed, 1, "id");
  const items = getDecoratedItems(repoRoot);
  const id = resolveItemId(items, lookup);
  return {
    command: "children",
    items: items.filter((item) => item.parent_id === id).map((item) => buildDisplayItem(item)),
  };
}

async function handleGraph(repoRoot) {
  const items = loadItems(repoRoot);
  return {
    command: "graph",
    graph: buildGraphView(items),
  };
}

async function handleDoctor(repoRoot, parsed) {
  const fix = takeBooleanOption(parsed, "fix");
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
  const fixOutcome = await runMutation(repoRoot, "pulse_work.mjs doctor --fix", ({ items: currentItems }) => ({
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

async function dispatchCommand(repoRoot, parsed) {
  const command = parsed.positionals[0] || "help";

  switch (command) {
    case "create":
      return handleCreate(repoRoot, parsed);
    case "show":
      return handleShow(repoRoot, parsed);
    case "list":
      return handleList(repoRoot, parsed);
    case "ready":
      return handleReady(repoRoot, parsed);
    case "update":
      return handleUpdate(repoRoot, parsed);
    case "close":
      return handleClose(repoRoot, parsed);
    case "reopen":
      return handleReopen(repoRoot, parsed);
    case "dep": {
      const mode = parsed.positionals[1];
      if (!mode || !["add", "rm"].includes(mode)) {
        throw new Error("dep requires add or rm.");
      }
      return handleDependencyMutation(repoRoot, parsed, mode);
    }
    case "children":
      return handleChildren(repoRoot, parsed);
    case "graph":
      return handleGraph(repoRoot, parsed);
    case "doctor":
      return handleDoctor(repoRoot, parsed);
    case "help":
    default:
      return {
        command: "help",
        ok: true,
        commands: [
          "create",
          "show",
          "list",
          "ready",
          "update",
          "close",
          "reopen",
          "dep add",
          "dep rm",
          "children",
          "graph",
          "doctor",
        ],
      };
  }
}

export async function main(argv = process.argv.slice(2)) {
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(
      [
        "Usage: pulse_work.mjs <command> [options]",
        "",
        "Commands:",
        "  create --kind <kind> --title <title> [--parent <id>]",
        "  show <id>",
        "  list [--kind <kind>] [--status <status>] [--epic <id>] [--parent <id>] [--owner <owner>] [--label <label>]",
        "  ready",
        "  update <id> [--title <title>] [--slug <slug>] [--status <status>] [--priority <n>] [--owner <owner>] [--add-label <label>] [--rm-label <label>] [--add-risk <flag>] [--rm-risk <flag>] [--blocked-reason <text>]",
        "  close <id>",
        "  reopen <id>",
        "  dep add <id> <depends-on>",
        "  dep rm <id> <depends-on>",
        "  children <id>",
        "  graph",
        "  doctor [--fix]",
      ].join("\n"),
    );
    return 0;
  }

  const parsed = parseArgv(argv);
  const repoRoot = resolveRepoRoot(normalizeNullableString(takeOption(parsed, "repo-root")));
  const asJson = takeBooleanOption(parsed, "json");

  try {
    const result = await dispatchCommand(repoRoot, parsed);
    process.stdout.write(asJson ? `${JSON.stringify(result, null, 2)}\n` : `${renderHumanSummary(result)}\n`);
    return result.ok === false ? 1 : 0;
  } catch (error) {
    const payload = {
      ok: false,
      error: error.message,
      issues: error.issues || [],
    };
    process.stdout.write(asJson ? `${JSON.stringify(payload, null, 2)}\n` : `Error: ${error.message}\n`);
    return 1;
  }
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
