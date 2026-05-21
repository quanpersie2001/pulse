import fs from "node:fs";
import path from "node:path";

import { ensureParent, readJsonIfExists, readTextIfExists } from "../core/fs.mjs";
import { buildDefaultState, normalizePulseState } from "../pulse_state.mjs";
import {
  managedAgentsPresent,
  mergeAgentsContent,
  readTemplate,
} from "./agents.mjs";
import {
  initializeWorkgraphFilesystem,
  writeSupportAssets,
} from "./assets.mjs";
import {
  buildDomainNormalization,
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
  ONBOARDING_SCHEMA_VERSION,
  readOnboardingState,
  utcNow,
  WORKFLOW_COMMAND,
  writeStateMarkdownFromTooling,
} from "./state.mjs";
import {
  loadPluginVersion,
  PULSE_COMMAND,
} from "./package.mjs";

export function applyRepo(repoRoot, options = {}) {
  const runtime = getNodeRuntimeStatus();
  if (!runtime.supported) {
    return buildRuntimeBlockedPayload(repoRoot, "apply");
  }

  const pluginVersion = loadPluginVersion();
  const template = readTemplate();
  const domainNormalization = buildDomainNormalization(repoRoot);

  const agentsPath = path.join(repoRoot, "AGENTS.md");
  const onboardingPath = path.join(repoRoot, ONBOARDING_MARKER_PATH);
  const statePath = path.join(repoRoot, ".pulse", "runtime", "state.json");
  const memoryRootPath = path.join(repoRoot, ".pulse", "memory");
  const memoryLearningsPath = path.join(memoryRootPath, "learnings");
  const memoryCorrectionsPath = path.join(memoryRootPath, "corrections");
  const memoryRatchetPath = path.join(memoryRootPath, "ratchet");
  const existingOnboarding = readOnboardingState(onboardingPath);

  ensureParent(agentsPath);
  ensureParent(onboardingPath);
  ensureParent(statePath);
  fs.mkdirSync(memoryLearningsPath, { recursive: true });
  fs.mkdirSync(memoryCorrectionsPath, { recursive: true });
  fs.mkdirSync(memoryRatchetPath, { recursive: true });

  const mergedAgents = mergeAgentsContent(readTextIfExists(agentsPath), template);
  fs.writeFileSync(agentsPath, mergedAgents.text, "utf8");

  const supportAssets = writeSupportAssets(repoRoot);
  const workgraphPaths = initializeWorkgraphFilesystem(repoRoot);

  const defaultState = buildDefaultState();
  const domainDetails = classifyDomains(repoRoot);
  const domainStatus = domainStatusSummary(domainDetails);
  const blockers = [];
  const degradations = [];
  const warnings = [];

  const requestedMode = "full-pipeline";
  const recommendedMode = "single-worker";
  const readinessStatus = buildReadinessStatus({ blockers, degradations });

  const toolingStatusPayload = buildToolingStatusPayload(repoRoot, buildToolingStatusOptions({
    runtime,
    pulseCommand: PULSE_COMMAND,
    requestedMode,
    recommendedMode,
    readinessStatus,
    onboardingStatus: "PASS",
    domainStatus,
    blockers,
    degradations,
    warnings,
    resumeOwner: options.resumeOwner,
  }));

  const nextState = normalizePulseState({
    ...defaultState,
    ...readJsonIfExists(statePath),
    phase: WORKFLOW_COMMAND,
    active_command: toolingStatusPayload.session_load?.active_context?.active_command || WORKFLOW_COMMAND,
    active_epic_id: toolingStatusPayload.session_load?.active_context?.active_epic_id || null,
    active_story_id: toolingStatusPayload.session_load?.active_context?.active_story_id || null,
    active_item_id: toolingStatusPayload.session_load?.active_context?.active_item_id || null,
    status: readinessStatus,
    requested_mode: requestedMode,
    recommended_mode: recommendedMode,
    session: {
      posture: toolingStatusPayload.session_load?.posture || "fresh",
      scout_findings: toolingStatusPayload.session?.scout_findings || [],
      resume_options: toolingStatusPayload.session_load?.resume_options || [],
    },
    session_load: toolingStatusPayload.session_load,
    tooling_status: ".pulse/runtime/tooling-status.json",
    next_command: toolingStatusPayload.next_command || "pulse:workflow explore",
  });

  const toolingStatusPath = path.join(repoRoot, ".pulse", "runtime", "tooling-status.json");
  ensureParent(toolingStatusPath);
  fs.writeFileSync(toolingStatusPath, `${JSON.stringify(toolingStatusPayload, null, 2)}\n`, "utf8");
  fs.writeFileSync(statePath, `${JSON.stringify(nextState, null, 2)}\n`, "utf8");
  writeStateMarkdownFromTooling(repoRoot, toolingStatusPayload);

  const onboardingNotes = [
    ...domainNormalization.domains.pulse.notes,
    ...domainNormalization.domains.docs.notes,
    ...domainNormalization.domains.works.notes,
  ];
  const status = "complete";

  const onboardingPayload = {
    schema_version: ONBOARDING_SCHEMA_VERSION,
    plugin: "pulse",
    plugin_version: pluginVersion,
    installed_at: utcNow(),
    status,
    previous_onboarding_status:
      typeof existingOnboarding?.status === "string" && existingOnboarding.status
        ? existingOnboarding.status
        : null,
    managed_assets: {
      agents_mode: mergedAgents.status,
      support_assets: supportAssets,
      onboarding_marker_path: ONBOARDING_MARKER_PATH,
      domain_normalization: domainNormalization,
      works_reconstructions: domainNormalization.domains.works.reconstructions,
      docs_reconstructions: domainNormalization.domains.docs.reconstructions,
      workgraph: {
        schema: path.relative(repoRoot, workgraphPaths.schemaPath),
        items: path.relative(repoRoot, workgraphPaths.itemsPath),
        views: Object.fromEntries(
          Object.entries(workgraphPaths.viewPaths).map(([name, filePath]) => [name, path.relative(repoRoot, filePath)]),
        ),
      },
      state_file: path.relative(repoRoot, statePath),
      memory_root: path.relative(repoRoot, memoryRootPath),
      memory_directories: [
        path.relative(repoRoot, memoryLearningsPath),
        path.relative(repoRoot, memoryCorrectionsPath),
        path.relative(repoRoot, memoryRatchetPath),
      ],
    },
    notes: onboardingNotes,
  };
  fs.writeFileSync(`${onboardingPath}`, `${JSON.stringify(onboardingPayload, null, 2)}\n`, "utf8");

  return {
    repo_root: repoRoot,
    status: readinessStatus,
    requested_mode: requestedMode,
    recommended_mode: recommendedMode,
    actions: [],
    blockers,
    degradations,
    warnings,
    requires_confirmation: false,
    next_command: toolingStatusPayload.next_command,
    details: {
      plugin_version: pluginVersion,
      agents_exists: mergedAgents.text.trim() !== "",
      agents_managed_block: managedAgentsPresent(mergedAgents.text),
      onboarding_marker_path: ONBOARDING_MARKER_PATH,
      onboarding_state: onboardingPayload,
      domain_status: domainStatus,
      domain_details: domainDetails,
      state_exists: true,
      workgraph: {
        schema_exists: fs.existsSync(workgraphPaths.schemaPath),
        items_exists: fs.existsSync(workgraphPaths.itemsPath),
        views: Object.fromEntries(
          Object.entries(workgraphPaths.viewPaths).map(([name, filePath]) => [name, fs.existsSync(filePath)]),
        ),
      },
      runtime,
      tooling_status_preview: toolingStatusPayload,
    },
    applied: true,
    result: onboardingPayload,
  };
}
