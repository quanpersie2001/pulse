import fs from "node:fs";
import path from "node:path";

import { readJsonIfExists, readTextIfExists } from "../core/fs.mjs";
import { normalizePulseState } from "../runtime/state.mjs";
import { getWorkgraphPaths } from "../workgraph/store.mjs";
import { managedAgentsPresent } from "./agents.mjs";
import { supportAssetsNeedUpdate } from "./assets.mjs";
import {
  classifyDomains,
  domainStatusSummary,
} from "./domains.mjs";
import {
  buildReadinessStatus,
  buildRuntimeBlockedPayload,
  buildToolingStatusOptions,
  buildToolingStatusPayload,
  getNodeRuntimeStatus,
  ONBOARDING_MARKER_PATH,
} from "./state.mjs";
import {
  loadPluginVersion,
  PULSE_COMMAND,
} from "./package.mjs";

export {
  buildReadinessStatus,
  buildRuntimeBlockedPayload,
  getNodeRuntimeStatus,
};

export function checkRepo(repoRoot, options = {}) {
  const runtime = getNodeRuntimeStatus();
  if (!runtime.supported) {
    return buildRuntimeBlockedPayload(repoRoot, "check");
  }

  const pluginVersion = loadPluginVersion();
  const agentsPath = path.join(repoRoot, "AGENTS.md");
  const onboardingPath = path.join(repoRoot, ONBOARDING_MARKER_PATH);
  const statePath = path.join(repoRoot, ".pulse", "runtime", "state.json");
  const workgraphPaths = getWorkgraphPaths(repoRoot);

  const agentsText = readTextIfExists(agentsPath);
  const agentsExists = agentsText.trim() !== "";
  const managedAgents = agentsExists && managedAgentsPresent(agentsText);

  const onboarding = readJsonIfExists(onboardingPath) || {};
  const onboardingMarkerExists = fs.existsSync(onboardingPath);

  const domainDetails = classifyDomains(repoRoot);
  const domainStatus = domainStatusSummary(domainDetails);

  const actions = [];
  if (!agentsExists) {
    actions.push("create_AGENTS.md");
  } else if (!managedAgents) {
    actions.push("append_pulse_managed_block_to_AGENTS.md");
  }

  if (supportAssetsNeedUpdate(repoRoot)) {
    actions.push("sync_pulse_data_assets");
  }

  if (domainStatus.pulse !== "compliant") {
    actions.push("normalize_.pulse_structure");
  }
  if (domainStatus.docs !== "compliant") {
    actions.push("normalize_docs_structure");
  }
  if (domainStatus.works !== "compliant") {
    actions.push("normalize_works_structure");
  }

  if (!fs.existsSync(workgraphPaths.schemaPath)) {
    actions.push("write_.pulse/workgraph/schema.json");
  }
  if (!fs.existsSync(workgraphPaths.itemsPath)) {
    actions.push("write_.pulse/workgraph/items.jsonl");
  }
  if (Object.values(workgraphPaths.viewPaths).some((filePath) => !fs.existsSync(filePath))) {
    actions.push("sync_.pulse/workgraph/views");
  }

  const state = readJsonIfExists(statePath);
  const normalizedState = normalizePulseState(state);
  const stateNeedsWrite =
    !state || JSON.stringify(state, null, 2) !== JSON.stringify(normalizedState, null, 2);
  if (stateNeedsWrite) {
    actions.push("write_.pulse/runtime/state.json");
  }

  if (onboarding.plugin_version !== pluginVersion) {
    actions.push("write_.pulse/runtime/onboarding.json");
  }

  const blockers = [...actions];
  const degradations = [];
  const warnings = [];

  const requestedMode = "full-pipeline";
  const recommendedMode = blockers.length > 0 ? "blocked" : "single-worker";
  const readinessStatus = buildReadinessStatus({ blockers, degradations });
  const toolingStatusPreview = buildToolingStatusPayload(repoRoot, buildToolingStatusOptions({
    runtime,
    pulseCommand: PULSE_COMMAND,
    requestedMode,
    recommendedMode,
    readinessStatus,
    onboardingStatus: actions.length === 0
      ? "PASS"
      : onboardingMarkerExists
        ? "NEEDS_REMEDIATION"
        : "NEEDS_SETUP",
    domainStatus,
    blockers,
    degradations,
    warnings,
    resumeOwner: options.resumeOwner,
  }));

  return {
    repo_root: repoRoot,
    status: readinessStatus,
    requested_mode: requestedMode,
    recommended_mode: recommendedMode,
    actions,
    blockers,
    degradations,
    warnings,
    requires_confirmation: false,
    next_command: toolingStatusPreview.next_command,
    details: {
      plugin_version: pluginVersion,
      agents_exists: agentsExists,
      agents_managed_block: managedAgents,
      onboarding_marker_path: ONBOARDING_MARKER_PATH,
      onboarding_state: Object.keys(onboarding).length > 0 ? onboarding : null,
      domain_status: domainStatus,
      domain_details: domainDetails,
      state_exists: fs.existsSync(statePath),
      workgraph: {
        schema_exists: fs.existsSync(workgraphPaths.schemaPath),
        items_exists: fs.existsSync(workgraphPaths.itemsPath),
        views: Object.fromEntries(
          Object.entries(workgraphPaths.viewPaths).map(([name, filePath]) => [name, fs.existsSync(filePath)]),
        ),
      },
      runtime,
      tooling_status_preview: toolingStatusPreview,
    },
  };
}
