function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

export function normalizeNextCommandSurface(value) {
  const normalized = firstNonEmptyString(value);
  if (!normalized) {
    return "";
  }

  const validCommands = new Set([
    "use",
    "explore",
    "brainstorm",
    "plan",
    "validate",
    "swarm",
    "execute",
    "review",
    "compound",
  ]);

  if (normalized.startsWith("pulse:workflow ")) {
    const command = normalized.slice("pulse:workflow ".length).trim();
    return validCommands.has(command) ? normalized : "";
  }

  if (normalized.startsWith("pulse:")) {
    return "";
  }

  if (validCommands.has(normalized)) {
    return `pulse:workflow ${normalized}`;
  }

  return normalized;
}

export function inferWorkShapeNextSkillRecommended(status) {
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

export function inferGateNextSkillRecommended(status, gate, gateStatus) {
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

export function inferGateNextAction(status, gateStatus, nextSkillRecommended) {
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

export function buildNextReads(status) {
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


  for (const entry of status.memory_recall?.recall_pack || []) {
    if (entry.path) {
      reads.push(entry.path);
    }
  }

  return [...new Set(reads)];
}

export function buildRecommendedActions(status) {
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
      actions.push("When repo terminology or ownership boundaries matter, read the mapped project docs before going deeper into active work context.");
    } else if (status.project_docs?.status === "detected") {
      actions.push("Repo-level project docs were detected but are not mapped yet; map .pulse/project-docs.json before deeper planning.");
    } else {
      actions.push("If repo-wide terminology keeps drifting, propose a lightweight project-doc scaffold before deeper planning.");
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

  const sessionLoadNextCommand = normalizeNextCommandSurface(status.session_load?.next_command);
  if (sessionLoadNextCommand) {
    const actions = [`Next command suggestion: ${sessionLoadNextCommand}.`];
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
