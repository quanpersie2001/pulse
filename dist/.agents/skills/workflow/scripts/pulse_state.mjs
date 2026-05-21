#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { getPulsePaths, resolveRepoRoot as resolveRepoRootFromPaths } from "./pulse_paths.mjs";

export const STATE_SCHEMA_VERSION = "1.0";

function utcNow() {
  return new Date().toISOString();
}

function ensureParent(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

export function fileTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
}

export function readJsonIfExists(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

export function resolveRepoRoot(explicitRoot, startFrom = process.cwd(), env = process.env) {
  return resolveRepoRootFromPaths({
    explicitRoot,
    cwd: startFrom,
    env,
  });
}

export function buildDefaultState(overrides = {}) {
  const session = overrides.session && typeof overrides.session === "object" && !Array.isArray(overrides.session)
    ? overrides.session
    : {};

  return {
    schema_version: STATE_SCHEMA_VERSION,
    phase: typeof overrides.phase === "string" && overrides.phase ? overrides.phase : "idle",
    status: typeof overrides.status === "string" ? overrides.status : "",
    active_skill:
      typeof overrides.active_skill === "string" ? overrides.active_skill : "pulse:workflow",
    active_command: typeof overrides.active_command === "string" ? overrides.active_command : "",
    active_epic_id: typeof overrides.active_epic_id === "string" ? overrides.active_epic_id : null,
    active_story_id: typeof overrides.active_story_id === "string" ? overrides.active_story_id : null,
    active_item_id: typeof overrides.active_item_id === "string" ? overrides.active_item_id : null,
    active_feature: typeof overrides.active_feature === "string" ? overrides.active_feature : "",
    gate: typeof overrides.gate === "string" ? overrides.gate : "",
    gate_status: typeof overrides.gate_status === "string" ? overrides.gate_status : "",
    requested_mode: typeof overrides.requested_mode === "string" ? overrides.requested_mode : "",
    recommended_mode: typeof overrides.recommended_mode === "string" ? overrides.recommended_mode : "",
    next_action: typeof overrides.next_action === "string" ? overrides.next_action : "",
    next_command: typeof overrides.next_command === "string" ? overrides.next_command : "",
    next_command_recommended:
      typeof overrides.next_command_recommended === "string" ? overrides.next_command_recommended : "",
    next_skill_recommended:
      typeof overrides.next_skill_recommended === "string" ? overrides.next_skill_recommended : "",
    session: {
      posture: typeof session.posture === "string" ? session.posture : "",
      scout_findings: Array.isArray(session.scout_findings) ? session.scout_findings : [],
      resume_options: Array.isArray(session.resume_options) ? session.resume_options : [],
    },
    session_load:
      overrides.session_load && typeof overrides.session_load === "object" && !Array.isArray(overrides.session_load)
        ? overrides.session_load
        : null,
    tooling_status:
      typeof overrides.tooling_status === "string" && overrides.tooling_status
        ? overrides.tooling_status
        : ".pulse/runtime/tooling-status.json",
    handoff_manifest:
      typeof overrides.handoff_manifest === "string" && overrides.handoff_manifest
        ? overrides.handoff_manifest
        : ".pulse/runtime/handoffs/manifest.json",
    last_updated:
      typeof overrides.last_updated === "string" && overrides.last_updated
        ? overrides.last_updated
        : utcNow(),
  };
}

export function normalizePulseState(state) {
  if (!state || typeof state !== "object" || Array.isArray(state)) {
    return buildDefaultState();
  }
  return buildDefaultState(state);
}

export function getPulseStatePaths(repoRoot) {
  return getPulsePaths(repoRoot);
}

export function readPulseState(repoRoot) {
  const paths = getPulseStatePaths(repoRoot);
  return normalizePulseState(readJsonIfExists(paths.stateJson));
}

export function writePulseState(repoRoot, nextState) {
  const paths = getPulseStatePaths(repoRoot);
  const normalized = normalizePulseState(nextState);
  ensureParent(paths.stateJson);
  fs.writeFileSync(paths.stateJson, `${JSON.stringify(normalized, null, 2)}\n`, "utf8");
  return normalized;
}

export function parseLooseKeyValueMarkdown(text) {
  const parsed = {};
  for (const line of text.split("\n")) {
    const match = line.match(/^([A-Za-z][A-Za-z0-9 _/-]+):\s*(.+)$/);
    if (!match) {
      continue;
    }
    const key = match[1].trim().toLowerCase().replace(/[^a-z0-9]+/g, "_");
    parsed[key] = match[2].trim();
  }
  return parsed;
}

