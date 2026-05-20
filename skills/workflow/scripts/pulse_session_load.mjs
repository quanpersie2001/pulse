#!/usr/bin/env node

/**
 * Purpose: Build manifest-first Pulse session resume context.
 * Caller/flow: Used by onboard/status flows to restore safe next command context.
 * Reads/Writes: Reads runtime state, handoffs, reservations, and workgraph pointers; no writes.
 * CLI args: --repo-root, --resume-owner, --json, --help.
 * Ownership: Advisory loader only; does not mutate runtime or workgraph.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs.
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resolveRepoRoot } from "./pulse_paths.mjs";

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

function readJsonIfExistsSafe(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

function parseJsonSafe(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function normalizeWorkflowCommand(value) {
  const normalized = firstNonEmptyString(value);
  if (!normalized) {
    return "";
  }
  if (normalized.startsWith("pulse:workflow ")) {
    return normalized === "pulse:workflow onboard" ? "pulse:workflow use" : normalized;
  }
  if (normalized === "onboard") {
    return "pulse:workflow use";
  }
  if (normalized.startsWith("pulse:")) {
    return "";
  }
  return `pulse:workflow ${normalized}`;
}

function isSafeSessionRelativePath(relativePath) {
  const candidate = String(relativePath || "").replace(/\\/g, "/").trim();
  if (!candidate || candidate.startsWith("/") || candidate.includes("..") || candidate.includes("%2e%2e") || candidate.includes("%2E%2E")) {
    return false;
  }
  const normalized = path.posix.normalize(candidate);
  if (normalized !== candidate) {
    return false;
  }
  return (
    normalized === "AGENTS.md" ||
    normalized.startsWith(".pulse/runtime/handoffs/") ||
    normalized.startsWith(".pulse/memory/") ||
    normalized.startsWith("works/") ||
    normalized.startsWith("docs/")
  );
}

function resolveSafeSessionPath(repoRoot, relativePath) {
  const candidate = String(relativePath || "").replace(/\\/g, "/").trim();
  if (!isSafeSessionRelativePath(candidate)) {
    return null;
  }
  const absolute = path.resolve(repoRoot, ...candidate.split("/"));
  const root = path.resolve(repoRoot);
  if (absolute !== root && !absolute.startsWith(`${root}${path.sep}`)) {
    return null;
  }
  return { relative: candidate, absolute };
}

function uniqueStrings(values) {
  return [...new Set((values || []).filter((value) => typeof value === "string" && value.trim()).map((value) => value.trim()))];
}

function readWorkgraphItemsSafe(repoRoot) {
  const itemsPath = path.join(repoRoot, ".pulse", "workgraph", "items.jsonl");
  if (!fs.existsSync(itemsPath)) {
    return [];
  }
  const items = [];
  for (const line of fs.readFileSync(itemsPath, "utf8").split("\n")) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const parsed = parseJsonSafe(trimmed);
    if (parsed) {
      items.push(parsed);
    }
  }
  return items;
}

function findWorkgraphItem(items, id) {
  if (typeof id !== "string" || !id.trim()) {
    return null;
  }
  return items.find((item) => item?.id === id.trim()) || null;
}

function collectWorkgraphContext(repoRoot, activeContext) {
  const items = readWorkgraphItemsSafe(repoRoot);
  const requestedIds = uniqueStrings([
    activeContext.active_epic_id,
    activeContext.active_story_id,
    activeContext.active_item_id,
  ]);
  const loaded = [];
  const readFirst = [];
  const missing = [];

  for (const id of requestedIds) {
    const item = findWorkgraphItem(items, id);
    if (!item) {
      missing.push(`.pulse/workgraph/items.jsonl#${id}`);
      continue;
    }
    loaded.push({
      id: item.id,
      kind: item.kind || "",
      title: item.title || "",
      status: item.status || "",
      owner: item.owner || null,
      content_path: item.content_path || "",
      verification_path: item.verification_path || null,
    });
    if (item.content_path) {
      readFirst.push(item.content_path);
    }
    if (item.verification_path) {
      readFirst.push(item.verification_path);
    }
  }

  return { items: loaded, read_first: uniqueStrings(readFirst), missing_items: missing };
}

function validateSessionFilePointers(repoRoot, paths) {
  const missing = [];
  const rejected = [];

  for (const relativePath of uniqueStrings(paths)) {
    const resolved = resolveSafeSessionPath(repoRoot, relativePath);
    if (!resolved) {
      rejected.push(relativePath);
      continue;
    }
    if (!fs.existsSync(resolved.absolute) || !fs.statSync(resolved.absolute).isFile()) {
      missing.push(resolved.relative);
    }
  }

  return { missing_files: missing, rejected_paths: rejected };
}

function mapResumeOptions(activeHandoffs) {
  return activeHandoffs.map((entry) => ({
    owner_id: typeof entry?.owner_id === "string" ? entry.owner_id : "",
    owner_type: typeof entry?.owner_type === "string" ? entry.owner_type : "",
    surface: typeof entry?.surface === "string" ? entry.surface : "",
    active_command: typeof entry?.active_command === "string" ? entry.active_command : null,
    active_epic_id: typeof entry?.active_epic_id === "string" ? entry.active_epic_id : null,
    active_story_id: typeof entry?.active_story_id === "string" ? entry.active_story_id : null,
    active_item_id: typeof entry?.active_item_id === "string" ? entry.active_item_id : null,
    phase: typeof entry?.phase === "string" ? entry.phase : "",
    summary: typeof entry?.summary === "string" ? entry.summary : "",
    path: typeof entry?.path === "string" ? entry.path : "",
    next_action: typeof entry?.next_action === "string" ? entry.next_action : "",
  }));
}

const SCRIPT_PATH = fileURLToPath(import.meta.url);

export function buildSessionLoad(repoRoot, options = {}) {
  const state = readJsonIfExistsSafe(path.join(repoRoot, ".pulse", "runtime", "state.json")) || {};
  const toolingStatus = readJsonIfExistsSafe(path.join(repoRoot, ".pulse", "runtime", "tooling-status.json")) || {};
  const handoffManifest = readJsonIfExistsSafe(path.join(repoRoot, ".pulse", "runtime", "handoffs", "manifest.json")) || {};
  const reservations = readJsonIfExistsSafe(path.join(repoRoot, ".pulse", "runtime", "reservations.json")) || {};

  const activeHandoffs = Array.isArray(handoffManifest.active) ? handoffManifest.active : [];
  const activeReservations = Array.isArray(reservations.reservations)
    ? reservations.reservations.filter((entry) => entry?.status === "active")
    : [];
  const resumeOptions = mapResumeOptions(activeHandoffs);

  const selectedEntry = options.resumeOwner
    ? activeHandoffs.find((entry) => entry?.owner_id === options.resumeOwner) || null
    : activeHandoffs.length === 1
      ? activeHandoffs[0]
      : null;
  const requiresSelection = activeHandoffs.length > 1 && !selectedEntry;
  const selectedHandoffPath = selectedEntry?.path || "";
  const selectedHandoffResolved = selectedHandoffPath ? resolveSafeSessionPath(repoRoot, selectedHandoffPath) : null;
  const selectedHandoff = selectedHandoffResolved && fs.existsSync(selectedHandoffResolved.absolute)
    ? readJsonIfExistsSafe(selectedHandoffResolved.absolute)
    : null;

  const activeContext = {
    active_command: firstNonEmptyString(selectedHandoff?.active_command, selectedEntry?.active_command, state.active_command),
    active_epic_id: firstNonEmptyString(selectedHandoff?.active_epic_id, selectedEntry?.active_epic_id, state.active_epic_id) || null,
    active_story_id: firstNonEmptyString(selectedHandoff?.active_story_id, selectedEntry?.active_story_id, state.active_story_id) || null,
    active_item_id: firstNonEmptyString(selectedHandoff?.active_item_id, selectedEntry?.active_item_id, state.active_item_id) || null,
  };
  const activeItemIds = [activeContext.active_epic_id, activeContext.active_story_id, activeContext.active_item_id]
    .filter((value) => typeof value === "string" && value.trim());
  const inProgressItems = activeItemIds.length > 0 ? 1 : 0;
  const workgraphContext = collectWorkgraphContext(repoRoot, activeContext);
  const handoffReadFirst = Array.isArray(selectedHandoff?.read_first) ? selectedHandoff.read_first : [];
  const memoryHooks = selectedHandoff?.memory_hooks && typeof selectedHandoff.memory_hooks === "object" ? selectedHandoff.memory_hooks : {};
  const memoryReadFirst = [
    memoryHooks.critical_patterns,
    ...(Array.isArray(memoryHooks.learnings) ? memoryHooks.learnings : []),
    ...(Array.isArray(memoryHooks.corrections) ? memoryHooks.corrections : []),
    ...(Array.isArray(memoryHooks.ratchet) ? memoryHooks.ratchet : []),
  ];
  const readFirst = uniqueStrings([
    selectedHandoffPath,
    ...handoffReadFirst,
    ...workgraphContext.read_first,
    ...memoryReadFirst,
  ]);
  const pointerStatus = validateSessionFilePointers(repoRoot, readFirst);
  const selectedHandoffMissing = selectedHandoffPath && (!selectedHandoffResolved || !selectedHandoff)
    ? [selectedHandoffPath]
    : [];
  const conflicts = [
    ...workgraphContext.missing_items,
    ...selectedHandoffMissing.map((value) => `${value} missing or unreadable`),
  ];
  const posture = conflicts.length > 0
    ? "conflicted"
    : activeReservations.length > 0 || activeContext.active_epic_id || activeContext.active_story_id || activeContext.active_item_id
      ? "active"
      : activeHandoffs.length > 0
        ? "resumable"
        : "fresh";
  const summary = firstNonEmptyString(
    selectedHandoff?.summary,
    selectedHandoff?.handoff_summary,
    selectedEntry?.summary,
    posture === "fresh" ? "No active Pulse session was found." : "Pulse session context was restored from runtime pointers.",
  );
  const nextAction = firstNonEmptyString(selectedHandoff?.next_action, selectedEntry?.next_action);
  const nextCommand =
    normalizeWorkflowCommand(selectedHandoff?.next_command) ||
    normalizeWorkflowCommand(selectedEntry?.next_command) ||
    normalizeWorkflowCommand(state.next_command_recommended) ||
    normalizeWorkflowCommand(state.next_command) ||
    normalizeWorkflowCommand(toolingStatus.next_command_recommended) ||
    normalizeWorkflowCommand(toolingStatus.next_command) ||
    normalizeWorkflowCommand(state.next_skill_recommended) ||
    normalizeWorkflowCommand(toolingStatus.next_skill) ||
    "pulse:workflow explore";
  const scoutFindings = [];
  if (activeHandoffs.length > 0) {
    scoutFindings.push(`Detected ${activeHandoffs.length} active handoff entries from manifest.`);
    scoutFindings.push("Resume context is manifest-first; handoff entries are advisory until the operator chooses a route.");
    scoutFindings.push("No auto-execution is performed during use; continue only via an explicit pulse:workflow command.");
  }
  if (activeReservations.length > 0) {
    scoutFindings.push(`Detected ${activeReservations.length} active runtime reservations.`);
  }
  if (activeContext.active_command) {
    scoutFindings.push(`Active workflow command detected: ${activeContext.active_command}.`);
  }
  if (activeItemIds.length > 0) {
    scoutFindings.push(`Active work item context detected: ${activeItemIds.join(" / ")}.`);
  }
  if (activeHandoffs.length > 0) {
    scoutFindings.push("Recommended read order: handoff manifest, selected owner handoff, runtime state, then the next workflow command contract.");
  }

  return {
    posture,
    in_progress_items: inProgressItems,
    open_reservations: activeReservations.length,
    scout_findings: scoutFindings,
    requires_selection: requiresSelection,
    selected_handoff: selectedEntry
      ? {
          owner_id: selectedEntry.owner_id || "",
          path: selectedHandoffPath,
        }
      : null,
    resume_options: resumeOptions,
    active_context: activeContext,
    workgraph_items: workgraphContext.items,
    read_first: readFirst,
    missing_files: [...pointerStatus.missing_files, ...selectedHandoffMissing],
    rejected_paths: pointerStatus.rejected_paths,
    conflicts,
    summary,
    next_action: nextAction,
    next_command: nextCommand,
  };
}

function parseCliArgs(argv) {
  const args = {
    repoRoot: undefined,
    resumeOwner: "",
    json: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repo-root") {
      args.repoRoot = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith("--repo-root=")) {
      args.repoRoot = arg.slice("--repo-root=".length);
      continue;
    }
    if (arg === "--resume-owner") {
      args.resumeOwner = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--resume-owner=")) {
      args.resumeOwner = arg.slice("--resume-owner=".length);
      continue;
    }
    if (arg === "--json") {
      args.json = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage: pulse_session_load.mjs [--repo-root <path>] [--resume-owner <owner_id>] [--json]",
          "",
          "Loads Pulse session context from runtime pointers.",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

export function main(argv = process.argv.slice(2), env = process.env, cwd = process.cwd()) {
  const args = parseCliArgs(argv);
  const repoRoot = resolveRepoRoot({ explicitRoot: args.repoRoot, env, cwd });
  const payload = buildSessionLoad(repoRoot, { resumeOwner: args.resumeOwner });

  if (args.json) {
    process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
  } else {
    process.stdout.write(`${payload.summary}\n`);
    process.stdout.write(`posture: ${payload.posture}\n`);
    process.stdout.write(`next_command: ${payload.next_command}\n`);
  }

  return 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  process.exitCode = main();
}
