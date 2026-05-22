import fs from "node:fs";
import path from "node:path";

import {
  readJsonIfExists,
  writeJsonAtomic,
  writeTextAtomic,
} from "../core/fs.mjs";
import { acquireWriteLock, releaseWriteLock } from "./lock.mjs";
import {
  ITEM_KIND_VALUES,
  RISK_FLAG_VALUES,
  STATUS_VALUES,
  canonicalizeItemRecord,
  sortItemsDeterministically,
} from "./model.mjs";
import { buildViews } from "./views.mjs";
import { assertValidGraph } from "./validate.mjs";

export const WORKGRAPH_SCHEMA = {
  $schema: "https://json-schema.org/draft/2020-12/schema",
  title: "Pulse Workgraph Item",
  type: "object",
  additionalProperties: false,
  required: [
    "id",
    "kind",
    "title",
    "slug",
    "status",
    "parent_id",
    "epic_id",
    "depends_on",
    "priority",
    "owner",
    "labels",
    "risk_flags",
    "blocked_reason",
    "content_path",
    "verification_path",
    "created_at",
    "updated_at",
    "closed_at",
  ],
  properties: {
    id: { type: "string", pattern: "^[ESBT]-[0-9A-Z]+(?:-[0-9]+)?$" },
    kind: { enum: ITEM_KIND_VALUES },
    title: { type: "string", minLength: 1 },
    slug: { type: "string", minLength: 1 },
    status: { enum: STATUS_VALUES },
    parent_id: { type: ["string", "null"] },
    epic_id: { type: "string" },
    depends_on: {
      type: "array",
      items: { type: "string" },
      uniqueItems: true,
    },
    priority: { type: "integer" },
    owner: { type: ["string", "null"] },
    labels: {
      type: "array",
      items: { type: "string" },
      uniqueItems: true,
    },
    risk_flags: {
      type: "array",
      items: { enum: RISK_FLAG_VALUES },
      uniqueItems: true,
    },
    blocked_reason: { type: ["string", "null"] },
    content_path: { type: "string", pattern: "^works/" },
    verification_path: { type: ["string", "null"] },
    created_at: { type: "string", format: "date-time" },
    updated_at: { type: "string", format: "date-time" },
    closed_at: { type: ["string", "null"], format: "date-time" },
  },
  x_workgraph_rules: {
    hierarchy: {
      EPIC: { parent_id: null, epic_id: "self" },
      STORY: { parent_kind: ["EPIC"], epic_id: "ancestor_epic" },
      TASK: { parent_kind: ["STORY"], epic_id: "ancestor_epic" },
      BUG: { parent_kind: ["STORY"], epic_id: "ancestor_epic" },
    },
    lifecycle: {
      blocked_reason_required_for: ["BLOCKED"],
      closed_at_required_for: ["CLOSED"],
      close_requires_verification_kinds: ["TASK", "BUG"],
    },
    dependencies: {
      allow_cross_epic: true,
      allow_cycles: false,
    },
  },
};

export function getWorkgraphPaths(repoRoot) {
  const workgraphRoot = path.join(repoRoot, ".pulse", "workgraph");
  const viewsDir = path.join(workgraphRoot, "views");
  return {
    repoRoot,
    workgraphRoot,
    itemsPath: path.join(workgraphRoot, "items.jsonl"),
    schemaPath: path.join(workgraphRoot, "schema.json"),
    lockPath: path.join(workgraphRoot, "write.lock"),
    viewsDir,
    viewPaths: {
      active: path.join(viewsDir, "active.json"),
      closed: path.join(viewsDir, "closed.json"),
      ready: path.join(viewsDir, "ready.json"),
      graph: path.join(viewsDir, "graph.json"),
    },
  };
}

export function buildCanonicalItemsText(items) {
  return sortItemsDeterministically(items)
    .map((item) => JSON.stringify(canonicalizeItemRecord(item)))
    .join("\n")
    .concat(items.length > 0 ? "\n" : "");
}

export function ensureWorkgraphFilesystem(repoRoot, options = {}) {
  const paths = getWorkgraphPaths(repoRoot);
  fs.mkdirSync(paths.viewsDir, { recursive: true });
  if (!fs.existsSync(paths.itemsPath)) {
    fs.writeFileSync(paths.itemsPath, "", "utf8");
  }
  if (!fs.existsSync(paths.schemaPath) || options.syncSchema) {
    fs.writeFileSync(paths.schemaPath, `${JSON.stringify(WORKGRAPH_SCHEMA, null, 2)}\n`, "utf8");
  }
  return paths;
}

export function parseItemsText(text) {
  const lines = String(text || "")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  return lines.map((line, index) => {
    try {
      return JSON.parse(line);
    } catch (error) {
      throw new Error(`Failed to parse items.jsonl line ${index + 1}: ${error.message}`);
    }
  });
}

export function loadItems(repoRoot) {
  const paths = ensureWorkgraphFilesystem(repoRoot);
  return parseItemsText(fs.readFileSync(paths.itemsPath, "utf8"));
}

export { readJsonIfExists, writeJsonAtomic, writeTextAtomic };

export function writeItems(repoRoot, items) {
  const paths = ensureWorkgraphFilesystem(repoRoot);
  const text = buildCanonicalItemsText(items);
  writeTextAtomic(paths.itemsPath, text);
  return paths.itemsPath;
}

export function writeViews(repoRoot, items) {
  const paths = ensureWorkgraphFilesystem(repoRoot);
  const views = buildViews(items);
  writeJsonAtomic(paths.viewPaths.active, views.active);
  writeJsonAtomic(paths.viewPaths.closed, views.closed);
  writeJsonAtomic(paths.viewPaths.ready, views.ready);
  writeJsonAtomic(paths.viewPaths.graph, views.graph);
  return views;
}

export function inspectViewDrift(repoRoot, items) {
  const paths = ensureWorkgraphFilesystem(repoRoot);
  const views = buildViews(items);
  const drifts = [];

  for (const [name, filePath] of Object.entries(paths.viewPaths)) {
    if (!fs.existsSync(filePath)) {
      drifts.push({ view: name, reason: "missing" });
      continue;
    }

    const actual = fs.readFileSync(filePath, "utf8");
    const expected = `${JSON.stringify(views[name], null, 2)}\n`;
    if (actual !== expected) {
      drifts.push({ view: name, reason: "stale" });
    }
  }

  return {
    drifts,
    expected: views,
  };
}

export async function runMutation(repoRoot, command, mutate) {
  const paths = ensureWorkgraphFilesystem(repoRoot);
  const lock = acquireWriteLock(paths.lockPath, command);

  try {
    const currentItems = loadItems(repoRoot);
    const outcome = (await mutate({ repoRoot, paths, items: currentItems })) || {};
    const nextItems = outcome.items || currentItems;

    assertValidGraph(nextItems);

    if (typeof outcome.beforeWrite === "function") {
      await outcome.beforeWrite({ repoRoot, paths, previousItems: currentItems, nextItems });
    }

    writeItems(repoRoot, nextItems);
    const views = writeViews(repoRoot, nextItems);
    return {
      ...outcome,
      items: nextItems,
      views,
      paths,
    };
  } finally {
    releaseWriteLock(lock);
  }
}
