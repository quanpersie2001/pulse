#!/usr/bin/env node

/**
 * Purpose: Derive current Pulse runtime snapshot records from persisted runtime artifacts.
 * Caller/flow: Called by read-model/status paths to normalize current_feature and runtime_snapshot views.
 * Reads/Writes: Reads state JSON/markdown, tooling status, handoff manifest, reservations, and critical memory marker; no writes.
 * CLI args: None (module API).
 * Ownership: Derivation layer only; source-of-truth files stay owned by their runtime/workgraph writers.
 * Repo root rule: Normalizes explicit repo roots through runtime/state.mjs.
 */

import fs from "node:fs";
import {
  getPulseStatePaths,
  parseLooseKeyValueMarkdown,
  readJsonIfExists,
  fileTextIfExists,
  resolveRepoRoot,
  normalizePulseState,
} from "./state.mjs";
import { summarizeReservationStatusForState } from "../reservation/store.mjs";
import { summarizeHandoffManifest } from "./handoffs.mjs";
import {
  buildCurrentFeatureRecord,
  buildRuntimeSnapshotRecord,
  summarizeCurrentFeature,
  summarizeRuntimeSnapshot,
} from "./derivations.mjs";

function buildPulseRuntimeArtifacts(repoRoot) {
  const paths = getPulseStatePaths(repoRoot);
  const stateJson = readJsonIfExists(paths.stateJson);
  const stateMarkdownText = fileTextIfExists(paths.stateMarkdown);
  const stateMarkdown = parseLooseKeyValueMarkdown(stateMarkdownText);
  const toolingStatus = readJsonIfExists(paths.toolingStatus);
  const handoffManifest = readJsonIfExists(paths.handoffManifest);

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
      next_command: typeof toolingStatus?.next_command === "string" ? toolingStatus.next_command : "",
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
    reservations: summarizeReservationStatusForState(repoRoot),
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

export function derivePulseRuntimeArtifacts(repoRoot) {
  const normalizedRoot = resolveRepoRoot(repoRoot);
  return buildPulseRuntimeArtifacts(normalizedRoot);
}
