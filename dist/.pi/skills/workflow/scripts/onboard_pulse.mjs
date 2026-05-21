#!/usr/bin/env node

/**
 * Purpose: Check/apply Pulse onboarding and runtime-domain normalization.
 * Caller/flow: Invoked by /pulse workflow use/onboard to bootstrap and verify runtime readiness.
 * Reads/Writes: Reads plugin/runtime/workgraph/docs/works state and writes managed .pulse/runtime, workgraph, and AGENTS assets.
 * CLI args: --repo-root, --apply, --resume-owner, --help.
 * Ownership: Owns onboarding orchestration; delegates status/session derivation to shared modules.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs; treats target repo root as mutation boundary.
 */

import fs from "node:fs";
import path from "node:path";
import {
  getPluginRoot,
  getPulseEntrypointPath,
  getScriptDir,
  getWorkflowSkillDir,
} from "./pulse_package_paths.mjs";
import {
  buildDefaultState,
  normalizePulseState,
} from "./pulse_state.mjs";
import { syncPulseRuntimeArtifacts } from "./pulse_runtime_sync.mjs";
import {
  relativePosix,
  resolveRepoRoot as resolveRepoRootFromPaths,
} from "./pulse_paths.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  loadItems,
  writeViews,
} from "./workgraph_store.mjs";
import { buildSessionLoad } from "./pulse_session_load.mjs";
import { isDirectExecution } from "./cli_execution.mjs";

const SCRIPT_DIR = getScriptDir(import.meta.url);
const WORKFLOW_SKILL_DIR = getWorkflowSkillDir(SCRIPT_DIR);
const PLUGIN_ROOT = getPluginRoot(SCRIPT_DIR);
const HARNESS_BACKLOG_TEMPLATE_PATH = path.join(WORKFLOW_SKILL_DIR, "templates", "HARNESS_BACKLOG.md");
const PLUGIN_MANIFEST_PATH = path.join(PLUGIN_ROOT, ".codex-plugin", "plugin.json");
const AGENTS_TEMPLATE_PATH = path.join(PLUGIN_ROOT, "AGENTS.template.md");
const PULSE_ENTRYPOINT_PATH = getPulseEntrypointPath(SCRIPT_DIR);
const PULSE_COMMAND = `node ${JSON.stringify(PULSE_ENTRYPOINT_PATH)}`;
const ONBOARDING_SCHEMA_VERSION = "1.0";
const WORKFLOW_COMMAND = "use";
const WORKFLOW_SETUP_STEP = "onboarding";
const ONBOARDING_MARKER_PATH = path.join(".pulse", "runtime", "onboarding.json");
const MIN_NODE_MAJOR = 18;

/**
 * Runtime checks and CLI root resolution.
 */

/**
 * Check whether the current Node.js runtime can execute Pulse onboarding.
 */
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

function utcNow() {
  return new Date().toISOString();
}

function loadPluginVersion() {
  return JSON.parse(fs.readFileSync(PLUGIN_MANIFEST_PATH, "utf8")).version;
}

/**
 * Resolve the target repository root from an explicit path, Git, or cwd fallback.
 */
export function resolveRepoRoot(explicitRoot, env = process.env, cwd = process.cwd()) {
  return resolveRepoRootFromPaths({ explicitRoot, env, cwd });
}

/**
 * Tooling status helpers.
 */

function buildReadinessStatus({ blockers = [], degradations = [] }) {
  if ((blockers || []).length > 0) {
    return "FAIL";
  }
  if ((degradations || []).length > 0) {
    return "DEGRADED";
  }
  return "PASS";
}

/**
 * Build the failure payload returned when Node.js is too old to continue.
 */
function buildRuntimeBlockedPayload(repoRoot, action) {
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

/**
 * Build the machine-readable runtime status payload written by check and apply.
 */
function buildToolingStatusPayload(repoRoot, options) {
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

/**
 * Render the human-readable runtime state mirror from tooling status.
 */
function writeStateMarkdownFromTooling(repoRoot, toolingStatusPayload) {
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

/**
 * Domain classifiers.
 */

function listActiveDomainEntries(domainPath) {
  return listDirectoryEntries(domainPath).filter((entry) => !isBackupEntry(entry));
}

/**
 * Classify the .pulse domain against the expected v2 runtime layout.
 */
function classifyPulseDomain(repoRoot) {
  const pulsePath = path.join(repoRoot, ".pulse");
  if (!fs.existsSync(pulsePath)) {
    return { status: "missing", missing: [".pulse"], unexpected_entries: [], conflicts: [] };
  }

  const required = [
    ["runtime", "directory"],
    [path.join("runtime", "handoffs"), "directory"],
    ["workgraph", "directory"],
    [path.join("workgraph", "views"), "directory"],
    ["harness", "directory"],
    ["memory", "directory"],
    [path.join("harness", "HARNESS_BACKLOG.md"), "file"],
  ];
  const missing = [];
  for (const [relative, kind] of required) {
    const absolute = path.join(pulsePath, relative);
    const exists = fs.existsSync(absolute);
    if (!exists || (kind === "directory" && !fs.statSync(absolute).isDirectory()) || (kind === "file" && !fs.statSync(absolute).isFile())) {
      missing.push(path.posix.join(".pulse", relative.split(path.sep).join(path.posix.sep)));
    }
  }

  const allowedTopLevel = new Set(["runtime", "workgraph", "harness", "memory", "scripts"]);
  const unexpectedEntries = listActiveDomainEntries(pulsePath)
    .filter((entry) => !allowedTopLevel.has(entry))
    .map((entry) => path.posix.join(".pulse", entry));

  return {
    status: missing.length === 0 && unexpectedEntries.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_entries: unexpectedEntries,
    conflicts: [],
  };
}

/**
 * Classify the docs domain against the expected semantic docs scaffold.
 */
function classifyDocsDomain(repoRoot) {
  const docsPath = path.join(repoRoot, "docs");
  if (!fs.existsSync(docsPath)) {
    return { status: "missing", missing: ["docs"], unexpected_entries: [], conflicts: [] };
  }
  const required = ["ARCHITECTURE.md", "GLOSSARY.md", "decisions", "product"];
  const missing = required.filter((entry) => !fs.existsSync(path.join(docsPath, entry)));
  return {
    status: missing.length === 0 ? "compliant" : "non_compliant",
    missing: missing.map((entry) => path.posix.join("docs", entry)),
    unexpected_entries: [],
    conflicts: [],
  };
}

/**
 * Classify the works domain against the expected story-first work layout.
 */
function classifyWorksDomain(repoRoot) {
  const worksPath = path.join(repoRoot, "works");
  if (!fs.existsSync(worksPath)) {
    return { status: "missing", missing: ["works"], unexpected_entries: [], conflicts: [] };
  }

  const activeEntries = listActiveDomainEntries(worksPath);
  const allowedTopLevel = new Set(["epics", "backlog.md", "test-matrix.md"]);
  const unexpectedEntries = activeEntries.filter((entry) => !allowedTopLevel.has(entry));
  const missing = fs.existsSync(path.join(worksPath, "epics")) ? [] : ["works/epics"];

  return {
    status: missing.length === 0 && unexpectedEntries.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_entries: unexpectedEntries.map((entry) => path.posix.join("works", entry)),
    conflicts: [],
  };
}

function classifyDomains(repoRoot) {
  return {
    pulse: classifyPulseDomain(repoRoot),
    docs: classifyDocsDomain(repoRoot),
    works: classifyWorksDomain(repoRoot),
  };
}

function domainStatusSummary(domains) {
  return Object.fromEntries(Object.entries(domains).map(([name, value]) => [name, value.status]));
}

/**
 * Domain normalization helpers.
 */

function backupDomainInPlace(repoRoot, relativePath, stamp) {
  const domainPath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(domainPath)) {
    return { backup: "", moved: [] };
  }
  ensureDirectory(domainPath);
  const backupName = `backup-${stamp}`;
  const backupAbsolute = path.join(domainPath, backupName);
  const moved = [];
  ensureDirectory(backupAbsolute);
  for (const entry of fs.readdirSync(domainPath)) {
    if (entry === backupName || isBackupEntry(entry)) {
      continue;
    }
    fs.renameSync(path.join(domainPath, entry), path.join(backupAbsolute, entry));
    moved.push(entry);
  }
  return {
    backup: path.posix.join(relativePath.split(path.sep).join(path.posix.sep), backupName),
    moved,
  };
}

function readOnboardingState(onboardingPath) {
  return readJsonIfExists(onboardingPath) || {};
}

function ensurePulseDomainLayout(repoRoot) {
  for (const relative of [
    [".pulse", "runtime"],
    [".pulse", "runtime", "handoffs"],
    [".pulse", "runtime", "onboarding"],
    [".pulse", "workgraph"],
    [".pulse", "workgraph", "views"],
    [".pulse", "harness"],
    [".pulse", "memory"],
  ]) {
    ensureDirectory(path.join(repoRoot, ...relative));
  }

  const manifestPath = path.join(repoRoot, ".pulse", "runtime", "handoffs", "manifest.json");
  if (!fs.existsSync(manifestPath)) {
    fs.writeFileSync(manifestPath, `${JSON.stringify({ schema_version: "1.0", updated_at: utcNow(), active: [] }, null, 2)}\n`, "utf8");
  }
  const reservationsPath = path.join(repoRoot, ".pulse", "runtime", "reservations.json");
  if (!fs.existsSync(reservationsPath)) {
    fs.writeFileSync(reservationsPath, `${JSON.stringify({ schema_version: "1.0", reservations: [] }, null, 2)}\n`, "utf8");
  }
}

function ensureDocsScaffold(repoRoot) {
  ensureDirectory(path.join(repoRoot, "docs"));
  ensureDirectory(path.join(repoRoot, "docs", "decisions"));
  ensureDirectory(path.join(repoRoot, "docs", "product"));
  const architecturePath = path.join(repoRoot, "docs", "ARCHITECTURE.md");
  if (!fs.existsSync(architecturePath)) {
    fs.writeFileSync(architecturePath, "# Architecture\n", "utf8");
  }
  const glossaryPath = path.join(repoRoot, "docs", "GLOSSARY.md");
  if (!fs.existsSync(glossaryPath)) {
    fs.writeFileSync(glossaryPath, "# Glossary\n", "utf8");
  }
}

function ensureWorksScaffold(repoRoot) {
  ensureDirectory(path.join(repoRoot, "works"));
  ensureDirectory(path.join(repoRoot, "works", "epics"));
}

/**
 * Copy known-safe runtime artifacts from a backed-up .pulse domain.
 */
function restorePulseBackup(repoRoot, backupRelativePath) {
  const restored = [];
  const notes = [];
  if (!backupRelativePath) {
    return { restored, notes };
  }

  const backupAbsolute = path.join(repoRoot, backupRelativePath);
  const copyIfPresent = (from, to) => {
    if (copyPathIfExists(path.join(backupAbsolute, from), path.join(repoRoot, to))) {
      restored.push(`${path.posix.join(backupRelativePath, from.split(path.sep).join(path.posix.sep))} -> ${to.split(path.sep).join(path.posix.sep)}`);
    }
  };

  copyIfPresent("runtime", path.join(".pulse", "runtime"));
  copyIfPresent("memory", path.join(".pulse", "memory"));
  copyIfPresent("workgraph", path.join(".pulse", "workgraph"));

  const unmapped = listActiveDomainEntries(backupAbsolute).filter(
    (entry) => !["runtime", "memory", "workgraph"].includes(entry),
  );
  if (unmapped.length > 0) {
    notes.push(`Unmapped .pulse backup entries require review: ${unmapped.join(", ")}.`);
  }

  return { restored, notes };
}

/**
 * Write operator briefs for content that was backed up during normalization.
 */
function writeOnboardingReconstructionBriefs(repoRoot, normalization) {
  const reconstructionDir = path.join(repoRoot, ".pulse", "runtime", "onboarding");
  ensureDirectory(reconstructionDir);
  const briefs = [];

  const writeBrief = (fileName, lines) => {
    const target = path.join(reconstructionDir, fileName);
    fs.writeFileSync(target, `${lines.join("\n").replace(/\s*$/, "")}\n`, "utf8");
    briefs.push(relativePosix(repoRoot, target));
  };

  if (normalization.domains.pulse.backup) {
    writeBrief("pulse-reconstruction-brief.md", [
      "# Pulse Runtime Reconstruction Brief",
      "",
      `Backup: ${normalization.domains.pulse.backup}`,
      "",
      "Known-safe runtime paths were copied into the v2 .pulse layout when possible.",
      "Review unmapped backup entries before treating them as current runtime truth.",
    ]);
  }

  if (normalization.domains.docs.backup) {
    writeBrief("docs-regeneration-brief.md", [
      "# Docs Regeneration Brief",
      "",
      `Backup: ${normalization.domains.docs.backup}`,
      "",
      "Read the backed-up docs and the current codebase, then regenerate target docs that conform to:",
      "",
      "- docs/ARCHITECTURE.md",
      "- docs/GLOSSARY.md",
      "- docs/decisions/",
      "- docs/product/",
      "",
      "Do not blindly copy the old docs structure back into active docs/.",
    ]);
  }

  if (normalization.domains.works.backup) {
    writeBrief("works-reconstruction-brief.md", [
      "# Works Reconstruction Brief",
      "",
      `Backup: ${normalization.domains.works.backup}`,
      "",
      "Read the backed-up work artifacts, infer the active work slices, and reconstruct them into:",
      "",
      "- works/epics/<E-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/SPEC.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/verification.md",
      "",
      "Synchronize reconstructed work with .pulse/workgraph/items.jsonl instead of preserving backed-up layout as active truth.",
    ]);
  }

  if (briefs.length > 0) {
    const manifestPath = path.join(reconstructionDir, "manifest.json");
    fs.writeFileSync(manifestPath, `${JSON.stringify({ schema_version: "1.0", generated_at: utcNow(), briefs }, null, 2)}\n`, "utf8");
  }

  return briefs;
}

function ensureDocsDomain(repoRoot, stamp) {
  const initial = classifyDocsDomain(repoRoot);
  const notes = [];
  const reconstructions = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureDocsScaffold(repoRoot);
    return { ...initial, backup, notes, reconstructions };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "docs"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "docs", stamp);
      backup = backupResult.backup;
      reconstructions.push(`docs active content -> ${backup}`);
      notes.push("docs domain was backed up and scaffolded; regenerate semantic docs from the onboarding reconstruction brief.");
    }
    ensureDocsScaffold(repoRoot);
  }

  return { ...initial, backup, notes, reconstructions };
}

function ensureWorksDomain(repoRoot, stamp) {
  const initial = classifyWorksDomain(repoRoot);
  const notes = [];
  const reconstructions = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureWorksScaffold(repoRoot);
    return { ...initial, backup, notes, reconstructions };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "works"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "works", stamp);
      backup = backupResult.backup;
      reconstructions.push(`works active content -> ${backup}`);
      notes.push("works domain was backed up and scaffolded; reconstruct work items from the onboarding reconstruction brief.");
    }
    ensureWorksScaffold(repoRoot);
  }

  return { ...initial, backup, notes, reconstructions };
}

/**
 * Normalize .pulse, docs, and works into the managed v2 layout.
 */
function buildDomainNormalization(repoRoot) {
  const stamp = backupStamp();
  const initial = classifyDomains(repoRoot);
  let pulseBackup = "";
  let pulseReconstructions = [];
  let pulseNotes = [];

  if (initial.pulse.status === "missing") {
    ensurePulseDomainLayout(repoRoot);
  } else if (initial.pulse.status === "non_compliant") {
    const backupResult = backupDomainInPlace(repoRoot, ".pulse", stamp);
    pulseBackup = backupResult.backup;
    ensurePulseDomainLayout(repoRoot);
    const reconstruction = restorePulseBackup(repoRoot, pulseBackup);
    pulseReconstructions = reconstruction.restored;
    pulseNotes = reconstruction.notes;
  } else {
    ensurePulseDomainLayout(repoRoot);
  }

  const docs = ensureDocsDomain(repoRoot, stamp);
  const works = ensureWorksDomain(repoRoot, stamp);
  const normalization = {
    backup_stamp: stamp,
    domains: {
      pulse: { ...initial.pulse, backup: pulseBackup, notes: pulseNotes, reconstructions: pulseReconstructions },
      docs,
      works,
    },
  };
  normalization.reconstruction_briefs = writeOnboardingReconstructionBriefs(repoRoot, normalization);
  return normalization;
}

/**
 * Managed AGENTS.md block helpers.
 */

function managedAgentsPresent(text) {
  return text.includes("<!-- PULSE:START -->") && text.includes("<!-- PULSE:END -->");
}

function mergeAgentsContent(existing, template) {
  const stripped = existing.trim();
  if (!stripped) {
    return { text: template, status: "created_from_template" };
  }

  if (managedAgentsPresent(existing)) {
    const updated = existing.replace(
      /<!-- PULSE:START -->[\s\S]*?<!-- PULSE:END -->\n?/,
      template,
    );
    return { text: `${updated.replace(/\s*$/, "")}\n`, status: "updated_managed_block" };
  }

  const glue = existing.endsWith("\n\n") ? "" : "\n\n";
  return {
    text: `${existing.replace(/\s*$/, "")}${glue}${template}`,
    status: "appended_managed_block",
  };
}

/**
 * Managed repo-local data asset helpers.
 */

/**
 * Detect whether managed repo-local data assets are stale.
 */
function supportAssetsNeedUpdate(repoRoot) {
  const harnessBacklogTarget = path.join(repoRoot, ".pulse", "harness", "HARNESS_BACKLOG.md");
  const source = fs.readFileSync(HARNESS_BACKLOG_TEMPLATE_PATH, "utf8");
  return !fs.existsSync(harnessBacklogTarget) || fs.readFileSync(harnessBacklogTarget, "utf8") !== source;
}

/**
 * Materialize managed Pulse data assets.
 */
function writeSupportAssets(repoRoot) {
  const written = [];
  const harnessDir = path.join(repoRoot, ".pulse", "harness");
  fs.mkdirSync(harnessDir, { recursive: true });
  const harnessBacklogTarget = path.join(harnessDir, "HARNESS_BACKLOG.md");
  fs.copyFileSync(HARNESS_BACKLOG_TEMPLATE_PATH, harnessBacklogTarget);
  fs.chmodSync(harnessBacklogTarget, 0o644);
  written.push(path.relative(repoRoot, harnessBacklogTarget));

  return written;
}

/**
 * Initialize workgraph storage and refresh derived views.
 */
function initializeWorkgraphFilesystem(repoRoot) {
  ensureWorkgraphFilesystem(repoRoot, { syncSchema: true });
  writeViews(repoRoot, loadItems(repoRoot));
  return getWorkgraphPaths(repoRoot);
}

/**
 * Public onboarding API.
 */

/**
 * Check onboarding readiness without modifying the repository.
 */
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
  const toolingStatusPreview = buildToolingStatusPayload(repoRoot, {
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
    tools: {
      git: { available: true },
      node: runtime,
      pulse_runtime_helper: { available: true, command: `${PULSE_COMMAND} status --repo-root <repo> --json` },
    },
    resumeOwner: options.resumeOwner,
  });

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

/**
 * Apply onboarding, normalize managed domains, and write runtime state.
 */
export function applyRepo(repoRoot, _allowCompactPromptReplace, options = {}) {
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

  const toolingStatusPayload = buildToolingStatusPayload(repoRoot, {
    requestedMode,
    recommendedMode,
    readinessStatus,
    onboardingStatus: "PASS",
    domainStatus,
    blockers,
    degradations,
    warnings,
    tools: {
      git: { available: true },
      node: runtime,
      pulse_runtime_helper: { available: true, command: `${PULSE_COMMAND} status --repo-root <repo> --json` },
    },
    resumeOwner: options.resumeOwner,
  });

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
    next_command_recommended: toolingStatusPayload.next_command || "pulse:workflow explore",
    next_skill_recommended: toolingStatusPayload.next_command || "pulse:workflow explore",
  });

  const toolingStatusPath = path.join(repoRoot, ".pulse", "runtime", "tooling-status.json");
  ensureParent(toolingStatusPath);
  fs.writeFileSync(toolingStatusPath, `${JSON.stringify(toolingStatusPayload, null, 2)}\n`, "utf8");
  fs.writeFileSync(statePath, `${JSON.stringify(nextState, null, 2)}\n`, "utf8");
  writeStateMarkdownFromTooling(repoRoot, toolingStatusPayload);

  syncPulseRuntimeArtifacts(repoRoot);

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
    ...checkRepo(repoRoot, { resumeOwner: options.resumeOwner }),
    applied: true,
    result: onboardingPayload,
  };
}

// Compact prompt draft, intentionally not installed by onboarding:
// MANDATORY: Pulse context compaction recovery.
// STOP. Before doing anything else: read AGENTS.md completely.
// Run the rendered pulse.mjs status command for a quick Pulse status snapshot.
// Read .pulse/runtime/tooling-status.json, .pulse/runtime/state.json, and .pulse/runtime/STATE.md if they exist.
// Read .pulse/runtime/handoffs/manifest.json and any active owner handoff you are resuming.
// Re-open the active work content before more planning or edits.
// Re-open the current work item before running more implementation commands.
// Check the current worktree state with git status before resuming.
// After completing these steps, briefly confirm what context you restored and only then continue.

/**
 * Helper functions.
 */

function readTemplate() {
  return `${fs.readFileSync(AGENTS_TEMPLATE_PATH, "utf8").replace(/\s*$/, "")}\n`;
}

function readTextIfExists(filePath) {
  return fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
}

function readJsonIfExists(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

function ensureParent(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function ensureDirectory(dirPath) {
  fs.mkdirSync(dirPath, { recursive: true });
}

function movePathIfExists(sourcePath, targetPath) {
  if (!fs.existsSync(sourcePath)) {
    return false;
  }
  ensureParent(targetPath);
  fs.renameSync(sourcePath, targetPath);
  return true;
}

function listDirectoryEntries(dirPath) {
  return fs.existsSync(dirPath) ? fs.readdirSync(dirPath) : [];
}

function isBackupEntry(name) {
  return /^backup-/.test(name);
}

function backupStamp() {
  return utcNow().replace(/[:.]/g, "-");
}

function copyPathIfExists(sourcePath, targetPath) {
  if (!fs.existsSync(sourcePath)) {
    return false;
  }
  ensureParent(targetPath);
  fs.cpSync(sourcePath, targetPath, { recursive: true, force: true });
  return true;
}

/**
 * CLI argument parsing and process entrypoint.
 */

/**
 * Parse onboard CLI arguments.
 */
function parseCliArgs(argv) {
  const args = {
    repoRoot: undefined,
    apply: false,
    resumeOwner: "",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repo-root") {
      args.repoRoot = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith("--repo-root=")) {
      args.repoRoot = arg.slice("--repo-root=".length);
      continue;
    }
    if (arg === "--apply") {
      args.apply = true;
      continue;
    }
    if (arg === "--resume-owner") {
      args.resumeOwner = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--resume-owner=")) {
      args.resumeOwner = arg.slice("--resume-owner=".length);
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage: onboard_pulse.mjs [--repo-root <path>] [--apply] [--resume-owner <owner_id>]",
          "",
          "Checks or applies pulse:workflow use readiness and session loading.",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

/**
 * Run the onboard CLI and print the JSON payload.
 */
export function main(argv = process.argv.slice(2)) {
  const args = parseCliArgs(argv);
  const repoRoot = resolveRepoRoot(args.repoRoot);
  const payload = args.apply
    ? applyRepo(repoRoot, false, { resumeOwner: args.resumeOwner })
    : checkRepo(repoRoot, { resumeOwner: args.resumeOwner });

  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
  return payload.status === "FAIL" ? 1 : 0;
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
