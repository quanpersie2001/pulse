#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { getPulsePaths, resolveRepoRoot as resolveRepoRootFromPaths } from "./pulse_paths.mjs";
import { renderPulseStatus as renderPulseStatusImpl } from "./pulse_status_render.mjs";

export const STATE_SCHEMA_VERSION = "1.0";
export const CURRENT_FEATURE_SCHEMA_VERSION = "1.0";
export const RUNTIME_SNAPSHOT_SCHEMA_VERSION = "1.0";
export const RESERVATION_SCHEMA_VERSION = "1.0";

function utcNow() {
  return new Date().toISOString();
}

function ensureParent(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function fileTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
}

function readJsonIfExists(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

function parseTomlMcpServerNames(filePath) {
  if (!fs.existsSync(filePath)) {
    return [];
  }

  const source = fs.readFileSync(filePath, "utf8");
  const names = new Set();
  for (const pattern of [/^\s*\[mcp_servers\.([^\]]+)\]\s*$/gm, /^\s*\[mcp\.servers\.([^\]]+)\]\s*$/gm]) {
    for (const match of source.matchAll(pattern)) {
      names.add(match[1].trim().replace(/^['"]|['"]$/g, ""));
    }
  }
  return [...names];
}

function parseJsonMcpServerNames(filePath) {
  if (!fs.existsSync(filePath)) {
    return [];
  }

  try {
    const payload = JSON.parse(fs.readFileSync(filePath, "utf8"));
    return payload && typeof payload === "object" && !Array.isArray(payload) ? Object.keys(payload) : [];
  } catch {
    return [];
  }
}

function readGitNexusMcpSources(repoRoot) {
  const sources = [
    {
      key: "repo_codex_config",
      server_names: parseTomlMcpServerNames(path.join(repoRoot, ".codex", "config.toml")),
    },
    {
      key: "global_codex_config",
      server_names: parseTomlMcpServerNames(path.join(os.homedir(), ".codex", "config.toml")),
    },
    {
      key: "plugin_mcp_manifest",
      server_names: parseJsonMcpServerNames(path.join(repoRoot, ".mcp.json")),
    },
  ];

  return sources
    .filter((source) => source.server_names.includes("gitnexus"))
    .map((source) => source.key)
    .sort((left, right) => left.localeCompare(right));
}

function buildGitNexusRecommendedAction(configured, matchedSources) {
  if (configured) {
    return `GitNexus is configured in ${matchedSources.join(", ")} — use graph-backed discovery as supporting context, then confirm results with direct file reads.`;
  }

  return "GitNexus is not configured in known MCP sources — use grep/file inspection fallback, or add the gitnexus MCP server before graph-backed discovery.";
}

export async function readGitNexusReadiness(repoRoot) {
  const matchedSources = readGitNexusMcpSources(repoRoot);
  const configured = matchedSources.length > 0;

  return {
    configured,
    matched_sources: matchedSources,
    recommended_action: buildGitNexusRecommendedAction(configured, matchedSources),
  };
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

export function buildEmptyReservationStore() {
  return {
    schema_version: RESERVATION_SCHEMA_VERSION,
    updated_at: utcNow(),
    reservations: [],
  };
}

export function normalizeReservationStore(store) {
  if (!store || typeof store !== "object" || Array.isArray(store)) {
    return buildEmptyReservationStore();
  }

  return {
    schema_version: RESERVATION_SCHEMA_VERSION,
    updated_at:
      typeof store.updated_at === "string" && store.updated_at ? store.updated_at : utcNow(),
    reservations: Array.isArray(store.reservations) ? store.reservations : [],
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

function summarizeProjectDocs(repoRoot, paths) {
  const projectDocs = readJsonIfExists(paths.projectDocs);
  const rootContextPath = "CONTEXT.md";
  const contextMapPath = "CONTEXT-MAP.md";
  const adrDirPath = "docs/adr";
  const hasRootContext = fs.existsSync(path.join(repoRoot, rootContextPath));
  const hasContextMap = fs.existsSync(path.join(repoRoot, contextMapPath));
  const hasAdrDir = fs.existsSync(path.join(repoRoot, adrDirPath));

  const mappedEntries = Array.isArray(projectDocs?.context?.entries)
    ? projectDocs.context.entries
        .filter((entry) => entry && typeof entry.path === "string" && entry.path)
        .map((entry) => ({
          id: typeof entry.id === "string" ? entry.id : "",
          path: normalizeSelector(entry.path),
        }))
    : [];

  const mappedMode = typeof projectDocs?.mode === "string" ? projectDocs.mode : "";
  const status = projectDocs
    ? (typeof projectDocs.status === "string" && projectDocs.status ? projectDocs.status : "mapped")
    : ((hasRootContext || hasContextMap || hasAdrDir) ? "detected" : "missing");
  const mode = mappedMode || (hasContextMap ? "multi-context" : (hasRootContext ? "single-context" : ""));
  const contextRoot = typeof projectDocs?.context?.root === "string" && projectDocs.context.root
    ? normalizeSelector(projectDocs.context.root)
    : (hasRootContext ? rootContextPath : "");
  const contextMap = typeof projectDocs?.context?.map === "string" && projectDocs.context.map
    ? normalizeSelector(projectDocs.context.map)
    : (hasContextMap ? contextMapPath : "");
  const adrDir = typeof projectDocs?.adrs?.dir === "string" && projectDocs.adrs.dir
    ? normalizeSelector(projectDocs.adrs.dir)
    : (hasAdrDir ? adrDirPath : "");
  const notes = Array.isArray(projectDocs?.notes)
    ? projectDocs.notes.filter((item) => typeof item === "string" && item.trim() !== "")
    : [];
  const warnings = [];

  if (projectDocs && !mode) {
    warnings.push("project-docs.json exists but mode is missing.");
  }
  if (projectDocs && mode === "single-context" && !contextRoot) {
    warnings.push("project-docs.json says single-context but no root CONTEXT.md is mapped.");
  }
  if (projectDocs && mode === "multi-context" && !contextMap && mappedEntries.length === 0) {
    warnings.push("project-docs.json says multi-context but no CONTEXT-MAP.md or context entries are mapped.");
  }
  if (!projectDocs && (hasRootContext || hasContextMap || hasAdrDir)) {
    warnings.push("Repo-level project docs were detected but .pulse/project-docs.json is missing.");
  }

  return {
    exists: Boolean(projectDocs),
    status,
    mode,
    mapping_path: projectDocs ? ".pulse/project-docs.json" : "",
    context: {
      root: contextRoot,
      map: contextMap,
      entries: mappedEntries,
    },
    adrs: {
      enabled: typeof projectDocs?.adrs?.enabled === "boolean" ? projectDocs.adrs.enabled : hasAdrDir,
      dir: adrDir,
      exists: adrDir ? fs.existsSync(path.join(repoRoot, adrDir)) : false,
    },
    notes,
    warnings,
  };
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

export function readReservationStore(repoRoot) {
  const paths = getPulseStatePaths(repoRoot);
  return normalizeReservationStore(readJsonIfExists(paths.reservations));
}

export function writeReservationStore(repoRoot, nextStore) {
  const paths = getPulseStatePaths(repoRoot);
  const normalized = normalizeReservationStore(nextStore);
  ensureParent(paths.reservations);
  fs.writeFileSync(paths.reservations, `${JSON.stringify(normalized, null, 2)}\n`, "utf8");
  return normalized;
}

export function ensureReservationStore(repoRoot) {
  const store = readReservationStore(repoRoot);
  return writeReservationStore(repoRoot, store);
}

function summarizeReservations(store) {
  const reservations = Array.isArray(store?.reservations) ? store.reservations : [];
  const active = reservations.filter((item) => item?.status === "active");
  const expired = reservations.filter((item) => item?.status === "expired");
  const released = reservations.filter((item) => item?.status === "released");

  return {
    exists: true,
    schema_version: typeof store?.schema_version === "string" ? store.schema_version : RESERVATION_SCHEMA_VERSION,
    updated_at: typeof store?.updated_at === "string" ? store.updated_at : "",
    total: reservations.length,
    active_count: active.length,
    expired_count: expired.length,
    released_count: released.length,
    active_agents: [...new Set(active.map((item) => item?.agent).filter(Boolean))].sort(),
    active_reservations: active,
  };
}

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

function normalizeFeaturePointer(value) {
  const normalized = typeof value === "string" ? value.trim() : "";
  return normalized === "(none)" ? "" : normalized;
}

function normalizeNextCommandSurface(value) {
  const normalized = firstNonEmptyString(value);
  if (!normalized) {
    return "";
  }

  const legacyMap = {
    "pulse:planning": "pulse:workflow plan",
    "pulse:validating": "pulse:workflow validate",
    "pulse:swarming": "pulse:workflow swarm",
    "pulse:executing": "pulse:workflow execute",
    "pulse:reviewing": "pulse:workflow review",
    "pulse:compounding": "pulse:workflow compound",
    "pulse:exploring": "pulse:workflow explore",
    "pulse:brainstorming": "pulse:workflow brainstorm",
    "pulse:using-pulse": "pulse:workflow use",
    "pulse:preflight": "pulse:workflow use",
    "pulse:workflow onboard": "pulse:workflow use",
  };

  if (legacyMap[normalized]) {
    return legacyMap[normalized];
  }

  if (normalized.startsWith("pulse:workflow")) {
    return normalized;
  }

  const validCommands = new Set([
    "use",
    "onboard",
    "explore",
    "brainstorm",
    "plan",
    "validate",
    "swarm",
    "execute",
    "review",
    "compound",
  ]);

  if (validCommands.has(normalized)) {
    return `pulse:workflow ${normalized}`;
  }

  return normalized;
}

function inferWorkShapeNextSkillRecommended(status) {
  const workShapeStatus = firstNonEmptyString(
    status.state_markdown?.work_shape_status,
    status.state_json?.work_shape_status,
    status.current_feature?.work_shape_status,
    status.runtime_snapshot?.work_shape_status,
  ).toLowerCase();
  const currentWorkStatus = firstNonEmptyString(
    status.state_markdown?.current_work_status,
    status.state_json?.current_work_status,
    status.current_feature?.current_work_status,
    status.runtime_snapshot?.current_work_status,
  ).toLowerCase();
  const feasibilityStatus = firstNonEmptyString(
    status.state_markdown?.feasibility_status,
    status.state_json?.feasibility_status,
    status.current_feature?.feasibility_status,
    status.runtime_snapshot?.feasibility_status,
  ).toLowerCase();
  const reviewStatus = firstNonEmptyString(
    status.state_markdown?.review_status,
    status.state_json?.review_status,
    status.current_feature?.review_status,
    status.runtime_snapshot?.review_status,
  ).toLowerCase();

  if (reviewStatus === "approved") {
    return "pulse:workflow compound";
  }

  if (feasibilityStatus === "approved" || feasibilityStatus === "ready") {
    const executionMode = firstNonEmptyString(
      status.tooling_status?.recommended_mode,
      status.runtime_snapshot?.recommended_mode,
      status.state_json?.recommended_mode,
    );
    if (executionMode === "swarm") {
      return "pulse:workflow swarm";
    }
    if (executionMode === "single-worker" || executionMode === "execution-only") {
      return "pulse:workflow execute";
    }
    return "";
  }

  if (workShapeStatus === "approved" && ["prepared", "ready", "validated"].includes(currentWorkStatus)) {
    return "pulse:workflow validate";
  }

  if (workShapeStatus === "approved") {
    return "pulse:workflow plan";
  }

  return "";
}

function inferGateNextSkillRecommended(status, gate, gateStatus) {
  const explicit = firstNonEmptyString(
    status.state_markdown?.next_skill_recommended,
    status.state_json?.next_skill_recommended,
    status.current_feature?.next_skill_recommended,
    status.runtime_snapshot?.next_skill_recommended,
  );
  if (explicit) {
    return normalizeNextCommandSurface(explicit);
  }

  const workShapeNext = inferWorkShapeNextSkillRecommended(status);
  if (workShapeNext) {
    return workShapeNext;
  }

  if (gateStatus !== "approved") {
    return "";
  }

  if (gate === "GATE 1" || gate === "GATE 2") {
    return "pulse:workflow plan";
  }
  if (gate === "GATE 3") {
    const executionMode = firstNonEmptyString(
      status.tooling_status?.recommended_mode,
      status.runtime_snapshot?.recommended_mode,
      status.state_json?.recommended_mode,
    );
    if (executionMode === "swarm") {
      return "pulse:workflow swarm";
    }
    if (executionMode === "single-worker") {
      return "pulse:workflow execute";
    }
    return "";
  }
  if (gate === "GATE 4") {
    return "pulse:workflow compound";
  }
  return "";
}

function inferGateNextAction(status, gateStatus, nextSkillRecommended) {
  const explicit = firstNonEmptyString(
    status.state_markdown?.next_action,
    status.state_json?.next_action,
    status.current_feature?.next_action,
    status.runtime_snapshot?.next_action,
  );
  if (explicit) {
    return explicit;
  }
  if (gateStatus === "approved" && nextSkillRecommended) {
    return "manual_invoke";
  }
  return "";
}

function buildCurrentFeatureRecord(status) {
  const featureKey = firstNonEmptyString(
    normalizeFeaturePointer(status.state_json?.active_feature),
    normalizeFeaturePointer(status.state_markdown?.focus),
    normalizeFeaturePointer(status.current_feature?.feature_key),
  );
  const phase = firstNonEmptyString(
    status.state_json?.phase,
    status.state_markdown?.phase,
    status.runtime_snapshot?.phase,
    status.current_feature?.phase,
    featureKey ? "idle" : "",
  );
  const gate = firstNonEmptyString(
    status.state_markdown?.gate,
    status.state_json?.gate,
    status.current_feature?.gate,
  );
  const gateStatus = firstNonEmptyString(
    status.state_markdown?.gate_status,
    status.state_json?.gate_status,
    status.current_feature?.gate_status,
    status.runtime_snapshot?.gate_status,
  );
  const workShapeStatus = firstNonEmptyString(
    status.state_markdown?.work_shape_status,
    status.state_json?.work_shape_status,
    status.current_feature?.work_shape_status,
    status.runtime_snapshot?.work_shape_status,
  );
  const shapeArtifact = firstNonEmptyString(
    status.state_markdown?.shape_artifact,
    status.state_json?.shape_artifact,
    status.current_feature?.shape_artifact,
    status.runtime_snapshot?.shape_artifact,
  );
  const currentWorkId = firstNonEmptyString(
    status.state_markdown?.current_work_id,
    status.state_json?.current_work_id,
    status.current_feature?.current_work_id,
    status.runtime_snapshot?.current_work_id,
  );
  const currentWorkStatus = firstNonEmptyString(
    status.state_markdown?.current_work_status,
    status.state_json?.current_work_status,
    status.current_feature?.current_work_status,
    status.runtime_snapshot?.current_work_status,
  );
  const feasibilityStatus = firstNonEmptyString(
    status.state_markdown?.feasibility_status,
    status.state_json?.feasibility_status,
    status.current_feature?.feasibility_status,
    status.runtime_snapshot?.feasibility_status,
  );
  const readinessStatus = firstNonEmptyString(
    status.state_markdown?.readiness_status,
    status.state_json?.readiness_status,
    status.current_feature?.readiness_status,
    status.runtime_snapshot?.readiness_status,
  );
  const reviewStatus = firstNonEmptyString(
    status.state_markdown?.review_status,
    status.state_json?.review_status,
    status.current_feature?.review_status,
    status.runtime_snapshot?.review_status,
  );
  const currentStatus = featureKey
    ? (status.current_feature?.status && status.current_feature.status !== "idle"
        ? status.current_feature.status
        : "active")
    : firstNonEmptyString(status.current_feature?.status, "idle");
  const nextSkillRecommended = inferGateNextSkillRecommended(status, gate, gateStatus);
  const nextAction = inferGateNextAction(status, gateStatus, nextSkillRecommended);

  return {
    schema_version: CURRENT_FEATURE_SCHEMA_VERSION,
    feature_key: featureKey,
    phase,
    gate,
    gate_status: gateStatus,
    work_shape_status: workShapeStatus,
    shape_artifact: shapeArtifact,
    current_work_id: currentWorkId,
    current_work_status: currentWorkStatus,
    feasibility_status: feasibilityStatus,
    readiness_status: readinessStatus,
    review_status: reviewStatus,
    status: currentStatus,
    next_action: nextAction,
    next_skill_recommended: nextSkillRecommended,
    updated_at: utcNow(),
  };
}

function buildRuntimeSnapshotRecord(status) {
  const source = {
    state_json: ".pulse/runtime/state.json",
    state_markdown: ".pulse/runtime/STATE.md",
  };
  const gate = firstNonEmptyString(
    status.current_feature?.gate,
    status.state_markdown?.gate,
    status.state_json?.gate,
  );
  const gateStatus = firstNonEmptyString(
    status.current_feature?.gate_status,
    status.state_markdown?.gate_status,
    status.state_json?.gate_status,
    status.runtime_snapshot?.gate_status,
  );
  const nextSkillRecommended = inferGateNextSkillRecommended(status, gate, gateStatus);
  const nextAction = inferGateNextAction(status, gateStatus, nextSkillRecommended);

  return {
    schema_version: RUNTIME_SNAPSHOT_SCHEMA_VERSION,
    active_feature: firstNonEmptyString(
      normalizeFeaturePointer(status.state_json?.active_feature),
      normalizeFeaturePointer(status.state_markdown?.focus),
      normalizeFeaturePointer(status.current_feature?.feature_key),
    ),
    active_skill: firstNonEmptyString(status.state_json?.active_skill, "pulse"),
    phase: firstNonEmptyString(
      status.current_feature?.phase,
      status.state_json?.phase,
      status.state_markdown?.phase,
      "idle",
    ),
    gate,
    gate_status: gateStatus,
    work_shape_status: firstNonEmptyString(
      status.current_feature?.work_shape_status,
      status.state_json?.work_shape_status,
      status.state_markdown?.work_shape_status,
    ),
    shape_artifact: firstNonEmptyString(
      status.current_feature?.shape_artifact,
      status.state_json?.shape_artifact,
      status.state_markdown?.shape_artifact,
    ),
    current_work_id: firstNonEmptyString(
      status.current_feature?.current_work_id,
      status.state_json?.current_work_id,
      status.state_markdown?.current_work_id,
    ),
    current_work_status: firstNonEmptyString(
      status.current_feature?.current_work_status,
      status.state_json?.current_work_status,
      status.state_markdown?.current_work_status,
    ),
    feasibility_status: firstNonEmptyString(
      status.current_feature?.feasibility_status,
      status.state_json?.feasibility_status,
      status.state_markdown?.feasibility_status,
    ),
    readiness_status: firstNonEmptyString(
      status.current_feature?.readiness_status,
      status.state_json?.readiness_status,
      status.state_markdown?.readiness_status,
    ),
    review_status: firstNonEmptyString(
      status.current_feature?.review_status,
      status.state_json?.review_status,
      status.state_markdown?.review_status,
    ),
    requested_mode: firstNonEmptyString(
      status.tooling_status?.requested_mode,
      status.state_json?.requested_mode,
    ),
    recommended_mode: firstNonEmptyString(
      status.tooling_status?.recommended_mode,
      status.state_json?.recommended_mode,
    ),
    next_action: nextAction,
    next_skill_recommended: nextSkillRecommended,
    updated_at: utcNow(),
    source,
  };
}

export function writeCurrentFeature(_repoRoot, nextCurrentFeature) {
  return summarizeCurrentFeature(nextCurrentFeature);
}

export function writeRuntimeSnapshot(_repoRoot, nextRuntimeSnapshot) {
  return summarizeRuntimeSnapshot(nextRuntimeSnapshot);
}

function deriveAndPersistRuntimeArtifacts(repoRoot) {
  const paths = getPulseStatePaths(repoRoot);
  const stateJson = readJsonIfExists(paths.stateJson);
  const stateMarkdownText = fileTextIfExists(paths.stateMarkdown);
  const stateMarkdown = parseLooseKeyValueMarkdown(stateMarkdownText);
  const toolingStatus = readJsonIfExists(paths.toolingStatus);
  const handoffManifest = readJsonIfExists(paths.handoffManifest);
  ensureReservationStore(repoRoot);

  const draftStatus = {
    repo_root: repoRoot,
    onboarding: {
      exists: Boolean(readJsonIfExists(paths.onboarding)),
      status: "",
      plugin_version: "",
    },
    tooling_status: {
      exists: Boolean(toolingStatus),
      status: typeof toolingStatus?.status === "string" ? toolingStatus.status : "",
      requested_mode: typeof toolingStatus?.requested_mode === "string" ? toolingStatus.requested_mode : "",
      recommended_mode: typeof toolingStatus?.recommended_mode === "string" ? toolingStatus.recommended_mode : "",
      next_skill: typeof toolingStatus?.next_skill === "string" ? toolingStatus.next_skill : "",
      blockers: Array.isArray(toolingStatus?.blockers) ? toolingStatus.blockers : [],
    },
    state_json: {
      exists: Boolean(stateJson),
      ...normalizePulseState(stateJson),
    },
    state_markdown: {
      exists: stateMarkdownText.trim() !== "",
      ...stateMarkdown,
    },
    current_feature: summarizeCurrentFeature(null),
    runtime_snapshot: summarizeRuntimeSnapshot(null),
    reservations: summarizeReservations(readReservationStore(repoRoot)),
    handoff_manifest: summarizeHandoffManifest(handoffManifest),
    gitnexus_readiness: null,
    critical_patterns_exists: fs.existsSync(paths.criticalPatterns),
    memory_recall: null,
    next_reads: [],
    recommended_actions: [],
  };

  const currentFeatureRecord = buildCurrentFeatureRecord(draftStatus);

  const runtimeSnapshotRecord = buildRuntimeSnapshotRecord({
    ...draftStatus,
    current_feature: summarizeCurrentFeature(currentFeatureRecord),
  });

  return {
    current_feature: currentFeatureRecord,
    runtime_snapshot: runtimeSnapshotRecord,
  };

}

export function syncPulseRuntimeArtifacts(repoRoot) {
  const normalizedRoot = resolveRepoRoot(repoRoot);
  return deriveAndPersistRuntimeArtifacts(normalizedRoot);
}

function parseLooseKeyValueMarkdown(text) {
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

export function deriveFeature(status) {
  if (status.current_feature?.feature_key) {
    return status.current_feature.feature_key;
  }
  if (status.state_json.active_feature) {
    return status.state_json.active_feature;
  }
  const focus = status.state_markdown.focus || "";
  return focus === "(none)" ? "" : focus;
}

function listDirectoryFiles(dirPath) {
  if (!fs.existsSync(dirPath)) {
    return [];
  }

  try {
    return fs.readdirSync(dirPath, { withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => entry.name)
      .sort((a, b) => a.localeCompare(b));
  } catch {
    return [];
  }
}

function normalizeSelector(selector) {
  return String(selector || "").trim().replaceAll("\\", "/");
}

function tokenizeRecallValue(value) {
  return String(value || "")
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean);
}

function stripDatedMemoryPrefix(fileName) {
  return fileName.toLowerCase().replace(/^\d{8}-/, "");
}

function parseInlineMetadataArray(value) {
  const normalized = String(value || "").trim();
  if (!normalized) {
    return [];
  }
  if (normalized.startsWith("[") && normalized.endsWith("]")) {
    return normalized
      .slice(1, -1)
      .split(",")
      .map((item) => item.trim().replace(/^['"]|['"]$/g, ""))
      .filter(Boolean);
  }
  return [normalized.replace(/^['"]|['"]$/g, "")].filter(Boolean);
}

function parseFrontmatterScalar(value) {
  const normalized = String(value || "").trim();
  if (!normalized) {
    return "";
  }
  if (normalized.startsWith("[") && normalized.endsWith("]")) {
    return parseInlineMetadataArray(normalized);
  }
  if (normalized === "true") {
    return true;
  }
  if (normalized === "false") {
    return false;
  }
  return normalized.replace(/^['"]|['"]$/g, "");
}

function parseMetadataFrontmatter(text) {
  if (!text.startsWith("---\n")) {
    return {};
  }

  const lines = text.split("\n");
  let endIndex = -1;
  for (let index = 1; index < lines.length; index += 1) {
    if (lines[index].trim() === "---") {
      endIndex = index;
      break;
    }
  }
  if (endIndex === -1) {
    return {};
  }

  const parsed = {};
  let activeArrayKey = "";
  for (const line of lines.slice(1, endIndex)) {
    const listMatch = line.match(/^\s*-\s*(.+)$/);
    if (activeArrayKey && listMatch) {
      parsed[activeArrayKey].push(parseFrontmatterScalar(listMatch[1]));
      continue;
    }

    const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!match) {
      activeArrayKey = "";
      continue;
    }

    const key = match[1].toLowerCase();
    const rawValue = match[2].trim();
    if (!rawValue) {
      parsed[key] = [];
      activeArrayKey = key;
      continue;
    }

    parsed[key] = parseFrontmatterScalar(rawValue);
    activeArrayKey = Array.isArray(parsed[key]) ? key : "";
  }
  return parsed;
}

function extractApplicableWhen(text) {
  const exactMatch = text.match(/^\*\*Applicable-when:\*\*\s*(.+)$/im);
  if (exactMatch) {
    return exactMatch[1].trim();
  }
  const fallbackMatch = text.match(/^applicable-when:\s*(.+)$/im);
  return fallbackMatch ? fallbackMatch[1].trim() : "";
}

function toMetadataArray(value) {
  if (Array.isArray(value)) {
    return value.map((item) => String(item || "").trim()).filter(Boolean);
  }
  return parseInlineMetadataArray(value || "");
}

function loadRecallEntryMetadata(repoRoot, relativePath) {
  const absolutePath = path.join(repoRoot, relativePath);
  const text = fileTextIfExists(absolutePath);
  const frontmatter = parseMetadataFrontmatter(text);
  const appliesWhen = firstNonEmptyString(
    frontmatter.applies_when,
    frontmatter["applicable-when"],
    extractApplicableWhen(text),
  );
  const scope = toMetadataArray(frontmatter.scope || frontmatter.files || []);
  const signals = toMetadataArray(frontmatter.signals || frontmatter.triggers || []);
  const tags = toMetadataArray(frontmatter.tags || []);
  const feature = firstNonEmptyString(frontmatter.feature);
  const severity = firstNonEmptyString(frontmatter.severity);
  const missingFields = [];

  if (!feature) {
    missingFields.push("feature");
  }
  if (tags.length === 0) {
    missingFields.push("tags");
  }
  if (!severity) {
    missingFields.push("severity");
  }
  if (!appliesWhen) {
    missingFields.push("applies_when");
  }
  if (scope.length === 0) {
    missingFields.push("scope");
  }
  if (signals.length === 0) {
    missingFields.push("signals");
  }

  return {
    feature,
    tags,
    severity,
    applies_when: appliesWhen,
    scope,
    signals,
    missing_fields: missingFields,
    has_metadata: text.startsWith("---\n"),
  };
}

function buildRecallSelectionContext(feature, status) {
  return {
    feature_tokens: [...new Set(tokenizeRecallValue(feature))],
    blocker_tokens: [...new Set(
      (Array.isArray(status.tooling_status?.blockers) ? status.tooling_status.blockers : [])
        .flatMap((item) => tokenizeRecallValue(item)),
    )],
    phase_tokens: [...new Set(tokenizeRecallValue(status.current_feature?.phase || status.state_json?.phase || ""))],
  };
}

function scoreMetadataTokens(tokens, haystacks, reasonPrefix, reasons, pointsPerMatch) {
  let score = 0;
  for (const token of tokens || []) {
    if (!token) {
      continue;
    }
    if (haystacks.some((value) => value.includes(token))) {
      reasons.push(`${reasonPrefix}:${token}`);
      score += pointsPerMatch;
    }
  }
  return score;
}

function scoreExactFieldMatch(tokens, values, reasonPrefix, reasons, pointsPerMatch) {
  let score = 0;
  const normalizedValues = (values || []).map((value) => tokenizeRecallValue(value).join(" ")).filter(Boolean);
  for (const token of tokens || []) {
    if (!token) {
      continue;
    }
    if (normalizedValues.some((value) => value === token || value.split(" ").includes(token))) {
      reasons.push(`${reasonPrefix}:${token}`);
      score += pointsPerMatch;
    }
  }
  return score;
}

function inferRecallSchemaStrength(metadata) {
  const requiredFields = ["feature", "tags", "severity", "applies_when", "scope", "signals"];
  const presentCount = requiredFields.filter((field) => {
    const value = metadata?.[field];
    return Array.isArray(value) ? value.length > 0 : Boolean(value);
  }).length;
  return {
    required_fields: requiredFields,
    present_fields: presentCount,
    is_strong: presentCount === requiredFields.length,
  };
}

function classifyRecallEntry(relativePath, selectionContext, repoRoot) {
  const fileName = stripDatedMemoryPrefix(path.basename(relativePath, path.extname(relativePath)));
  const metadata = loadRecallEntryMetadata(repoRoot, relativePath);
  const metadataHaystacks = [
    metadata.feature,
    ...metadata.tags,
    metadata.applies_when,
    ...metadata.scope,
    ...metadata.signals,
  ].flatMap((value) => tokenizeRecallValue(value));
  const fileNameTokens = tokenizeRecallValue(fileName);
  const reasons = [];
  let score = 0;

  score += scoreExactFieldMatch(selectionContext.feature_tokens, [metadata.feature], "feature", reasons, 8);
  score += scoreMetadataTokens(selectionContext.feature_tokens, metadataHaystacks, "feature", reasons, 6);
  score += scoreMetadataTokens(selectionContext.phase_tokens, metadata.tags, "phase-tag", reasons, 6);
  score += scoreMetadataTokens(selectionContext.phase_tokens, [metadata.applies_when], "phase", reasons, 5);
  score += scoreMetadataTokens(selectionContext.phase_tokens, metadata.scope, "scope", reasons, 4);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, metadata.signals, "signal", reasons, 7);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, [metadata.applies_when], "blocker", reasons, 5);
  score += scoreMetadataTokens(selectionContext.blocker_tokens, metadata.scope, "scope", reasons, 4);

  if (reasons.length === 0) {
    score += scoreMetadataTokens(selectionContext.feature_tokens, fileNameTokens, "feature", reasons, 2);
    score += scoreMetadataTokens(selectionContext.phase_tokens, fileNameTokens, "phase", reasons, 1);
    score += scoreMetadataTokens(selectionContext.blocker_tokens, fileNameTokens, "blocker", reasons, 1);
  }

  if (metadata.severity === "critical") {
    score += 2;
    reasons.push("severity:critical");
  }

  const schemaStrength = inferRecallSchemaStrength(metadata);
  if (schemaStrength.is_strong) {
    score += 2;
  }

  return {
    path: relativePath,
    reasons: [...new Set(reasons)],
    score,
    metadata: {
      ...metadata,
      schema_strength: schemaStrength,
    },
  };
}

function pickRelevantRecallEntries(pathsList, selectionContext, repoRoot) {
  const matched = [];
  const fallback = [];

  for (const relativePath of pathsList) {
    const entry = classifyRecallEntry(relativePath, selectionContext, repoRoot);
    if (entry.reasons.length > 0) {
      matched.push(entry);
    } else {
      fallback.push(entry);
    }
  }

  const sortEntries = (entries) => entries.sort((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    return left.path.localeCompare(right.path);
  });

  return matched.length > 0 ? sortEntries(matched).slice(0, 3) : sortEntries(fallback).slice(0, 3);
}

function getFileSizeSafe(filePath) {
  try {
    return fs.statSync(filePath).size;
  } catch {
    return 0;
  }
}

function getFileAgeDaysSafe(filePath) {
  try {
    const modifiedAt = fs.statSync(filePath).mtimeMs;
    const ageMs = Date.now() - modifiedAt;
    return Math.floor(ageMs / (24 * 60 * 60 * 1000));
  } catch {
    return null;
  }
}

function collectDuplicateMemorySlugs(relativePaths) {
  const counts = new Map();
  for (const relativePath of relativePaths) {
    const slug = stripDatedMemoryPrefix(path.basename(relativePath, path.extname(relativePath)));
    counts.set(slug, (counts.get(slug) || 0) + 1);
  }
  return [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([slug]) => slug)
    .sort((left, right) => left.localeCompare(right));
}

function buildMemoryHygiene(paths, selectedRecall, allRecallPaths) {
  const warnings = [];
  const criticalPatternsBytes = fs.existsSync(paths.criticalPatterns) ? getFileSizeSafe(paths.criticalPatterns) : 0;

  if (criticalPatternsBytes > 24 * 1024) {
    warnings.push("critical-patterns.md is getting large; review for compact, globally useful guidance only.");
  }

  const duplicateLearnings = collectDuplicateMemorySlugs(allRecallPaths.learnings);
  if (duplicateLearnings.length > 0) {
    warnings.push(`Possible duplicate learnings: ${duplicateLearnings.join(", ")}.`);
  }

  const duplicateCorrections = collectDuplicateMemorySlugs(allRecallPaths.corrections);
  if (duplicateCorrections.length > 0) {
    warnings.push(`Possible duplicate corrections: ${duplicateCorrections.join(", ")}.`);
  }

  const missingMetadataWarnings = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ].flatMap((entry) => {
    const missingFields = Array.isArray(entry.metadata?.missing_fields) ? entry.metadata.missing_fields : [];
    return missingFields.length > 0
      ? [`${entry.path} missing metadata: ${missingFields.join(", ")}`]
      : [];
  });
  if (missingMetadataWarnings.length > 0) {
    warnings.push(`Selected memory entries need stronger metadata: ${missingMetadataWarnings.join("; ")}.`);
  }

  const staleEntries = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ].flatMap((entry) => {
    const absolutePath = path.join(path.dirname(paths.agents), entry.path);
    const ageDays = getFileAgeDaysSafe(absolutePath);
    return ageDays !== null && ageDays > 180 ? [`${entry.path} (${ageDays}d old)`] : [];
  });
  if (staleEntries.length > 0) {
    warnings.push(`Selected memory entries may be stale: ${staleEntries.join(", ")}.`);
  }

  return {
    warnings,
    stats: {
      critical_patterns_bytes: criticalPatternsBytes,
      learnings_count: allRecallPaths.learnings.length,
      corrections_count: allRecallPaths.corrections.length,
      ratchet_count: allRecallPaths.ratchet.length,
    },
  };
}

function summarizeRecallReason(entry, fallbackReason) {
  if (!entry || !Array.isArray(entry.reasons) || entry.reasons.length === 0) {
    return fallbackReason;
  }

  return `matched ${entry.reasons.join(", ")}`;
}

function buildRecallPack(criticalPatternsPath, selectedRecall) {
  const pack = [];

  if (criticalPatternsPath) {
    pack.push({
      kind: "critical-patterns",
      path: criticalPatternsPath,
      reason: "global planning baseline",
    });
  }

  for (const entry of selectedRecall.corrections) {
    pack.push({
      kind: "correction",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted tactical guardrail"),
    });
  }
  for (const entry of selectedRecall.ratchet) {
    pack.push({
      kind: "ratchet",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted non-regression rule"),
    });
  }
  for (const entry of selectedRecall.learnings) {
    pack.push({
      kind: "learning",
      path: entry.path,
      reason: summarizeRecallReason(entry, "targeted prior lesson"),
    });
  }

  return pack;
}

function summarizeMemoryRecall(paths, feature, status) {
  const memoryRootExists = fs.existsSync(paths.memoryRoot);
  const criticalPatternsExists = fs.existsSync(paths.criticalPatterns);
  const repoRoot = path.dirname(paths.agents);
  const learnings = listDirectoryFiles(paths.memoryLearnings).map((fileName) => path.join(".pulse", "memory", "learnings", fileName));
  const corrections = listDirectoryFiles(paths.memoryCorrections).map((fileName) => path.join(".pulse", "memory", "corrections", fileName));
  const ratchet = listDirectoryFiles(paths.memoryRatchet).map((fileName) => path.join(".pulse", "memory", "ratchet", fileName));
  const selectionContext = buildRecallSelectionContext(feature, status);
  const selectedRecall = {
    learnings: pickRelevantRecallEntries(learnings, selectionContext, repoRoot),
    corrections: pickRelevantRecallEntries(corrections, selectionContext, repoRoot),
    ratchet: pickRelevantRecallEntries(ratchet, selectionContext, repoRoot),
  };
  const criticalPatternsPath = criticalPatternsExists ? ".pulse/memory/critical-patterns.md" : "";

  const selectedEntries = [
    ...selectedRecall.learnings,
    ...selectedRecall.corrections,
    ...selectedRecall.ratchet,
  ];
  const strongSchemaCount = selectedEntries.filter((entry) => entry.metadata?.schema_strength?.is_strong).length;

  return {
    root_exists: memoryRootExists,
    critical_patterns: criticalPatternsPath,
    learnings: selectedRecall.learnings.map((entry) => entry.path),
    corrections: selectedRecall.corrections.map((entry) => entry.path),
    ratchet: selectedRecall.ratchet.map((entry) => entry.path),
    selection_context: selectionContext,
    recall_pack: buildRecallPack(criticalPatternsPath, selectedRecall),
    schema_summary: {
      selected_entries: selectedEntries.length,
      strong_schema_entries: strongSchemaCount,
      metadata_first_ranking: true,
      fallback_to_filename_tokens: selectedEntries.some((entry) => entry.reasons.length === 0),
    },
    hygiene: buildMemoryHygiene(paths, selectedRecall, { learnings, corrections, ratchet }),
  };
}

function listHistoryFeatureFiles(repoRoot, feature) {
  const historyDir = path.join(repoRoot, "history", feature);
  if (!fs.existsSync(historyDir)) {
    return [];
  }

  const queue = [historyDir];
  const files = [];
  while (queue.length > 0) {
    const currentDir = queue.shift();
    let entries = [];
    try {
      entries = fs.readdirSync(currentDir, { withFileTypes: true });
    } catch {
      continue;
    }

    for (const entry of entries) {
      const absolutePath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        queue.push(absolutePath);
        continue;
      }
      const relativePath = path.relative(repoRoot, absolutePath).split(path.sep).join("/");
      files.push(relativePath);
    }
  }

  return files.sort((left, right) => left.localeCompare(right));
}

function summarizeHistoryLifecycle(repoRoot, feature) {
  const summary = {
    feature,
    exists: false,
    lifecycle_summary: "",
    approved_artifacts: [],
    verification: [],
    memory_promotions: [],
    lifecycle_signals: [],
    next_reads: [],
    self_sufficient: false,
  };

  if (!feature) {
    return summary;
  }

  const historyFiles = listHistoryFeatureFiles(repoRoot, feature);
  if (historyFiles.length === 0) {
    return summary;
  }

  summary.exists = true;
  const lifecycleSummaryPath = `history/${feature}/lifecycle-summary.md`;
  if (historyFiles.includes(lifecycleSummaryPath)) {
    summary.lifecycle_summary = lifecycleSummaryPath;
  }

  const requiredArtifacts = [
    `history/${feature}/CONTEXT.md`,
    `history/${feature}/approach.md`,
  ].filter((item) => historyFiles.includes(item));
  const shapeArtifacts = [
    `history/${feature}/phase-plan.md`,
    `history/${feature}/epic-map.md`,
    `history/${feature}/work-shape.md`,
    `history/${feature}/current-story-pack.md`,
  ].filter((item) => historyFiles.includes(item));
  const approvedArtifacts = [...requiredArtifacts, ...shapeArtifacts];
  summary.approved_artifacts = approvedArtifacts;

  const lifecycleSignals = historyFiles.filter((item) => (
    /phase-\d+-(contract|story-map)\.md$/u.test(item)
    || /\/(epic-map|work-shape|current-story-pack)\.md$/u.test(item)
  ));
  summary.lifecycle_signals = lifecycleSignals;

  summary.verification = historyFiles.filter((item) => item.startsWith(`history/${feature}/verification/`));
  summary.memory_promotions = [
    ...historyFiles.filter((item) => item.startsWith(`history/${feature}/memory/`)),
    ...historyFiles.filter((item) => item.endsWith("lifecycle-summary.md") && item !== lifecycleSummaryPath),
  ];

  summary.self_sufficient = Boolean(
    summary.lifecycle_summary
    && requiredArtifacts.length >= 2
    && shapeArtifacts.length > 0
    && lifecycleSignals.length > 0
    && summary.verification.length > 0,
  );

  summary.next_reads = [...new Set([
    summary.lifecycle_summary,
    ...approvedArtifacts,
    ...lifecycleSignals.slice(0, 4),
    ...summary.verification.slice(0, 4),
  ].filter(Boolean))];

  return summary;
}

function buildNextReads(status) {
  const reads = ["AGENTS.md", ".pulse/runtime/tooling-status.json"];

  if (status.project_docs?.mapping_path) {
    reads.push(status.project_docs.mapping_path);
  }
  if (status.project_docs?.context?.root) {
    reads.push(status.project_docs.context.root);
  }
  if (status.project_docs?.context?.map) {
    reads.push(status.project_docs.context.map);
  }
  for (const entry of status.project_docs?.context?.entries || []) {
    if (entry.path) {
      reads.push(entry.path);
    }
  }
  if (status.project_docs?.adrs?.dir) {
    reads.push(status.project_docs.adrs.dir);
  }

  if (status.state_json.exists) {
    reads.push(".pulse/runtime/state.json");
  }

  if (status.state_markdown.exists) {
    reads.push(".pulse/runtime/STATE.md");
  }

  if (status.handoff_manifest.exists) {
    reads.push(".pulse/runtime/handoffs/manifest.json");
  }
  for (const handoff of status.handoff_manifest?.active || []) {
    if (handoff.path) {
      reads.push(handoff.path);
    }
  }

  const feature = deriveFeature(status);
  if (feature) {
    reads.push(`history/${feature}/CONTEXT.md`);
  }

  for (const item of status.history_lifecycle?.next_reads || []) {
    reads.push(item);
  }


  for (const entry of status.memory_recall?.recall_pack || []) {
    if (entry.path) {
      reads.push(entry.path);
    }
  }

  return [...new Set(reads)];
}

function buildRecommendedActions(status) {
  if (!status.onboarding.exists) {
    return [
      "Run Pulse use before continuing.",
      "Run pulse:workflow use (or pulse_use.mjs --apply) to install repo-local assets and load session context.",
    ];
  }

  const recallPack = Array.isArray(status.memory_recall?.recall_pack)
    ? status.memory_recall.recall_pack
    : [];
  const hygieneWarnings = Array.isArray(status.memory_recall?.hygiene?.warnings)
    ? status.memory_recall.hygiene.warnings
    : [];
  const projectDocsWarnings = Array.isArray(status.project_docs?.warnings)
    ? status.project_docs.warnings
    : [];

  if (status.handoff_manifest.active_count > 0) {
    const actions = [
      "Surface the active handoffs to the user before resuming.",
      "Read the chosen handoff path, then reopen the active feature context.",
      "Use the generated handoff summary, resume briefing, and transfer block instead of ad hoc prose when presenting a resume path.",
    ];
    if (status.project_docs?.status === "mapped") {
      actions.push("When repo terminology or ownership boundaries matter, read the mapped project docs before going deeper into feature history.");
    } else if (status.project_docs?.status === "detected") {
      actions.push("Repo-level project docs were detected but are not mapped yet; map .pulse/project-docs.json before deeper planning.");
    } else {
      actions.push("If repo-wide terminology keeps drifting, propose a lightweight project-doc scaffold before deeper planning.");
    }

    if (status.history_lifecycle?.exists) {
      actions.push(
        status.history_lifecycle.self_sufficient
          ? "History plane has enough promoted lifecycle evidence for a durable audit pass without reopening live runtime state."
          : "History plane already exposes a promoted lifecycle trail, but live control state is still authoritative for resume and in-flight work.",
      );
    }
    if (recallPack.length > 0) {
      actions.push("Use the targeted recall pack to reopen only the most relevant critical patterns, corrections, ratchet rules, and learnings.");
    }
    if (hygieneWarnings.length > 0) {
      actions.push(`Memory hygiene warning: ${hygieneWarnings[0]}`);
    }
    if (projectDocsWarnings.length > 0) {
      actions.push(`Project docs warning: ${projectDocsWarnings[0]}`);
    }

    return actions;
  }

  const nextAction = firstNonEmptyString(
    status.runtime_snapshot?.next_action,
    status.current_feature?.next_action,
    status.state_json?.next_action,
  );
  const nextSkillRecommended = normalizeNextCommandSurface(firstNonEmptyString(
    status.runtime_snapshot?.next_skill_recommended,
    status.current_feature?.next_skill_recommended,
    status.state_json?.next_skill_recommended,
  ));

  if (nextAction === "manual_invoke" && nextSkillRecommended) {
    const actions = [`Gate cleared. Manually invoke ${nextSkillRecommended} when ready.`];
    actions.push("You can clear chat context or switch to a stronger model before invoking the recommended next skill.");
    if (status.project_docs?.status === "mapped") {
      actions.push("Read the mapped project docs when repo-level terminology, boundaries, or ADR context may affect the next decision.");
    } else if (status.project_docs?.status === "detected") {
      actions.push("Repo-level project docs were detected but .pulse/project-docs.json is missing; record the mapping before deeper planning.");
    } else {
      actions.push("If durable repo-level terminology or architecture context is missing, propose a lightweight project-doc scaffold before deeper planning.");
    }
    if (recallPack.length > 0) {
      actions.push("Before planning or debugging, consult the targeted recall pack instead of grepping the whole memory plane.");
    }
    if (hygieneWarnings.length > 0) {
      actions.push(`Memory hygiene warning: ${hygieneWarnings[0]}`);
    }
    if (projectDocsWarnings.length > 0) {
      actions.push(`Project docs warning: ${projectDocsWarnings[0]}`);
    }
    return actions;
  }

  if (status.tooling_status.next_skill) {
    const actions = [`Next command suggestion: ${normalizeNextCommandSurface(status.tooling_status.next_skill)}.`];
    if (status.project_docs?.status === "mapped") {
      actions.push("Read the mapped project docs when repo-level terminology, boundaries, or ADR context may affect the next decision.");
    } else if (status.project_docs?.status === "detected") {
      actions.push("Repo-level project docs were detected but .pulse/project-docs.json is missing; record the mapping before deeper planning.");
    } else {
      actions.push("If durable repo-level terminology or architecture context is missing, propose a lightweight project-doc scaffold before deeper planning.");
    }
    if (recallPack.length > 0) {
      actions.push("Before planning or debugging, consult the targeted recall pack instead of grepping the whole memory plane.");
    }
    if (hygieneWarnings.length > 0) {
      actions.push(`Memory hygiene warning: ${hygieneWarnings[0]}`);
    }
    if (projectDocsWarnings.length > 0) {
      actions.push(`Project docs warning: ${projectDocsWarnings[0]}`);
    }
    return actions;
  }

  const actions = [
    "Use this snapshot for fast orientation before deeper reads.",
    "If work is resuming, reopen the active feature context before planning or execution.",
  ];
  if (status.project_docs?.status === "mapped") {
    actions.push("Read the mapped project docs when repo-level terminology, boundaries, or ADR context may affect the next decision.");
  } else if (status.project_docs?.status === "detected") {
    actions.push("Repo-level project docs were detected but .pulse/project-docs.json is missing; record the mapping before deeper planning.");
  } else {
    actions.push("If durable repo-level terminology or architecture context is missing, propose a lightweight project-doc scaffold before deeper planning.");
  }

  if (status.history_lifecycle?.exists) {
    actions.push(
      status.history_lifecycle.self_sufficient
        ? "History plane has enough promoted lifecycle evidence for a durable audit pass without reopening live runtime state."
        : "History plane already exposes a promoted lifecycle trail, but live control state is still authoritative for resume and in-flight work.",
    );
  }
  if (recallPack.length > 0) {
    actions.push("Use the targeted recall pack to pull in the smallest relevant memory context before planning, debugging, or review.");
  }
  if (hygieneWarnings.length > 0) {
    actions.push(`Memory hygiene warning: ${hygieneWarnings[0]}`);
  }
  if (projectDocsWarnings.length > 0) {
    actions.push(`Project docs warning: ${projectDocsWarnings[0]}`);
  }

  return actions;
}

function summarizeCurrentFeature(currentFeature) {
  if (!currentFeature || typeof currentFeature !== "object" || Array.isArray(currentFeature)) {
    return {
      exists: false,
      feature_key: "",
      phase: "",
      gate: "",
      gate_status: "",
      work_shape_status: "",
      shape_artifact: "",
      current_work_id: "",
      current_work_status: "",
      feasibility_status: "",
      readiness_status: "",
      review_status: "",
      updated_at: "",
      status: "",
      next_action: "",
      next_skill_recommended: "",
    };
  }

  return {
    exists: true,
    feature_key: typeof currentFeature.feature_key === "string" ? currentFeature.feature_key : "",
    phase: typeof currentFeature.phase === "string" ? currentFeature.phase : "",
    gate: typeof currentFeature.gate === "string" ? currentFeature.gate : "",
    gate_status: typeof currentFeature.gate_status === "string" ? currentFeature.gate_status : "",
    work_shape_status: typeof currentFeature.work_shape_status === "string" ? currentFeature.work_shape_status : "",
    shape_artifact: typeof currentFeature.shape_artifact === "string" ? currentFeature.shape_artifact : "",
    current_work_id: typeof currentFeature.current_work_id === "string" ? currentFeature.current_work_id : "",
    current_work_status: typeof currentFeature.current_work_status === "string" ? currentFeature.current_work_status : "",
    feasibility_status: typeof currentFeature.feasibility_status === "string" ? currentFeature.feasibility_status : "",
    readiness_status: typeof currentFeature.readiness_status === "string" ? currentFeature.readiness_status : "",
    review_status: typeof currentFeature.review_status === "string" ? currentFeature.review_status : "",
    updated_at: typeof currentFeature.updated_at === "string" ? currentFeature.updated_at : "",
    status: typeof currentFeature.status === "string" ? currentFeature.status : "",
    next_action: typeof currentFeature.next_action === "string" ? currentFeature.next_action : "",
    next_skill_recommended: typeof currentFeature.next_skill_recommended === "string"
      ? currentFeature.next_skill_recommended
      : "",
  };
}

function summarizeRuntimeSnapshot(runtimeSnapshot) {
  if (!runtimeSnapshot || typeof runtimeSnapshot !== "object" || Array.isArray(runtimeSnapshot)) {
    return {
      exists: false,
      schema_version: "",
      active_feature: "",
      active_skill: "",
      phase: "",
      gate: "",
      gate_status: "",
      work_shape_status: "",
      shape_artifact: "",
      current_work_id: "",
      current_work_status: "",
      feasibility_status: "",
      readiness_status: "",
      review_status: "",
      requested_mode: "",
      recommended_mode: "",
      next_action: "",
      next_skill_recommended: "",
      updated_at: "",
      source: null,
    };
  }

  const source = runtimeSnapshot.source && typeof runtimeSnapshot.source === "object"
    ? {
        state_json: typeof runtimeSnapshot.source.state_json === "string"
          ? runtimeSnapshot.source.state_json
          : "",
        state_markdown: typeof runtimeSnapshot.source.state_markdown === "string"
          ? runtimeSnapshot.source.state_markdown
          : "",
        current_feature: typeof runtimeSnapshot.source.current_feature === "string"
          ? runtimeSnapshot.source.current_feature
          : "",
      }
    : null;

  return {
    exists: true,
    schema_version: typeof runtimeSnapshot.schema_version === "string"
      ? runtimeSnapshot.schema_version
      : "",
    active_feature: typeof runtimeSnapshot.active_feature === "string"
      ? runtimeSnapshot.active_feature
      : "",
    active_skill: typeof runtimeSnapshot.active_skill === "string" ? runtimeSnapshot.active_skill : "",
    phase: typeof runtimeSnapshot.phase === "string" ? runtimeSnapshot.phase : "",
    gate: typeof runtimeSnapshot.gate === "string" ? runtimeSnapshot.gate : "",
    gate_status: typeof runtimeSnapshot.gate_status === "string" ? runtimeSnapshot.gate_status : "",
    work_shape_status: typeof runtimeSnapshot.work_shape_status === "string" ? runtimeSnapshot.work_shape_status : "",
    shape_artifact: typeof runtimeSnapshot.shape_artifact === "string" ? runtimeSnapshot.shape_artifact : "",
    current_work_id: typeof runtimeSnapshot.current_work_id === "string" ? runtimeSnapshot.current_work_id : "",
    current_work_status: typeof runtimeSnapshot.current_work_status === "string" ? runtimeSnapshot.current_work_status : "",
    feasibility_status: typeof runtimeSnapshot.feasibility_status === "string" ? runtimeSnapshot.feasibility_status : "",
    readiness_status: typeof runtimeSnapshot.readiness_status === "string" ? runtimeSnapshot.readiness_status : "",
    review_status: typeof runtimeSnapshot.review_status === "string" ? runtimeSnapshot.review_status : "",
    requested_mode: typeof runtimeSnapshot.requested_mode === "string"
      ? runtimeSnapshot.requested_mode
      : "",
    recommended_mode: typeof runtimeSnapshot.recommended_mode === "string"
      ? runtimeSnapshot.recommended_mode
      : "",
    next_action: typeof runtimeSnapshot.next_action === "string" ? runtimeSnapshot.next_action : "",
    next_skill_recommended: typeof runtimeSnapshot.next_skill_recommended === "string"
      ? runtimeSnapshot.next_skill_recommended
      : "",
    updated_at: typeof runtimeSnapshot.updated_at === "string" ? runtimeSnapshot.updated_at : "",
    source,
  };
}

function renderHandoffSummary(entry) {
  const readFirst = Array.isArray(entry.read_first) ? entry.read_first.filter(Boolean) : [];
  return [
    "## Handoff Summary",
    `- Owner: ${entry.owner_id || "(unknown)"}`,
    `- Skill: ${entry.skill || "(unknown)"}`,
    `- Feature: ${entry.feature || "(none)"}`,
    `- Phase: ${entry.phase || "(none)"}`,
    `- Status: ${entry.status || "ready_to_resume"}`,
    `- Paused at: ${entry.paused_at || "(unknown)"}`,
    `- Reason: ${entry.reason || "(unspecified)"}`,
    `- Next action: ${entry.next_action || "(none)"}`,
    "- Read first:",
    ...(readFirst.length > 0 ? readFirst.map((item) => `  - ${item}`) : ["  - (none)"]),
    `- Summary: ${entry.summary || "(none)"}`,
  ].join("\n");
}

function renderResumeBriefing(entry) {
  const readFirst = Array.isArray(entry.read_first) ? entry.read_first.filter(Boolean) : [];
  return [
    "## Resume Briefing",
    `- Resuming: ${entry.owner_id || "(unknown)"} via ${entry.skill || "(unknown)"}`,
    `- Feature: ${entry.feature || "(none)"}`,
    `- Phase: ${entry.phase || "(none)"}`,
    `- Current state: ${entry.summary || "(none)"}`,
    `- Next action: ${entry.next_action || "(none)"}`,
    "- Required reads:",
    ...(readFirst.length > 0 ? readFirst.map((item) => `  - ${item}`) : ["  - (none)"]),
    "- Resume check: wait for explicit user confirmation before continuing.",
  ].join("\n");
}

function renderTransferBlock(entry) {
  const readFirst = Array.isArray(entry.read_first) ? entry.read_first.filter(Boolean).join(" | ") : "";
  return [
    "```text",
    "PULSE TRANSFER",
    `owner=${entry.owner_id || ""}`,
    `skill=${entry.skill || ""}`,
    `feature=${entry.feature || ""}`,
    `phase=${entry.phase || ""}`,
    `status=${entry.status || "ready_to_resume"}`,
    `paused_at=${entry.paused_at || ""}`,
    `reason=${entry.reason || ""}`,
    `next_action=${entry.next_action || ""}`,
    `read_first=${readFirst}`,
    `summary=${entry.summary || ""}`,
    `handoff_path=${entry.path || ""}`,
    "manifest_path=.pulse/runtime/handoffs/manifest.json",
    "```",
  ].join("\n");
}

function summarizeActiveHandoffEntry(entry) {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    return null;
  }

  const ownerId = typeof entry.owner_id === "string" ? entry.owner_id : "";
  const ownerType = typeof entry.owner_type === "string" ? entry.owner_type : "";
  const skill = typeof entry.skill === "string" ? entry.skill : "";
  const feature = typeof entry.feature === "string" ? entry.feature : "";
  const phase = typeof entry.phase === "string" ? entry.phase : "";
  const nextAction = typeof entry.next_action === "string" ? entry.next_action : "";
  const summary = typeof entry.summary === "string" ? entry.summary : "";
  const handoffPath = typeof entry.path === "string" ? entry.path : "";
  const status = typeof entry.status === "string" ? entry.status : "ready_to_resume";
  const pausedAt = typeof entry.paused_at === "string" ? entry.paused_at : "";
  const reason = typeof entry.reason === "string" ? entry.reason : "";
  const readFirst = Array.isArray(entry.read_first) ? entry.read_first.filter(Boolean) : [];

  const normalizedEntry = {
    owner_id: ownerId,
    owner_type: ownerType,
    skill,
    feature,
    phase,
    next_action: nextAction,
    summary,
    path: handoffPath,
    status,
    paused_at: pausedAt,
    reason,
    read_first: readFirst,
  };

  return {
    ...normalizedEntry,
    handoff_summary: renderHandoffSummary(normalizedEntry),
    resume_briefing: renderResumeBriefing(normalizedEntry),
    transfer_block: renderTransferBlock(normalizedEntry),
    operator_summary: [
      ownerId || "(unknown owner)",
      skill ? `via ${skill}` : "",
      feature ? `feature=${feature}` : "",
      phase ? `phase=${phase}` : "",
      nextAction ? `next=${nextAction}` : "",
      summary ? `summary=${summary}` : "",
      handoffPath ? `path=${handoffPath}` : "",
    ].filter(Boolean).join(" | "),
  };
}

function summarizeHandoffManifest(handoffManifest) {
  const activeEntries = Array.isArray(handoffManifest?.active)
    ? handoffManifest.active.map(summarizeActiveHandoffEntry).filter(Boolean)
    : [];

  return {
    exists: Boolean(handoffManifest),
    active_count: activeEntries.length,
    updated_at: typeof handoffManifest?.updated_at === "string" ? handoffManifest.updated_at : "",
    active: activeEntries,
  };
}
export async function readPulseStatus(repoRoot) {
  const paths = getPulseStatePaths(repoRoot);
  const onboarding = readJsonIfExists(paths.onboarding);
  const toolingStatus = readJsonIfExists(paths.toolingStatus);
  const stateJson = readJsonIfExists(paths.stateJson);
  const stateMarkdownText = fileTextIfExists(paths.stateMarkdown);
  const stateMarkdown = parseLooseKeyValueMarkdown(stateMarkdownText);
  const derivedRuntime = syncPulseRuntimeArtifacts(repoRoot);
  const handoffManifest = readJsonIfExists(paths.handoffManifest);

  const gitNexusReadiness = await readGitNexusReadiness(repoRoot);

  const stateJsonSummary = {
    exists: Boolean(stateJson),
    ...normalizePulseState(stateJson),
  };
  const stateMarkdownSummary = {
    exists: stateMarkdownText.trim() !== "",
    ...stateMarkdown,
  };
  const currentFeatureSummary = summarizeCurrentFeature(derivedRuntime.current_feature);
  const runtimeSnapshotSummary = summarizeRuntimeSnapshot(derivedRuntime.runtime_snapshot);
  const handoffManifestSummary = summarizeHandoffManifest(handoffManifest);
  const derivedFeature = deriveFeature({
    current_feature: currentFeatureSummary,
    state_json: stateJsonSummary,
    state_markdown: stateMarkdownSummary,
  });
  const historyLifecycle = summarizeHistoryLifecycle(repoRoot, derivedFeature);
  const projectDocsSummary = summarizeProjectDocs(repoRoot, paths);

  const status = {
    repo_root: repoRoot,
    onboarding: {
      exists: Boolean(onboarding),
      status: onboarding?.status || "",
      plugin_version: onboarding?.plugin_version || "",
    },
    tooling_status: {
      exists: Boolean(toolingStatus),
      status: typeof toolingStatus?.status === "string" ? toolingStatus.status : "",
      requested_mode:
        typeof toolingStatus?.requested_mode === "string" ? toolingStatus.requested_mode : "",
      recommended_mode:
        typeof toolingStatus?.recommended_mode === "string" ? toolingStatus.recommended_mode : "",
      next_skill: typeof toolingStatus?.next_skill === "string" ? toolingStatus.next_skill : "",
      blockers: Array.isArray(toolingStatus?.blockers) ? toolingStatus.blockers : [],
    },
    state_json: stateJsonSummary,
    state_markdown: stateMarkdownSummary,
    current_feature: currentFeatureSummary,
    runtime_snapshot: runtimeSnapshotSummary,
    reservations: summarizeReservations(readReservationStore(repoRoot)),
    handoff_manifest: handoffManifestSummary,
    history_lifecycle: historyLifecycle,
    project_docs: projectDocsSummary,
    critical_patterns_exists: fs.existsSync(paths.criticalPatterns),
    gitnexus_readiness: gitNexusReadiness,
    memory_recall: null,
    next_reads: [],
    recommended_actions: [],
  };

  status.memory_recall = summarizeMemoryRecall(paths, derivedFeature, status);
  status.next_reads = buildNextReads(status);
  status.recommended_actions = buildRecommendedActions(status);
  return status;
}

export function renderProjectDocsLines(status) {
  const projectDocs = status.project_docs && typeof status.project_docs === "object"
    ? status.project_docs
    : {
        exists: false,
        status: "missing",
        mode: "",
        mapping_path: "",
        context: { root: "", map: "", entries: [] },
        adrs: { enabled: false, dir: "", exists: false },
        notes: [],
        warnings: [],
      };

  const lines = ["Project docs:"];
  lines.push(`- Status: ${projectDocs.status || "missing"}`);
  lines.push(`- Mode: ${projectDocs.mode || "(unknown)"}`);
  lines.push(`- Mapping path: ${projectDocs.mapping_path || "(none)"}`);
  lines.push(`- Root context: ${projectDocs.context?.root || "(none)"}`);
  lines.push(`- Context map: ${projectDocs.context?.map || "(none)"}`);
  lines.push(`- Context entries: ${Array.isArray(projectDocs.context?.entries) ? projectDocs.context.entries.length : 0}`);
  lines.push(`- ADR dir: ${projectDocs.adrs?.dir || "(none)"}`);
  lines.push(`- ADRs present: ${projectDocs.adrs?.exists ? "yes" : "no"}`);
  if (Array.isArray(projectDocs.warnings) && projectDocs.warnings[0]) {
    lines.push(`- Warning: ${projectDocs.warnings[0]}`);
  }
  return lines;
}

export function renderGitNexusReadinessLines(status) {
  const readiness = status.gitnexus_readiness && typeof status.gitnexus_readiness === "object"
    ? status.gitnexus_readiness
    : null;
  if (!readiness) {
    return [];
  }

  const matchedSources =
    Array.isArray(readiness.matched_sources) && readiness.matched_sources.length > 0
      ? readiness.matched_sources.join(", ")
      : "none";

  return [
    "gitnexus readiness:",
    `- Configured: ${readiness.configured ? "yes" : "no"}`,
    `- Matched sources: ${matchedSources}`,
    `- Recommended action: ${readiness.recommended_action || "n/a"}`,
  ];
}

export function renderOperatorSurfaceLines(status) {
  const lines = ["Operator surface:"];
  const currentFeature = status.current_feature && typeof status.current_feature === "object"
    ? status.current_feature
    : { exists: false };
  const runtimeSnapshot = status.runtime_snapshot && typeof status.runtime_snapshot === "object"
    ? status.runtime_snapshot
    : { exists: false };
  const handoffManifest = status.handoff_manifest && typeof status.handoff_manifest === "object"
    ? status.handoff_manifest
    : { exists: false, active_count: 0, active: [] };
  const reservations = status.reservations && typeof status.reservations === "object"
    ? status.reservations
    : { exists: false, total: 0, active_count: 0, expired_count: 0, released_count: 0, active_agents: [] };
  const historyLifecycle = status.history_lifecycle && typeof status.history_lifecycle === "object"
    ? status.history_lifecycle
    : {
        exists: false,
        lifecycle_summary: "",
        approved_artifacts: [],
        verification: [],
        memory_promotions: [],
        lifecycle_signals: [],
        next_reads: [],
        self_sufficient: false,
      };
  const projectDocs = status.project_docs && typeof status.project_docs === "object"
    ? status.project_docs
    : {
        status: "missing",
        mode: "",
        mapping_path: "",
        context: { root: "", map: "", entries: [] },
        adrs: { enabled: false, dir: "", exists: false },
        warnings: [],
      };
  const memoryRecall = status.memory_recall && typeof status.memory_recall === "object"
    ? status.memory_recall
    : {
        root_exists: false,
        critical_patterns: "",
        learnings: [],
        corrections: [],
        ratchet: [],
        recall_pack: [],
        schema_summary: null,
        hygiene: { warnings: [] },
      };

  lines.push(
    `- Current feature snapshot: ${currentFeature.exists ? "present" : "missing"}`,
  );
  lines.push(`- Project docs: ${projectDocs.status || "missing"}${projectDocs.mode ? ` (${projectDocs.mode})` : ""}`);
  if (projectDocs.mapping_path) {
    lines.push(`  - mapping_path: ${projectDocs.mapping_path}`);
  }
  if (projectDocs.context?.root) {
    lines.push(`  - root_context: ${projectDocs.context.root}`);
  }
  if (projectDocs.context?.map) {
    lines.push(`  - context_map: ${projectDocs.context.map}`);
  }
  if (projectDocs.adrs?.dir) {
    lines.push(`  - adr_dir: ${projectDocs.adrs.dir}`);
  }
  if (currentFeature.exists) {
    lines.push(`  - feature_key: ${currentFeature.feature_key || "(none)"}`);
    lines.push(`  - phase: ${currentFeature.phase || "(none)"}`);
    lines.push(`  - gate: ${currentFeature.gate || "(none)"}`);
    if (currentFeature.gate_status) {
      lines.push(`  - gate_status: ${currentFeature.gate_status}`);
    }
    lines.push(`  - status: ${currentFeature.status || "(none)"}`);
    if (currentFeature.next_action) {
      lines.push(`  - next_action: ${currentFeature.next_action}`);
    }
    if (currentFeature.next_skill_recommended) {
      lines.push(`  - next_skill_recommended: ${currentFeature.next_skill_recommended}`);
    }
    lines.push(`  - updated_at: ${currentFeature.updated_at || "(none)"}`);
  }

  lines.push(
    `- Runtime snapshot: ${runtimeSnapshot.exists ? "present" : "missing"}`,
  );
  if (runtimeSnapshot.exists) {
    lines.push(`  - schema_version: ${runtimeSnapshot.schema_version || "(none)"}`);
    lines.push(`  - active_feature: ${runtimeSnapshot.active_feature || "(none)"}`);
    lines.push(`  - active_skill: ${runtimeSnapshot.active_skill || "(none)"}`);
    lines.push(`  - phase: ${runtimeSnapshot.phase || "(none)"}`);
    if (runtimeSnapshot.gate) {
      lines.push(`  - gate: ${runtimeSnapshot.gate}`);
    }
    if (runtimeSnapshot.gate_status) {
      lines.push(`  - gate_status: ${runtimeSnapshot.gate_status}`);
    }
    lines.push(`  - requested_mode: ${runtimeSnapshot.requested_mode || "(unspecified)"}`);
    lines.push(`  - recommended_mode: ${runtimeSnapshot.recommended_mode || "(unspecified)"}`);
    if (runtimeSnapshot.next_action) {
      lines.push(`  - next_action: ${runtimeSnapshot.next_action}`);
    }
    if (runtimeSnapshot.next_skill_recommended) {
      lines.push(`  - next_skill_recommended: ${runtimeSnapshot.next_skill_recommended}`);
    }
    lines.push(`  - updated_at: ${runtimeSnapshot.updated_at || "(none)"}`);
  }

  lines.push(`- Active reservations: ${reservations.active_count || 0}`);
  lines.push(`  - reservation_store: ${reservations.exists ? "present" : "missing"}`);
  lines.push(`  - reservation_count: ${reservations.total || 0}`);
  lines.push(`  - expired_reservations: ${reservations.expired_count || 0}`);
  lines.push(`  - released_reservations: ${reservations.released_count || 0}`);
  lines.push(`  - active_agents: ${(reservations.active_agents || []).length > 0 ? reservations.active_agents.join(", ") : "(none)"}`);

  lines.push(`- Active handoffs: ${handoffManifest.active_count || 0}`);
  if (Array.isArray(handoffManifest.active) && handoffManifest.active.length > 0) {
    for (const handoff of handoffManifest.active) {
      lines.push(`  - ${handoff.operator_summary}`);
    }
    lines.push(`  - manifest_updated_at: ${handoffManifest.updated_at || "(none)"}`);
  }


  lines.push(`- History lifecycle: ${historyLifecycle.exists ? "present" : "missing"}`);
  if (historyLifecycle.exists) {
    lines.push(`  - lifecycle_summary: ${historyLifecycle.lifecycle_summary || "(none)"}`);
    lines.push(`  - self_sufficient: ${historyLifecycle.self_sufficient ? "yes" : "no"}`);
    if ((historyLifecycle.approved_artifacts || []).length > 0) {
      lines.push(`  - approved_artifacts: ${historyLifecycle.approved_artifacts.join(", ")}`);
    }
    if ((historyLifecycle.lifecycle_signals || []).length > 0) {
      lines.push(`  - lifecycle_signals: ${historyLifecycle.lifecycle_signals.join(", ")}`);
    }
    if ((historyLifecycle.verification || []).length > 0) {
      lines.push(`  - canonical_verification: ${historyLifecycle.verification.join(", ")}`);
    }
  }

  lines.push(`- Memory recall root: ${memoryRecall.root_exists ? "present" : "missing"}`);
  if (memoryRecall.critical_patterns) {
    lines.push(`  - critical_patterns: ${memoryRecall.critical_patterns}`);
  }
  if ((memoryRecall.learnings || []).length > 0) {
    lines.push(`  - learnings: ${(memoryRecall.learnings || []).join(", ")}`);
  }
  if ((memoryRecall.corrections || []).length > 0) {
    lines.push(`  - corrections: ${(memoryRecall.corrections || []).join(", ")}`);
  }
  if ((memoryRecall.ratchet || []).length > 0) {
    lines.push(`  - ratchet: ${(memoryRecall.ratchet || []).join(", ")}`);
  }
  if ((memoryRecall.recall_pack || []).length > 0) {
    lines.push("  - recall_pack:");
    for (const entry of memoryRecall.recall_pack) {
      lines.push(`    - ${entry.kind}: ${entry.path} (${entry.reason})`);
    }
  }
  if (memoryRecall.schema_summary) {
    lines.push(
      `  - schema_summary: ${memoryRecall.schema_summary.strong_schema_entries || 0}/${memoryRecall.schema_summary.selected_entries || 0} strong-schema entries selected; metadata_first=${memoryRecall.schema_summary.metadata_first_ranking ? "yes" : "no"}; filename_fallback=${memoryRecall.schema_summary.fallback_to_filename_tokens ? "yes" : "no"}`,
    );
  }
  if ((memoryRecall.hygiene?.warnings || []).length > 0) {
    lines.push("  - hygiene_warnings:");
    for (const warning of memoryRecall.hygiene.warnings) {
      lines.push(`    - ${warning}`);
    }
  }

  return lines;
}

export function renderPulseStatus(status) {
  return renderPulseStatusImpl(status);
}
