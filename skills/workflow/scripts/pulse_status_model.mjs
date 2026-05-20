/**
 * Purpose: Build structured Pulse status payload consumed by scout output.
 * Caller/flow: Used by pulse_status.mjs and onboarding/readiness flows.
 * Reads/Writes: Reads runtime state, handoffs, reservations, memory, and gitnexus readiness; no writes.
 * CLI args: None (module API).
 * Ownership: Status aggregation only; does not own rendering or direct CLI I/O.
 * Repo root rule: Caller provides repo root and module reads within that boundary.
 */

import fs from "node:fs";

import {
  fileTextIfExists,
  getPulseStatePaths,
  normalizePulseState,
  parseLooseKeyValueMarkdown,
  readJsonIfExists,
} from "./pulse_state.mjs";
import { syncPulseRuntimeArtifacts } from "./pulse_runtime_sync.mjs";
import { summarizeReservationStatusForState } from "./pulse_reservation_store.mjs";
import { buildNextReads, buildRecommendedActions } from "./pulse_recommendations.mjs";
import { summarizeMemoryRecall } from "./pulse_memory_recall.mjs";
import {
  summarizeCurrentFeature,
  summarizeRuntimeSnapshot,
} from "./pulse_runtime_derivations.mjs";
import { readGitNexusReadiness } from "./pulse_gitnexus_readiness.mjs";
import { summarizeHandoffManifest } from "./pulse_handoffs.mjs";
import { buildSessionLoad } from "./pulse_session_load.mjs";

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
  const sessionLoad = buildSessionLoad(repoRoot);

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
    session_load: sessionLoad,
    handoff_manifest: handoffManifestSummary,
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
