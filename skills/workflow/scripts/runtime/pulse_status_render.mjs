import {
  deriveFeature,
  renderGitNexusReadinessLines,
  renderOperatorSurfaceLines,
  renderProjectDocsLines,
} from "./pulse_state.mjs";

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
    ...renderProjectDocsLines(status),
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
