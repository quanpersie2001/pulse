import fs from "node:fs";
import path from "node:path";

import { ensureParent, readJsonIfExists } from "../core/fs.mjs";
import { buildSessionLoad } from "../runtime/session-load.mjs";

export const ONBOARDING_SCHEMA_VERSION = "1.0";
export const WORKFLOW_COMMAND = "use";
export const WORKFLOW_SETUP_STEP = "onboarding";
export const ONBOARDING_MARKER_PATH = path.join(".pulse", "runtime", "onboarding.json");
export const MIN_NODE_MAJOR = 18;

export function getNodeRuntimeStatus(version = process.versions.node) {
  const major = Number.parseInt(String(version).split(".")[0] || "0", 10);
  const supported = Number.isFinite(major) && major >= MIN_NODE_MAJOR;
  return {
    command: "node",
    minimum_major: MIN_NODE_MAJOR,
    supported,
    version,
  };
}

export function utcNow() {
  return new Date().toISOString();
}

export function buildReadinessStatus({ blockers = [], degradations = [] }) {
  if ((blockers || []).length > 0) {
    return "FAIL";
  }
  if ((degradations || []).length > 0) {
    return "DEGRADED";
  }
  return "PASS";
}

export function buildRuntimeBlockedPayload(repoRoot, action) {
  const runtime = getNodeRuntimeStatus();
  return {
    repo_root: repoRoot,
    status: "FAIL",
    action,
    requires_confirmation: false,
    actions: ["install_supported_node_runtime"],
    message: `Pulse requires Node.js ${MIN_NODE_MAJOR}+ before onboarding can continue. Install Node.js and rerun onboarding.`,
    details: {
      runtime,
    },
  };
}

export function buildToolingStatusOptions({
  runtime,
  pulseCommand,
  requestedMode,
  recommendedMode,
  readinessStatus,
  onboardingStatus,
  domainStatus,
  blockers,
  degradations,
  warnings,
  resumeOwner,
}) {
  return {
    requestedMode,
    recommendedMode,
    readinessStatus,
    onboardingStatus,
    domainStatus,
    blockers,
    degradations,
    warnings,
    tools: {
      git: { available: true },
      node: runtime,
      pulse_runtime_helper: { available: true, command: `${pulseCommand} status --repo-root <repo> --json` },
    },
    resumeOwner,
  };
}

export function buildToolingStatusPayload(repoRoot, options) {
  const {
    requestedMode,
    recommendedMode,
    readinessStatus,
    onboardingStatus,
    domainStatus,
    blockers,
    degradations,
    warnings,
    tools,
    resumeOwner,
  } = options;

  const sessionLoad = buildSessionLoad(repoRoot, { resumeOwner });

  return {
    timestamp: utcNow(),
    project_root: repoRoot,
    requested_mode: requestedMode,
    recommended_mode: recommendedMode,
    status: readinessStatus.toLowerCase(),
    onboarding: onboardingStatus,
    onboarding_marker_path: ONBOARDING_MARKER_PATH,
    domain_status: domainStatus,
    tools,
    blockers,
    degradations,
    warnings,
    session: {
      posture: {
        route_command: WORKFLOW_COMMAND,
        setup_step: WORKFLOW_SETUP_STEP,
        active_command: sessionLoad.active_context.active_command || WORKFLOW_COMMAND,
        active_epic_id: sessionLoad.active_context.active_epic_id,
        active_story_id: sessionLoad.active_context.active_story_id,
        active_item_id: sessionLoad.active_context.active_item_id,
        in_progress_items: sessionLoad.in_progress_items,
        open_reservations: sessionLoad.open_reservations,
      },
      scout_findings: sessionLoad.scout_findings,
      resume_options: sessionLoad.resume_options,
    },
    session_load: sessionLoad,
    next_command: sessionLoad.next_command,
  };
}

export function readOnboardingState(onboardingPath) {
  return readJsonIfExists(onboardingPath) || {};
}

export function writeStateMarkdownFromTooling(repoRoot, toolingStatusPayload) {
  const stateMarkdownPath = path.join(repoRoot, ".pulse", "runtime", "STATE.md");
  const content = [
    "# Pulse Runtime State",
    "",
    `Workflow command: ${WORKFLOW_COMMAND}`,
    `Setup step: ${WORKFLOW_SETUP_STEP}`,
    `Status: ${toolingStatusPayload.status.toUpperCase()}`,
    `Requested mode: ${toolingStatusPayload.requested_mode}`,
    `Recommended mode: ${toolingStatusPayload.recommended_mode}`,
    `Next command: ${toolingStatusPayload.next_command || "(none)"}`,
    `Session posture: ${toolingStatusPayload.session_load?.posture || "fresh"}`,
    `Open reservations: ${toolingStatusPayload.session?.posture?.open_reservations || 0}`,
    `Resume options: ${Array.isArray(toolingStatusPayload.session_load?.resume_options) ? toolingStatusPayload.session_load.resume_options.length : 0}`,
    `Blockers: ${Array.isArray(toolingStatusPayload.blockers) ? toolingStatusPayload.blockers.length : 0}`,
    `Degradations: ${Array.isArray(toolingStatusPayload.degradations) ? toolingStatusPayload.degradations.length : 0}`,
    "",
    "## Session Load",
    "",
    `Requires selection: ${toolingStatusPayload.session_load?.requires_selection ? "yes" : "no"}`,
    `Active command: ${toolingStatusPayload.session_load?.active_context?.active_command || "(none)"}`,
    `Active epic: ${toolingStatusPayload.session_load?.active_context?.active_epic_id || "(none)"}`,
    `Active story: ${toolingStatusPayload.session_load?.active_context?.active_story_id || "(none)"}`,
    `Active item: ${toolingStatusPayload.session_load?.active_context?.active_item_id || "(none)"}`,
    `Read-first files: ${Array.isArray(toolingStatusPayload.session_load?.read_first) ? toolingStatusPayload.session_load.read_first.length : 0}`,
    `Missing files: ${Array.isArray(toolingStatusPayload.session_load?.missing_files) ? toolingStatusPayload.session_load.missing_files.length : 0}`,
    `Rejected paths: ${Array.isArray(toolingStatusPayload.session_load?.rejected_paths) ? toolingStatusPayload.session_load.rejected_paths.length : 0}`,
    "",
    toolingStatusPayload.session_load?.summary ? `Summary: ${toolingStatusPayload.session_load.summary}` : "Summary: (none)",
    toolingStatusPayload.session_load?.next_action ? `Next safe action: ${toolingStatusPayload.session_load.next_action}` : "Next safe action: (none)",
    "",
  ].join("\n");

  ensureParent(stateMarkdownPath);
  fs.writeFileSync(stateMarkdownPath, content, "utf8");
}
