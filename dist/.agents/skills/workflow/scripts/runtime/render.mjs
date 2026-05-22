/**
 * Purpose: Render human-readable Pulse status from normalized status payload.
 * Caller/flow: Used by cli/status.mjs text mode.
 * Reads/Writes: Pure formatting logic; no filesystem or runtime writes.
 * CLI args: None (module API).
 * Ownership: Presentation only; data sourcing/validation is owned by status model.
 * Repo root rule: Not applicable (no path resolution).
 */

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

function renderGitNexusReadinessLines(status) {
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

function renderOperatorSurfaceLines(status) {
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
    if (currentFeature.next_command) {
      lines.push(`  - next_command: ${currentFeature.next_command}`);
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
    if (runtimeSnapshot.next_command) {
      lines.push(`  - next_command: ${runtimeSnapshot.next_command}`);
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
  const onboarding = status.onboarding.exists
    ? `${status.onboarding.status || "installed"}${status.onboarding.plugin_version ? ` (${status.onboarding.plugin_version})` : ""}`
    : "missing";
  const feature = deriveFeature(status) || "(none)";
  const skill = status.state_json.active_skill || "(none)";
  const phase = status.state_json.phase || status.state_markdown.phase || "(none)";
  const requestedMode =
    status.tooling_status.requested_mode || status.state_json.requested_mode || "(unspecified)";
  const recommendedMode =
    status.tooling_status.recommended_mode || status.state_json.recommended_mode || "(unspecified)";

  return [
    "Pulse Status",
    `Repo: ${status.repo_root}`,
    `Onboarding: ${onboarding}`,
    `Feature: ${feature}`,
    `Skill: ${skill}`,
    `Phase: ${phase}`,
    `Requested mode: ${requestedMode}`,
    `Recommended mode: ${recommendedMode}`,
    `Active handoffs: ${status.handoff_manifest.active_count}`,
    "",
    ...renderOperatorSurfaceLines(status),
    "",
    ...renderGitNexusReadinessLines(status),
    "",
    "Next reads:",
    ...status.next_reads.map((item) => `- ${item}`),
    "",
    "Recommended actions:",
    ...status.recommended_actions.map((item) => `- ${item}`),
  ].join("\n");
}
