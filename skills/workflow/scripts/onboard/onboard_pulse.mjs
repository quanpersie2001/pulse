#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import {
  buildDefaultState,
  normalizePulseState,
  syncPulseRuntimeArtifacts,
} from "../runtime/pulse_state.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  loadItems,
  writeViews,
} from "../runtime/workgraph_store.mjs";
import { buildSessionLoad } from "./load_context.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const COMMAND_SCRIPT_DIR = path.dirname(SCRIPT_PATH);
const PULSE_SKILL_DIR = path.resolve(COMMAND_SCRIPT_DIR, "..", "..");
const PULSE_RUNTIME_DIR = path.join(PULSE_SKILL_DIR, "scripts", "runtime");
const REPO_ROOT = path.resolve(PULSE_SKILL_DIR, "..", "..");
const PULSE_WORK_TEMPLATES_DIR = path.join(REPO_ROOT, "skills", "workflow", "templates", "works");
const HARNESS_BACKLOG_TEMPLATE_PATH = path.join(REPO_ROOT, "skills", "workflow", "templates", "HARNESS_BACKLOG.md");
const PLUGIN_MANIFEST_PATH = path.join(REPO_ROOT, ".codex-plugin", "plugin.json");
const AGENTS_TEMPLATE_PATH = path.join(REPO_ROOT, "AGENTS.template.md");
const ONBOARDING_SCHEMA_VERSION = "1.0";
const WORKFLOW_COMMAND = "use";
const WORKFLOW_SETUP_STEP = "onboarding";
const ONBOARDING_MARKER_PATH = path.join(".pulse", "runtime", "onboarding.json");
const LEGACY_ONBOARDING_MARKER_PATH = path.join(".pulse", "onboarding.json");
const MIN_NODE_MAJOR = 18;
const MANAGED_SUPPORT_FILES = {
  "pulse-work": path.join(PULSE_RUNTIME_DIR, "pulse-work"),
  "pulse_work.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_work.mjs"),
  "workgraph_model.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_model.mjs"),
  "workgraph_ids.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_ids.mjs"),
  "workgraph_paths.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_paths.mjs"),
  "workgraph_views.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_views.mjs"),
  "workgraph_lock.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_lock.mjs"),
  "workgraph_validate.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_validate.mjs"),
  "workgraph_store.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_store.mjs"),
  "workgraph_templates.mjs": path.join(PULSE_RUNTIME_DIR, "workgraph_templates.mjs"),
  "pulse_status.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_status.mjs"),
  "pulse_state.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_state.mjs"),
  "pulse_reservations.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_reservations.mjs"),
  "pulse_session_context.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_session_context.mjs"),
  "load_context.mjs": path.join(COMMAND_SCRIPT_DIR, "load_context.mjs"),
  "onboard_pulse.mjs": path.join(COMMAND_SCRIPT_DIR, "onboard_pulse.mjs"),
};
const MANAGED_SUPPORT_TEMPLATE_FILES = {
  "epic-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "epic-README.md"),
  "story-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "story-README.md"),
  "story-SPEC.md": path.join(PULSE_WORK_TEMPLATES_DIR, "story-SPEC.md"),
  "task-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "task-README.md"),
  "verification.md": path.join(PULSE_WORK_TEMPLATES_DIR, "verification.md"),
};
const LEGACY_RUNTIME_TEXT_REPLACEMENTS = [
  [".pulse/tooling-status.json", ".pulse/runtime/tooling-status.json"],
  [".pulse/state.json", ".pulse/runtime/state.json"],
  [".pulse/STATE.md", ".pulse/runtime/STATE.md"],
  [".pulse/handoffs/manifest.json", ".pulse/runtime/handoffs/manifest.json"],
  [".pulse/handoffs/", ".pulse/runtime/handoffs/"],
  [".pulse/checkpoints/", ".pulse/runtime/checkpoints/"],
  [".pulse/reservations.json", ".pulse/runtime/reservations.json"],
];

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
export function resolveRepoRoot(explicitRoot) {
  if (explicitRoot) {
    return path.resolve(explicitRoot);
  }

  const cwd = path.resolve(process.cwd());
  try {
    const stdout = execFileSync("git", ["rev-parse", "--show-toplevel"], {
      cwd,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return path.resolve(stdout.trim());
  } catch {
    let candidate = cwd;
    while (true) {
      if (fs.existsSync(path.join(candidate, ".git"))) {
        return candidate;
      }
      const parent = path.dirname(candidate);
      if (parent === candidate) {
        return cwd;
      }
      candidate = parent;
    }
  }
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
    return { status: "missing", missing: [".pulse"], unexpected_legacy: [], conflicts: [] };
  }

  const required = [
    ["runtime", "directory"],
    [path.join("runtime", "handoffs"), "directory"],
    [path.join("runtime", "checkpoints"), "directory"],
    ["workgraph", "directory"],
    [path.join("workgraph", "views"), "directory"],
    ["scripts", "directory"],
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

  const unexpectedLegacy = [];
  for (const legacy of ["current-feature.json", "runtime-snapshot.json", "reservations.json", "state.json", "STATE.md", "tooling-status.json"]) {
    if (fs.existsSync(path.join(pulsePath, legacy))) {
      unexpectedLegacy.push(path.posix.join(".pulse", legacy));
    }
  }
  for (const legacyDir of ["handoffs", "checkpoints"]) {
    if (fs.existsSync(path.join(pulsePath, legacyDir))) {
      unexpectedLegacy.push(path.posix.join(".pulse", legacyDir));
    }
  }

  return {
    status: missing.length === 0 && unexpectedLegacy.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_legacy: unexpectedLegacy,
    conflicts: [],
  };
}

/**
 * Classify the docs domain against the expected semantic docs scaffold.
 */
function classifyDocsDomain(repoRoot) {
  const docsPath = path.join(repoRoot, "docs");
  if (!fs.existsSync(docsPath)) {
    return { status: "missing", missing: ["docs"], unexpected_legacy: [], conflicts: [] };
  }
  const required = ["ARCHITECTURE.md", "GLOSSARY.md", "decisions", "product"];
  const missing = required.filter((entry) => !fs.existsSync(path.join(docsPath, entry)));
  return {
    status: missing.length === 0 ? "compliant" : "non_compliant",
    missing: missing.map((entry) => path.posix.join("docs", entry)),
    unexpected_legacy: [],
    conflicts: [],
  };
}

/**
 * Classify the works domain against the expected story-first work layout.
 */
function classifyWorksDomain(repoRoot) {
  const worksPath = path.join(repoRoot, "works");
  if (!fs.existsSync(worksPath)) {
    return { status: "missing", missing: ["works"], unexpected_legacy: [], conflicts: [] };
  }

  const activeEntries = listActiveDomainEntries(worksPath);
  const allowedTopLevel = new Set(["epics", "backlog.md", "test-matrix.md"]);
  const unexpectedLegacy = activeEntries.filter((entry) => !allowedTopLevel.has(entry));
  const missing = fs.existsSync(path.join(worksPath, "epics")) ? [] : ["works/epics"];

  return {
    status: missing.length === 0 && unexpectedLegacy.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_legacy: unexpectedLegacy.map((entry) => path.posix.join("works", entry)),
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

function readOnboardingState(repoRoot, onboardingPath) {
  const legacyPath = path.join(repoRoot, LEGACY_ONBOARDING_MARKER_PATH);
  let onboarding = readJsonIfExists(onboardingPath) || {};
  let legacyMigrated = false;
  if (Object.keys(onboarding).length === 0 && fs.existsSync(legacyPath)) {
    onboarding = readJsonIfExists(legacyPath) || {};
  }
  if (fs.existsSync(legacyPath) && !fs.existsSync(onboardingPath)) {
    ensureParent(onboardingPath);
    fs.copyFileSync(legacyPath, onboardingPath);
    fs.rmSync(legacyPath, { force: true });
    legacyMigrated = true;
  } else if (fs.existsSync(legacyPath)) {
    fs.rmSync(legacyPath, { force: true });
    legacyMigrated = true;
  }
  return { onboarding, legacyMigrated };
}

function ensurePulseDomainLayout(repoRoot) {
  for (const relative of [
    [".pulse", "runtime"],
    [".pulse", "runtime", "handoffs"],
    [".pulse", "runtime", "checkpoints"],
    [".pulse", "runtime", "onboarding-migration"],
    [".pulse", "workgraph"],
    [".pulse", "workgraph", "views"],
    [".pulse", "scripts"],
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
function migratePulseBackup(repoRoot, backupRelativePath) {
  const migrated = [];
  const notes = [];
  if (!backupRelativePath) {
    return { migrated, notes };
  }

  const backupAbsolute = path.join(repoRoot, backupRelativePath);
  const copyIfPresent = (from, to) => {
    if (copyPathIfExists(path.join(backupAbsolute, from), path.join(repoRoot, to))) {
      migrated.push(`${path.posix.join(backupRelativePath, from.split(path.sep).join(path.posix.sep))} -> ${to.split(path.sep).join(path.posix.sep)}`);
    }
  };
  const copyTextIfPresent = (from, to) => {
    if (copyRewrittenTextIfExists(path.join(backupAbsolute, from), path.join(repoRoot, to))) {
      migrated.push(`${path.posix.join(backupRelativePath, from.split(path.sep).join(path.posix.sep))} -> ${to.split(path.sep).join(path.posix.sep)}`);
    }
  };

  copyTextIfPresent("state.json", path.join(".pulse", "runtime", "state.json"));
  copyTextIfPresent("STATE.md", path.join(".pulse", "runtime", "STATE.md"));
  copyTextIfPresent("tooling-status.json", path.join(".pulse", "runtime", "tooling-status.json"));
  copyTextIfPresent("reservations.json", path.join(".pulse", "runtime", "reservations.json"));
  copyIfPresent("handoffs", path.join(".pulse", "runtime", "handoffs"));
  copyIfPresent("checkpoints", path.join(".pulse", "runtime", "checkpoints"));
  copyIfPresent("runtime", path.join(".pulse", "runtime"));
  copyIfPresent("memory", path.join(".pulse", "memory"));
  copyIfPresent("workgraph", path.join(".pulse", "workgraph"));

  const unmapped = listActiveDomainEntries(backupAbsolute).filter(
    (entry) => !["state.json", "STATE.md", "tooling-status.json", "reservations.json", "handoffs", "checkpoints", "runtime", "memory", "workgraph"].includes(entry),
  );
  if (unmapped.length > 0) {
    notes.push(`Unmapped .pulse backup entries require review: ${unmapped.join(", ")}.`);
  }

  return { migrated, notes };
}

/**
 * Write operator briefs for content that was backed up during normalization.
 */
function writeOnboardingMigrationBriefs(repoRoot, normalization) {
  const migrationDir = path.join(repoRoot, ".pulse", "runtime", "onboarding-migration");
  ensureDirectory(migrationDir);
  const briefs = [];

  const writeBrief = (fileName, lines) => {
    const target = path.join(migrationDir, fileName);
    fs.writeFileSync(target, `${lines.join("\n").replace(/\s*$/, "")}\n`, "utf8");
    briefs.push(relativePosix(repoRoot, target));
  };

  if (normalization.domains.pulse.backup) {
    writeBrief("pulse-migration-brief.md", [
      "# Pulse Runtime Migration Brief",
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
    writeBrief("works-migration-brief.md", [
      "# Works Migration Brief",
      "",
      `Backup: ${normalization.domains.works.backup}`,
      "",
      "Read the backed-up work artifacts, infer the active work slices, and migrate them into:",
      "",
      "- works/epics/<E-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/SPEC.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/verification.md",
      "",
      "Synchronize migrated work with .pulse/workgraph/items.jsonl instead of preserving legacy layout as active truth.",
    ]);
  }

  if (briefs.length > 0) {
    const manifestPath = path.join(migrationDir, "manifest.json");
    fs.writeFileSync(manifestPath, `${JSON.stringify({ schema_version: "1.0", generated_at: utcNow(), briefs }, null, 2)}\n`, "utf8");
  }

  return briefs;
}

function ensureDocsDomain(repoRoot, stamp) {
  const initial = classifyDocsDomain(repoRoot);
  const notes = [];
  const migrations = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureDocsScaffold(repoRoot);
    return { ...initial, backup, notes, migrations };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "docs"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "docs", stamp);
      backup = backupResult.backup;
      migrations.push(`docs active content -> ${backup}`);
      notes.push("docs domain was backed up and scaffolded; regenerate semantic docs from the onboarding migration brief.");
    }
    ensureDocsScaffold(repoRoot);
  }

  return { ...initial, backup, notes, migrations };
}

function ensureWorksDomain(repoRoot, stamp) {
  const initial = classifyWorksDomain(repoRoot);
  const notes = [];
  const migrations = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureWorksScaffold(repoRoot);
    return { ...initial, backup, notes, migrations };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "works"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "works", stamp);
      backup = backupResult.backup;
      migrations.push(`works active content -> ${backup}`);
      notes.push("works domain was backed up and scaffolded; migrate work items from the onboarding migration brief.");
    }
    ensureWorksScaffold(repoRoot);
  }

  return { ...initial, backup, notes, migrations };
}

/**
 * Normalize .pulse, docs, and works into the managed v2 layout.
 */
function buildDomainNormalization(repoRoot) {
  const stamp = backupStamp();
  const initial = classifyDomains(repoRoot);
  let pulseBackup = "";
  let pulseMigrations = [];
  let pulseNotes = [];

  if (initial.pulse.status === "missing") {
    ensurePulseDomainLayout(repoRoot);
  } else if (initial.pulse.status === "non_compliant") {
    const backupResult = backupDomainInPlace(repoRoot, ".pulse", stamp);
    pulseBackup = backupResult.backup;
    ensurePulseDomainLayout(repoRoot);
    const migration = migratePulseBackup(repoRoot, pulseBackup);
    pulseMigrations = migration.migrated;
    pulseNotes = migration.notes;
  } else {
    ensurePulseDomainLayout(repoRoot);
  }

  const docs = ensureDocsDomain(repoRoot, stamp);
  const works = ensureWorksDomain(repoRoot, stamp);
  const normalization = {
    backup_stamp: stamp,
    domains: {
      pulse: { ...initial.pulse, backup: pulseBackup, notes: pulseNotes, migrations: pulseMigrations },
      docs,
      works,
    },
  };
  normalization.migration_briefs = writeOnboardingMigrationBriefs(repoRoot, normalization);
  return normalization;
}

/**
 * Legacy runtime text rewriting helpers.
 */

function rewriteLegacyRuntimeText(text) {
  let next = String(text || "");
  for (const [from, to] of LEGACY_RUNTIME_TEXT_REPLACEMENTS) {
    next = next.replaceAll(from, to);
  }
  return next;
}

function rewriteLegacyRuntimeTree(rootPath) {
  if (!fs.existsSync(rootPath)) {
    return;
  }

  for (const entry of fs.readdirSync(rootPath, { withFileTypes: true })) {
    const entryPath = path.join(rootPath, entry.name);
    if (entry.isDirectory()) {
      rewriteLegacyRuntimeTree(entryPath);
      continue;
    }
    if (!entry.isFile() || !/\.(json|md|txt)$/i.test(entry.name)) {
      continue;
    }
    const original = fs.readFileSync(entryPath, "utf8");
    const rewritten = rewriteLegacyRuntimeText(original);
    if (rewritten !== original) {
      fs.writeFileSync(entryPath, rewritten, "utf8");
    }
  }
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
 * Managed support asset helpers.
 */

function getManagedSupportScriptsDir(repoRoot) {
  return path.join(repoRoot, ".pulse", "scripts");
}

function getManagedSupportTemplatesDir(repoRoot) {
  return path.join(getManagedSupportScriptsDir(repoRoot), "templates", "works");
}

/**
 * Detect whether managed repo-local support scripts or templates are stale.
 */
function supportScriptsNeedUpdate(repoRoot) {
  const supportDir = getManagedSupportScriptsDir(repoRoot);
  const templatesDir = getManagedSupportTemplatesDir(repoRoot);

  for (const [name, sourcePath] of Object.entries(MANAGED_SUPPORT_FILES)) {
    const targetPath = path.join(supportDir, name);
    const source = fs.readFileSync(sourcePath, "utf8");
    if (!fs.existsSync(targetPath) || fs.readFileSync(targetPath, "utf8") !== source) {
      return true;
    }
  }

  for (const [name, sourcePath] of Object.entries(MANAGED_SUPPORT_TEMPLATE_FILES)) {
    const targetPath = path.join(templatesDir, name);
    const source = fs.readFileSync(sourcePath, "utf8");
    if (!fs.existsSync(targetPath) || fs.readFileSync(targetPath, "utf8") !== source) {
      return true;
    }
  }

  return false;
}

/**
 * Materialize managed Pulse support scripts, templates, and harness backlog.
 */
function writeSupportScripts(repoRoot) {
  const supportDir = getManagedSupportScriptsDir(repoRoot);
  const templatesDir = getManagedSupportTemplatesDir(repoRoot);
  fs.mkdirSync(supportDir, { recursive: true });
  fs.mkdirSync(templatesDir, { recursive: true });

  const written = [];
  for (const [name, sourcePath] of Object.entries(MANAGED_SUPPORT_FILES)) {
    const target = path.join(supportDir, name);
    fs.copyFileSync(sourcePath, target);
    fs.chmodSync(target, 0o755);
    written.push(path.relative(repoRoot, target));
  }
  for (const [name, sourcePath] of Object.entries(MANAGED_SUPPORT_TEMPLATE_FILES)) {
    const target = path.join(templatesDir, name);
    fs.copyFileSync(sourcePath, target);
    fs.chmodSync(target, 0o644);
    written.push(path.relative(repoRoot, target));
  }
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

  const legacyOnboardingPath = path.join(repoRoot, LEGACY_ONBOARDING_MARKER_PATH);
  const onboarding =
    readJsonIfExists(onboardingPath) ||
    readJsonIfExists(legacyOnboardingPath) ||
    {};
  const onboardingMarkerExists = fs.existsSync(onboardingPath) || fs.existsSync(legacyOnboardingPath);

  const domainDetails = classifyDomains(repoRoot);
  const domainStatus = domainStatusSummary(domainDetails);

  const actions = [];
  if (!agentsExists) {
    actions.push("create_AGENTS.md");
  } else if (!managedAgents) {
    actions.push("append_pulse_managed_block_to_AGENTS.md");
  }

  if (supportScriptsNeedUpdate(repoRoot)) {
    actions.push("sync_pulse_support_scripts");
  }

  if (fs.existsSync(legacyOnboardingPath)) {
    actions.push("migrate_legacy_onboarding_marker");
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
      pulse_runtime_helper: { available: true, command: "node .pulse/scripts/pulse_status.mjs --json" },
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
      legacy_onboarding_marker_exists: fs.existsSync(legacyOnboardingPath),
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
  const legacyOnboardingPath = path.join(repoRoot, LEGACY_ONBOARDING_MARKER_PATH);
  const hadLegacyOnboardingMarker = fs.existsSync(legacyOnboardingPath);
  const domainNormalization = buildDomainNormalization(repoRoot);

  const agentsPath = path.join(repoRoot, "AGENTS.md");
  const onboardingPath = path.join(repoRoot, ONBOARDING_MARKER_PATH);
  const statePath = path.join(repoRoot, ".pulse", "runtime", "state.json");
  const checkpointsRootPath = path.join(repoRoot, ".pulse", "runtime", "checkpoints");
  const memoryRootPath = path.join(repoRoot, ".pulse", "memory");
  const memoryLearningsPath = path.join(memoryRootPath, "learnings");
  const memoryCorrectionsPath = path.join(memoryRootPath, "corrections");
  const memoryRatchetPath = path.join(memoryRootPath, "ratchet");
  const { onboarding: existingOnboarding, legacyMigrated: legacyOnboardingMarkerMigrated } = readOnboardingState(repoRoot, onboardingPath);

  ensureParent(agentsPath);
  ensureParent(onboardingPath);
  ensureParent(statePath);
  fs.mkdirSync(checkpointsRootPath, { recursive: true });
  fs.mkdirSync(memoryLearningsPath, { recursive: true });
  fs.mkdirSync(memoryCorrectionsPath, { recursive: true });
  fs.mkdirSync(memoryRatchetPath, { recursive: true });

  const mergedAgents = mergeAgentsContent(readTextIfExists(agentsPath), template);
  fs.writeFileSync(agentsPath, mergedAgents.text, "utf8");

  const supportScripts = writeSupportScripts(repoRoot);
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
      pulse_runtime_helper: { available: true, command: "node .pulse/scripts/pulse_status.mjs --json" },
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
      support_scripts: supportScripts,
      onboarding_marker_path: ONBOARDING_MARKER_PATH,
      legacy_onboarding_marker_migrated: legacyOnboardingMarkerMigrated || hadLegacyOnboardingMarker,
      domain_normalization: domainNormalization,
      works_migrations: domainNormalization.domains.works.migrations,
      docs_migrations: domainNormalization.domains.docs.migrations,
      workgraph: {
        schema: path.relative(repoRoot, workgraphPaths.schemaPath),
        items: path.relative(repoRoot, workgraphPaths.itemsPath),
        views: Object.fromEntries(
          Object.entries(workgraphPaths.viewPaths).map(([name, filePath]) => [name, path.relative(repoRoot, filePath)]),
        ),
      },
      state_file: path.relative(repoRoot, statePath),
      checkpoints_root: path.relative(repoRoot, checkpointsRootPath),
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
// If present, run `node .pulse/scripts/pulse_status.mjs --json` for a quick Pulse status snapshot.
// Read .pulse/runtime/tooling-status.json, .pulse/runtime/state.json, and .pulse/runtime/STATE.md if they exist.
// Read .pulse/runtime/handoffs/manifest.json and any active owner handoff you are resuming.
// Re-open the active feature CONTEXT.md before more planning or edits.
// Re-open the current bead or task before running more implementation commands.
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

function relativePosix(repoRoot, filePath) {
  return path.relative(repoRoot, filePath).split(path.sep).join(path.posix.sep);
}

function copyPathIfExists(sourcePath, targetPath) {
  if (!fs.existsSync(sourcePath)) {
    return false;
  }
  ensureParent(targetPath);
  fs.cpSync(sourcePath, targetPath, { recursive: true, force: true });
  return true;
}

function copyRewrittenTextIfExists(sourcePath, targetPath) {
  if (!fs.existsSync(sourcePath) || !fs.statSync(sourcePath).isFile()) {
    return false;
  }
  ensureParent(targetPath);
  fs.writeFileSync(targetPath, rewriteLegacyRuntimeText(fs.readFileSync(sourcePath, "utf8")), "utf8");
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

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  process.exitCode = main();
}
