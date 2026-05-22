import fs from "node:fs";

import {
  ITEM_KIND_VALUES,
  RISK_FLAG_VALUES,
  STATUS_VALUES,
  cloneItemRecord,
  isIsoUtcTimestamp,
  normalizeNullableString,
} from "./model.mjs";
import {
  applyCanonicalPaths,
  assertSafeRelativeWorkPath,
  toFilesystemPath,
} from "./paths.mjs";

const REQUIRED_FIELDS = [
  "id",
  "kind",
  "title",
  "slug",
  "status",
  "parent_id",
  "epic_id",
  "depends_on",
  "content_path",
  "created_at",
  "updated_at",
  "priority",
  "owner",
  "labels",
  "risk_flags",
  "verification_path",
  "blocked_reason",
  "closed_at",
];
const VERIFICATION_HEADINGS = [
  "## Evidence Summary",
  "## Commands Run",
  "## Observed Outputs",
  "## Attempts",
  "## Artifacts",
  "## Unresolved Gaps",
];
const GENERIC_STATUS_TRANSITIONS = {
  OPEN: new Set(["OPEN", "IN_PROGRESS", "BLOCKED"]),
  IN_PROGRESS: new Set(["OPEN", "IN_PROGRESS", "BLOCKED"]),
  BLOCKED: new Set(["OPEN", "IN_PROGRESS", "BLOCKED"]),
  CLOSED: new Set(),
};

function createIssue(code, message, extra = {}) {
  return {
    code,
    message,
    ...extra,
  };
}

function parseFrontmatter(markdown) {
  const match = String(markdown || "").match(/^---\n([\s\S]*?)\n---\n?/);
  if (!match) {
    return { keys: [], values: new Map() };
  }

  const keys = [];
  const values = new Map();
  for (const line of match[1].split("\n")) {
    const keyMatch = line.match(/^([A-Za-z0-9_-]+)\s*:\s*(.*)$/);
    if (!keyMatch) {
      continue;
    }
    const key = keyMatch[1];
    const value = keyMatch[2].trim();
    keys.push(key);
    values.set(key, value);
  }

  return { keys, values };
}

function validateFrontmatterContract(item, markdown, fileLabel, issues) {
  const { keys, values } = parseFrontmatter(markdown);
  if (keys.length === 0 || !keys.includes("id")) {
    issues.push(createIssue("missing_id_frontmatter", `Item ${item.id} ${fileLabel} must include id frontmatter.`, { item_id: item.id }));
    return;
  }

  const leakedKeys = keys.filter((key) => key !== "id");
  if (leakedKeys.length > 0) {
    issues.push(createIssue("frontmatter_metadata_leak", `Item ${item.id} ${fileLabel} leaks metadata keys: ${leakedKeys.join(", ")}.`, { item_id: item.id }));
  }

  if ((values.get("id") || "") !== item.id) {
    issues.push(createIssue("frontmatter_id_mismatch", `Item ${item.id} ${fileLabel} frontmatter id must match item id.`, {
      item_id: item.id,
      expected: item.id,
      actual: values.get("id") || "",
    }));
  }
}

function validateRequiredFields(item, issues) {
  for (const field of REQUIRED_FIELDS) {
    if (!Object.prototype.hasOwnProperty.call(item, field)) {
      issues.push(createIssue("missing_field", `Item ${item.id || "(unknown)"} is missing ${field}.`, { item_id: item.id, field }));
    }
  }
}

function validateScalarFields(item, issues) {
  if (!ITEM_KIND_VALUES.includes(item.kind)) {
    issues.push(createIssue("invalid_kind", `Item ${item.id} has invalid kind ${item.kind}.`, { item_id: item.id }));
  }
  if (!STATUS_VALUES.includes(item.status)) {
    issues.push(createIssue("invalid_status", `Item ${item.id} has invalid status ${item.status}.`, { item_id: item.id }));
  }
  if (typeof item.title !== "string" || !item.title.trim()) {
    issues.push(createIssue("invalid_title", `Item ${item.id} must have a non-empty title.`, { item_id: item.id }));
  }
  if (typeof item.slug !== "string" || !item.slug.trim()) {
    issues.push(createIssue("invalid_slug", `Item ${item.id} must have a non-empty slug.`, { item_id: item.id }));
  }
  if (!Number.isInteger(item.priority)) {
    issues.push(createIssue("invalid_priority", `Item ${item.id} must have an integer priority.`, { item_id: item.id }));
  }
  if (item.owner !== null && typeof item.owner !== "string") {
    issues.push(createIssue("invalid_owner", `Item ${item.id} must use a string or null owner.`, { item_id: item.id }));
  }
  if (!Array.isArray(item.labels)) {
    issues.push(createIssue("invalid_labels", `Item ${item.id} must have a labels array.`, { item_id: item.id }));
  }
  if (!Array.isArray(item.risk_flags)) {
    issues.push(createIssue("invalid_risk_flags", `Item ${item.id} must have a risk_flags array.`, { item_id: item.id }));
  }
  if (!Array.isArray(item.depends_on)) {
    issues.push(createIssue("invalid_dependencies", `Item ${item.id} must have a depends_on array.`, { item_id: item.id }));
  }
  if (!isIsoUtcTimestamp(item.created_at)) {
    issues.push(createIssue("invalid_created_at", `Item ${item.id} has invalid created_at ${item.created_at}.`, { item_id: item.id }));
  }
  if (!isIsoUtcTimestamp(item.updated_at)) {
    issues.push(createIssue("invalid_updated_at", `Item ${item.id} has invalid updated_at ${item.updated_at}.`, { item_id: item.id }));
  }

  if (item.closed_at !== null && !isIsoUtcTimestamp(item.closed_at)) {
    issues.push(createIssue("invalid_closed_at", `Item ${item.id} has invalid closed_at ${item.closed_at}.`, { item_id: item.id }));
  }
  if (item.blocked_reason !== null && typeof item.blocked_reason !== "string") {
    issues.push(createIssue("invalid_blocked_reason", `Item ${item.id} must use a string or null blocked_reason.`, { item_id: item.id }));
  }

  const labelSet = new Set(item.labels || []);
  if (labelSet.size !== (item.labels || []).length) {
    issues.push(createIssue("duplicate_labels", `Item ${item.id} has duplicate labels.`, { item_id: item.id }));
  }

  const dependencySet = new Set(item.depends_on || []);
  if (dependencySet.size !== (item.depends_on || []).length) {
    issues.push(createIssue("duplicate_dependencies", `Item ${item.id} has duplicate dependencies.`, { item_id: item.id }));
  }

  const riskSet = new Set(item.risk_flags || []);
  if (riskSet.size !== (item.risk_flags || []).length) {
    issues.push(createIssue("duplicate_risk_flags", `Item ${item.id} has duplicate risk flags.`, { item_id: item.id }));
  }

  for (const risk of item.risk_flags || []) {
    if (!RISK_FLAG_VALUES.includes(risk)) {
      issues.push(createIssue("invalid_risk_flag", `Item ${item.id} uses unsupported risk flag ${risk}.`, { item_id: item.id }));
    }
  }

  if (item.kind === "TASK" || item.kind === "BUG") {
    if (item.verification_path === null || typeof item.verification_path !== "string") {
      issues.push(createIssue("missing_verification_path", `Item ${item.id} must have a verification_path.`, { item_id: item.id }));
    }
  } else if (item.verification_path !== null) {
    issues.push(createIssue("unexpected_verification_path", `Item ${item.id} must not have a verification_path.`, { item_id: item.id }));
  }

  if (item.status === "BLOCKED") {
    if (!normalizeNullableString(item.blocked_reason)) {
      issues.push(createIssue("missing_blocked_reason", `Item ${item.id} is BLOCKED but missing blocked_reason.`, { item_id: item.id }));
    }
  } else if (item.blocked_reason !== null) {
    issues.push(createIssue("unexpected_blocked_reason", `Item ${item.id} is not BLOCKED and must have blocked_reason = null.`, { item_id: item.id }));
  }

  if (item.status === "CLOSED") {
    if (!item.closed_at) {
      issues.push(createIssue("missing_closed_at", `Item ${item.id} is CLOSED but missing closed_at.`, { item_id: item.id }));
    }
  } else if (item.closed_at !== null) {
    issues.push(createIssue("unexpected_closed_at", `Item ${item.id} is not CLOSED and must have closed_at = null.`, { item_id: item.id }));
  }
}

function validateHierarchy(item, recordsById, issues) {
  const parent = item.parent_id ? recordsById.get(item.parent_id) : null;
  const epic = item.epic_id ? recordsById.get(item.epic_id) : null;

  if (item.kind === "EPIC") {
    if (item.parent_id !== null) {
      issues.push(createIssue("invalid_epic_parent", `Epic ${item.id} must use parent_id = null.`, { item_id: item.id }));
    }
    if (item.epic_id !== item.id) {
      issues.push(createIssue("invalid_epic_id", `Epic ${item.id} must use epic_id = id.`, { item_id: item.id }));
    }
    return;
  }

  if (!parent) {
    issues.push(createIssue("missing_parent", `Item ${item.id} references missing parent ${item.parent_id}.`, { item_id: item.id }));
  } else if (item.kind === "STORY" && parent.kind !== "EPIC") {
    issues.push(createIssue("invalid_story_parent", `Story ${item.id} must belong to an epic.`, { item_id: item.id }));
  } else if ((item.kind === "TASK" || item.kind === "BUG") && parent.kind !== "STORY") {
    issues.push(createIssue("invalid_item_parent", `Item ${item.id} must belong to a story.`, { item_id: item.id }));
  }

  if (!epic) {
    issues.push(createIssue("missing_epic", `Item ${item.id} references missing epic ${item.epic_id}.`, { item_id: item.id }));
  } else if (epic.kind !== "EPIC") {
    issues.push(createIssue("invalid_epic_reference", `Item ${item.id} epic_id must point to an epic.`, { item_id: item.id }));
  }

  if (parent && epic) {
    if (parent.kind === "EPIC" && parent.id !== epic.id) {
      issues.push(createIssue("epic_parent_mismatch", `Item ${item.id} parent epic does not match epic_id.`, { item_id: item.id }));
    }
    if (parent.kind === "STORY" && parent.epic_id !== item.epic_id) {
      issues.push(createIssue("story_epic_mismatch", `Item ${item.id} story parent epic_id does not match item epic_id.`, { item_id: item.id }));
    }
  }
}

function validateDependencies(item, recordsById, issues) {
  for (const dependencyId of item.depends_on || []) {
    if (dependencyId === item.id) {
      issues.push(createIssue("self_dependency", `Item ${item.id} cannot depend on itself.`, { item_id: item.id }));
      continue;
    }
    if (!recordsById.has(dependencyId)) {
      issues.push(createIssue("missing_dependency", `Item ${item.id} references missing dependency ${dependencyId}.`, { item_id: item.id }));
    }
  }
}

function findDependencyCycles(records) {
  const recordsById = new Map(records.map((item) => [item.id, item]));
  const visited = new Set();
  const stack = [];
  const active = new Set();
  const cycles = [];

  function visit(itemId) {
    if (active.has(itemId)) {
      const cycleStart = stack.indexOf(itemId);
      cycles.push([...stack.slice(cycleStart), itemId]);
      return;
    }
    if (visited.has(itemId)) {
      return;
    }

    visited.add(itemId);
    active.add(itemId);
    stack.push(itemId);

    const item = recordsById.get(itemId);
    for (const dependencyId of item?.depends_on || []) {
      if (recordsById.has(dependencyId)) {
        visit(dependencyId);
      }
    }

    stack.pop();
    active.delete(itemId);
  }

  for (const item of records) {
    visit(item.id);
  }

  return cycles;
}

export function validateVerificationFile(repoRoot, item) {
  if (!(item.kind === "TASK" || item.kind === "BUG")) {
    return [];
  }

  const issues = [];
  if (typeof item.verification_path !== "string" || !item.verification_path) {
    issues.push(createIssue("missing_verification_path", `Item ${item.id} is missing verification_path.`, { item_id: item.id }));
    return issues;
  }

  const verificationPath = toFilesystemPath(repoRoot, item.verification_path);
  if (!fs.existsSync(verificationPath)) {
    issues.push(createIssue("missing_verification_file", `Item ${item.id} is missing verification.md.`, { item_id: item.id }));
    return issues;
  }

  const markdown = fs.readFileSync(verificationPath, "utf8");
  if (!markdown.trim()) {
    issues.push(createIssue("empty_verification_file", `Item ${item.id} verification.md is empty.`, { item_id: item.id }));
    return issues;
  }

  for (const heading of VERIFICATION_HEADINGS) {
    if (!markdown.includes(heading)) {
      issues.push(createIssue("missing_verification_heading", `Item ${item.id} verification.md is missing heading ${heading}.`, { item_id: item.id, heading }));
    }
  }

  const unresolvedMatch = markdown.match(/## Unresolved Gaps\n([\s\S]*)$/);
  if (!unresolvedMatch || !unresolvedMatch[1].trim()) {
    issues.push(createIssue("missing_unresolved_gaps_content", `Item ${item.id} verification.md must state unresolved gaps or None..`, { item_id: item.id }));
  }

  return issues;
}

function validateMarkdownFiles(repoRoot, item, issues) {
  const contentPath = toFilesystemPath(repoRoot, item.content_path);
  if (!fs.existsSync(contentPath)) {
    issues.push(createIssue("missing_content_file", `Item ${item.id} is missing README.md at ${item.content_path}.`, { item_id: item.id }));
  } else {
    const contentText = fs.readFileSync(contentPath, "utf8");
    validateFrontmatterContract(item, contentText, "README.md", issues);
  }

  if (item.kind === "TASK" || item.kind === "BUG") {
    const verificationIssues = validateVerificationFile(repoRoot, item);
    issues.push(...verificationIssues);

    const verificationPath = toFilesystemPath(repoRoot, item.verification_path);
    if (fs.existsSync(verificationPath)) {
      validateFrontmatterContract(item, fs.readFileSync(verificationPath, "utf8"), "verification.md", issues);
    }
  }
}

export function collectGraphIssues(records, options = {}) {
  const issues = [];
  const items = (records || []).map((item) => cloneItemRecord(item));
  const idCounts = new Map();

  for (const item of items) {
    idCounts.set(item.id, (idCounts.get(item.id) || 0) + 1);
  }

  const recordsById = new Map(items.map((item) => [item.id, item]));
  for (const item of items) {
    validateRequiredFields(item, issues);
    validateScalarFields(item, issues);
    validateHierarchy(item, recordsById, issues);
    validateDependencies(item, recordsById, issues);

    if ((idCounts.get(item.id) || 0) > 1) {
      issues.push(createIssue("duplicate_id", `Duplicate id detected for ${item.id}.`, { item_id: item.id }));
    }

    try {
      assertSafeRelativeWorkPath(item.content_path);
      if (item.verification_path) {
        assertSafeRelativeWorkPath(item.verification_path);
      }
    } catch (error) {
      issues.push(createIssue("unsafe_path", error.message, { item_id: item.id }));
    }
  }

  const cycles = findDependencyCycles(items);
  for (const cycle of cycles) {
    issues.push(createIssue("dependency_cycle", `Dependency cycle detected: ${cycle.join(" -> ")}`, { cycle }));
  }

  try {
    const expectedItems = applyCanonicalPaths(items);
    const expectedById = new Map(expectedItems.map((item) => [item.id, item]));
    for (const item of items) {
      const expected = expectedById.get(item.id);
      if (!expected) {
        continue;
      }
      if (item.content_path !== expected.content_path) {
        issues.push(createIssue("content_path_drift", `Item ${item.id} content_path drifted from canonical path.`, {
          item_id: item.id,
          actual: item.content_path,
          expected: expected.content_path,
        }));
      }
      if ((item.verification_path || null) !== (expected.verification_path || null)) {
        issues.push(createIssue("verification_path_drift", `Item ${item.id} verification_path drifted from canonical path.`, {
          item_id: item.id,
          actual: item.verification_path,
          expected: expected.verification_path,
        }));
      }
    }
  } catch (error) {
    issues.push(createIssue("path_derivation_failed", error.message));
  }

  if (options.repoRoot) {
    for (const item of items) {
      validateMarkdownFiles(options.repoRoot, item, issues);
    }
  }

  return issues;
}

export function assertValidGraph(records, options = {}) {
  const issues = collectGraphIssues(records, options);
  if (issues.length > 0) {
    const error = new Error(`Workgraph validation failed with ${issues.length} issue(s).`);
    error.issues = issues;
    throw error;
  }
  return records;
}

export function assertValidGenericStatusTransition(previousItem, nextStatus) {
  if (!STATUS_VALUES.includes(nextStatus)) {
    throw new Error(`Unsupported status: ${nextStatus}`);
  }
  if (nextStatus === "CLOSED") {
    throw new Error("Use pulse.mjs workgraph close instead of update --status CLOSED.");
  }
  if (previousItem.status === "CLOSED") {
    throw new Error("Use pulse.mjs workgraph reopen to leave CLOSED state.");
  }

  const allowed = GENERIC_STATUS_TRANSITIONS[previousItem.status] || new Set();
  if (!allowed.has(nextStatus)) {
    throw new Error(`Invalid status transition ${previousItem.status} -> ${nextStatus}.`);
  }
}

export function assertItemClosable(item, records, repoRoot) {
  const openChildren = (records || []).filter(
    (candidate) => candidate.parent_id === item.id && candidate.status !== "CLOSED",
  );
  if (openChildren.length > 0) {
    throw new Error(`Item ${item.id} cannot close while children remain open: ${openChildren.map((child) => child.id).join(", ")}`);
  }

  const verificationIssues = validateVerificationFile(repoRoot, item);
  if (item.kind === "TASK" || item.kind === "BUG") {
    const verificationPath = toFilesystemPath(repoRoot, item.verification_path);
    if (fs.existsSync(verificationPath)) {
      validateFrontmatterContract(item, fs.readFileSync(verificationPath, "utf8"), "verification.md", verificationIssues);
    }
  }

  if (verificationIssues.length > 0) {
    throw new Error(verificationIssues.map((issue) => issue.message).join(" "));
  }
}

export function assertItemReopenable(item) {
  if (item.status !== "CLOSED") {
    throw new Error(`Only CLOSED items can be reopened. ${item.id} is ${item.status}.`);
  }
}
