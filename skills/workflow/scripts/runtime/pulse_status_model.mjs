import fs from "node:fs";

import {
  buildNextReads,
  buildRecommendedActions,
  fileTextIfExists,
  getPulseStatePaths,
  normalizePulseState,
  parseLooseKeyValueMarkdown,
  readGitNexusReadiness,
  readJsonIfExists,
  summarizeCurrentFeature,
  summarizeHandoffManifest,
  summarizeHistoryLifecycle,
  summarizeMemoryRecall,
  summarizeProjectDocs,
  summarizeReservationStatusForState,
  summarizeRuntimeSnapshot,
  syncPulseRuntimeArtifacts,
} from "./pulse_state.mjs";

function deriveFeature(status) {
  if (status.current_feature?.feature_key) {
    return status.current_feature.feature_key;
  }
  if (status.state_json.active_feature) {
    return status.state_json.active_feature;
  }
  const focus = status.state_markdown.focus || "";
  return focus === "(none)" ? "" : focus;
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
    reservations: summarizeReservationStatusForState(repoRoot),
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
