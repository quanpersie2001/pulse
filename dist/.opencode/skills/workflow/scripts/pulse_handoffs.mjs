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

export function summarizeHandoffManifest(handoffManifest) {
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
