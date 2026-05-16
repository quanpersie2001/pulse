#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

import {
  buildDefaultState,
  normalizePulseState,
  syncPulseRuntimeArtifacts,
  getPulseStatePaths,
} from "../../../scripts/runtime/pulse_state.mjs";
import {
  collectPulseSkillDependencies,
  readDependencyHealthSafe,
  buildDependencyWarningSummary,
} from "../../../scripts/runtime/pulse_dependencies.mjs";
import {
  ensureWorkgraphFilesystem,
  getWorkgraphPaths,
  loadItems,
  writeViews,
} from "../../../scripts/runtime/workgraph_store.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const COMMAND_SCRIPT_DIR = path.dirname(SCRIPT_PATH);
const PULSE_SKILL_DIR = path.resolve(COMMAND_SCRIPT_DIR, "..", "..", "..");
const PULSE_RUNTIME_DIR = path.join(PULSE_SKILL_DIR, "scripts", "runtime");
const REPO_ROOT = path.resolve(PULSE_SKILL_DIR, "..", "..");
const PULSE_WORK_TEMPLATES_DIR = path.join(REPO_ROOT, "skills", "pulse", "templates", "works");
const HARNESS_BACKLOG_TEMPLATE_PATH = path.join(REPO_ROOT, "skills", "pulse", "templates", "HARNESS_BACKLOG.md");
const PLUGIN_MANIFEST_PATH = path.join(REPO_ROOT, ".codex-plugin", "plugin.json");
const AGENTS_TEMPLATE_PATH = path.join(REPO_ROOT, "AGENTS.template.md");
const ONBOARDING_SCHEMA_VERSION = "1.0";
const COMPACT_PROMPT_MARKER_START = "# PULSE: compact_prompt start";
const COMPACT_PROMPT_MARKER_END = "# PULSE: compact_prompt end";
const MIN_NODE_MAJOR = 18;
const LEGACY_HOOK_SCRIPT_FILENAMES = [
  "pulse_session_start.py",
  "pulse_pre_tool_use.py",
  "pulse_stop.py",
];
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
  "pulse_dependencies.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_dependencies.mjs"),
  "pulse_reservations.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_reservations.mjs"),
  "pulse_session_context.mjs": path.join(PULSE_RUNTIME_DIR, "pulse_session_context.mjs"),
  "onboard_pulse.mjs": path.join(PULSE_RUNTIME_DIR, "onboard_pulse.mjs"),
};
const MANAGED_SUPPORT_TEMPLATE_FILES = {
  "epic-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "epic-README.md"),
  "story-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "story-README.md"),
  "task-README.md": path.join(PULSE_WORK_TEMPLATES_DIR, "task-README.md"),
  "verification.md": path.join(PULSE_WORK_TEMPLATES_DIR, "verification.md"),
};
const MANAGED_SUPPORT_INVENTORY_FILE = "pulse_dependency_inventory.json";
const LEGACY_RUNTIME_TEXT_REPLACEMENTS = [
  [".pulse/tooling-status.json", ".pulse/runtime/tooling-status.json"],
  [".pulse/state.json", ".pulse/runtime/state.json"],
  [".pulse/STATE.md", ".pulse/runtime/STATE.md"],
  [".pulse/handoffs/manifest.json", ".pulse/runtime/handoffs/manifest.json"],
  [".pulse/handoffs/", ".pulse/runtime/handoffs/"],
  [".pulse/checkpoints/", ".pulse/runtime/checkpoints/"],
  [".pulse/reservations.json", ".pulse/runtime/reservations.json"],
];
const LEGACY_RUNTIME_FILE_MIGRATIONS = [
  [".pulse/state.json", ".pulse/runtime/state.json"],
  [".pulse/STATE.md", ".pulse/runtime/STATE.md"],
  [".pulse/tooling-status.json", ".pulse/runtime/tooling-status.json"],
  [".pulse/reservations.json", ".pulse/runtime/reservations.json"],
];
const LEGACY_RUNTIME_DIRECTORY_MIGRATIONS = [
  [".pulse/handoffs", ".pulse/runtime/handoffs"],
  [".pulse/checkpoints", ".pulse/runtime/checkpoints"],
];
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

function buildManagedDependencyInventory() {
  return `${JSON.stringify({
    schema_version: "1.0",
    skills: collectPulseSkillDependencies({
      repoRoot: REPO_ROOT,
      skillsRoot: path.join(REPO_ROOT, "skills"),
    }),
  }, null, 2)}\n`;
}

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

function migrateLegacyRuntimeFile(repoRoot, legacyRelativePath, canonicalRelativePath) {
  const sourcePath = path.join(repoRoot, legacyRelativePath);
  const targetPath = path.join(repoRoot, canonicalRelativePath);
  if (!fs.existsSync(sourcePath)) {
    return { migrated: [], removed_duplicates: [], conflicts: [] };
  }

  const sourceText = rewriteLegacyRuntimeText(fs.readFileSync(sourcePath, "utf8"));
  if (!fs.existsSync(targetPath)) {
    ensureParent(targetPath);
    fs.writeFileSync(targetPath, sourceText, "utf8");
    fs.rmSync(sourcePath, { force: true });
    return { migrated: [`${legacyRelativePath} -> ${canonicalRelativePath}`], removed_duplicates: [], conflicts: [] };
  }

  const targetText = fs.readFileSync(targetPath, "utf8");
  if (targetText === sourceText) {
    fs.rmSync(sourcePath, { force: true });
    return { migrated: [], removed_duplicates: [legacyRelativePath], conflicts: [] };
  }

  return {
    migrated: [],
    removed_duplicates: [],
    conflicts: [`${legacyRelativePath} != ${canonicalRelativePath}`],
  };
}

function migrateLegacyRuntimeDirectory(repoRoot, legacyRelativePath, canonicalRelativePath) {
  const sourcePath = path.join(repoRoot, legacyRelativePath);
  const targetPath = path.join(repoRoot, canonicalRelativePath);
  if (!fs.existsSync(sourcePath)) {
    return { migrated: [], removed_duplicates: [], conflicts: [] };
  }

  if (fs.existsSync(targetPath)) {
    return {
      migrated: [],
      removed_duplicates: [],
      conflicts: [`${legacyRelativePath} != ${canonicalRelativePath}`],
    };
  }

  ensureParent(targetPath);
  fs.renameSync(sourcePath, targetPath);
  rewriteLegacyRuntimeTree(targetPath);
  return {
    migrated: [`${legacyRelativePath} -> ${canonicalRelativePath}`],
    removed_duplicates: [],
    conflicts: [],
  };
}

function collectLegacyRuntimeMigrationSummary(repoRoot) {
  const summary = {
    pending: [],
    migrated: [],
    removed_duplicates: [],
    conflicts: [],
  };

  for (const [legacyRelativePath, canonicalRelativePath] of LEGACY_RUNTIME_FILE_MIGRATIONS) {
    const sourcePath = path.join(repoRoot, legacyRelativePath);
    const targetPath = path.join(repoRoot, canonicalRelativePath);
    if (!fs.existsSync(sourcePath)) {
      continue;
    }
    summary.pending.push(`${legacyRelativePath} -> ${canonicalRelativePath}`);
    if (fs.existsSync(targetPath)) {
      const sourceText = rewriteLegacyRuntimeText(fs.readFileSync(sourcePath, "utf8"));
      const targetText = fs.readFileSync(targetPath, "utf8");
      if (targetText !== sourceText) {
        summary.conflicts.push(`${legacyRelativePath} != ${canonicalRelativePath}`);
      }
    }
  }

  for (const [legacyRelativePath, canonicalRelativePath] of LEGACY_RUNTIME_DIRECTORY_MIGRATIONS) {
    const sourcePath = path.join(repoRoot, legacyRelativePath);
    const targetPath = path.join(repoRoot, canonicalRelativePath);
    if (!fs.existsSync(sourcePath)) {
      continue;
    }
    summary.pending.push(`${legacyRelativePath} -> ${canonicalRelativePath}`);
    if (fs.existsSync(targetPath)) {
      summary.conflicts.push(`${legacyRelativePath} != ${canonicalRelativePath}`);
    }
  }

  return summary;
}

function migrateLegacyRuntimeArtifacts(repoRoot) {
  const summary = {
    migrated: [],
    removed_duplicates: [],
    conflicts: [],
  };

  for (const [legacyRelativePath, canonicalRelativePath] of LEGACY_RUNTIME_FILE_MIGRATIONS) {
    const outcome = migrateLegacyRuntimeFile(repoRoot, legacyRelativePath, canonicalRelativePath);
    summary.migrated.push(...outcome.migrated);
    summary.removed_duplicates.push(...outcome.removed_duplicates);
    summary.conflicts.push(...outcome.conflicts);
  }

  for (const [legacyRelativePath, canonicalRelativePath] of LEGACY_RUNTIME_DIRECTORY_MIGRATIONS) {
    const outcome = migrateLegacyRuntimeDirectory(repoRoot, legacyRelativePath, canonicalRelativePath);
    summary.migrated.push(...outcome.migrated);
    summary.removed_duplicates.push(...outcome.removed_duplicates);
    summary.conflicts.push(...outcome.conflicts);
  }

  return summary;
}

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

function insertBeforeFirstTable(text, block) {
  const match = text.match(/^\[/m);
  if (match && match.index !== undefined) {
    return `${text.slice(0, match.index)}${block}\n${text.slice(match.index)}`;
  }
  return `${text.replace(/\s*$/, "")}${text.trim() ? "\n\n" : ""}${block}\n`;
}

function findProjectDocMaxBytes(text) {
  const match = text.match(/^project_doc_max_bytes\s*=\s*(.+)$/m);
  if (!match) {
    return undefined;
  }
  const value = Number.parseInt(match[1], 10);
  return Number.isFinite(value) ? value : undefined;
}

function upsertProjectDocMaxBytes(text, existingValue) {
  const desired = 65536;
  const line = `project_doc_max_bytes = ${desired}`;

  if (existingValue === undefined) {
    return insertBeforeFirstTable(text, `${line}\n`);
  }

  if (existingValue >= desired) {
    return text;
  }

  return text.replace(/^project_doc_max_bytes\s*=\s*.+$/m, line);
}

function findSectionRange(text, sectionName) {
  const lines = text.split("\n");
  let offset = 0;
  let start = null;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (start === null) {
      if (line.trim() === `[${sectionName}]`) {
        start = offset + line.length + 1;
      }
      offset += line.length + 1;
      continue;
    }

    if (/^\[[^\]]+\]\s*$/.test(line)) {
      return { start, end: offset };
    }
    offset += line.length + 1;
  }

  return start === null ? null : { start, end: text.length };
}

function featureSectionBody(text) {
  const range = findSectionRange(text, "features");
  return range ? text.slice(range.start, range.end) : null;
}

function isCodexHooksEnabled(text) {
  const body = featureSectionBody(text);
  return body ? /^codex_hooks\s*=\s*true\s*$/m.test(body) : false;
}

function upsertFeaturesCodexHooks(text) {
  const range = findSectionRange(text, "features");
  if (!range) {
    const block = "[features]\ncodex_hooks = true\n";
    const suffix = text && !text.endsWith("\n") ? "\n" : "";
    return `${text}${suffix}${text.trim() ? "\n" : ""}${block}`;
  }

  let body = text.slice(range.start, range.end);
  if (/^codex_hooks\s*=/m.test(body)) {
    body = body.replace(/^codex_hooks\s*=.*$/m, "codex_hooks = true");
  } else {
    if (body && !body.endsWith("\n")) {
      body += "\n";
    }
    body += "codex_hooks = true\n";
  }

  return `${text.slice(0, range.start)}${body}${text.slice(range.end)}`;
}

function renderCompactPromptBlock() {
  return [
    COMPACT_PROMPT_MARKER_START,
    'compact_prompt = """',
    "MANDATORY: Pulse context compaction recovery.",
    "",
    "STOP. Before doing anything else:",
    "1. Read AGENTS.md completely.",
    "2. If present, run `node .pulse/scripts/pulse_status.mjs --json` for a quick Pulse status snapshot.",
    "3. Read .pulse/runtime/tooling-status.json, .pulse/runtime/state.json, and .pulse/runtime/STATE.md if they exist.",
    "4. Read .pulse/runtime/handoffs/manifest.json and any active owner handoff you are resuming.",
    "5. Re-open the active feature CONTEXT.md before more planning or edits.",
    "6. Re-open the current bead or task before running more implementation commands.",
    "7. Check the current worktree state with git status before resuming.",
    "",
    "After completing these steps, briefly confirm what context you restored and only then continue.",
    '"""',
    COMPACT_PROMPT_MARKER_END,
    "",
  ].join("\n");
}

function hasManagedCompactPrompt(text) {
  return text.includes(COMPACT_PROMPT_MARKER_START) && text.includes(COMPACT_PROMPT_MARKER_END);
}

function hasCompactPrompt(text) {
  return /^compact_prompt\s*=/m.test(text);
}

function replaceExistingCompactPrompt(text, replacement) {
  const tripleQuotePattern = /^compact_prompt\s*=\s*"""[\s\S]*?^"""\s*$/m;
  if (tripleQuotePattern.test(text)) {
    return text.replace(tripleQuotePattern, replacement.replace(/\n$/, ""));
  }

  const singleLinePattern = /^compact_prompt\s*=.*$/m;
  if (singleLinePattern.test(text)) {
    return text.replace(singleLinePattern, replacement.replace(/\n$/, ""));
  }

  return insertBeforeFirstTable(text, replacement);
}

function mergeCompactPrompt(text, allowReplace) {
  if (hasManagedCompactPrompt(text)) {
    const updated = text.replace(
      new RegExp(
        `${escapeRegExp(COMPACT_PROMPT_MARKER_START)}[\\s\\S]*?${escapeRegExp(COMPACT_PROMPT_MARKER_END)}\\n?`,
      ),
      renderCompactPromptBlock(),
    );
    return {
      text: updated,
      compact_prompt_status: "managed",
    };
  }

  if (hasCompactPrompt(text) && !allowReplace) {
    return {
      text,
      compact_prompt_status: "conflict_preserved",
    };
  }

  if (hasCompactPrompt(text) && allowReplace) {
    return {
      text: replaceExistingCompactPrompt(text, renderCompactPromptBlock()),
      compact_prompt_status: "replaced",
    };
  }

  return {
    text: insertBeforeFirstTable(text, renderCompactPromptBlock()),
    compact_prompt_status: "installed",
  };
}

function mergeCodexConfig(configPath, allowCompactPromptReplace) {
  const existingText = readTextIfExists(configPath);
  const changes = [];

  let updatedText = existingText;
  const nextProjectDocText = upsertProjectDocMaxBytes(updatedText, findProjectDocMaxBytes(existingText));
  if (nextProjectDocText !== updatedText) {
    changes.push("set_project_doc_max_bytes");
    updatedText = nextProjectDocText;
  }

  const nextFeatureText = upsertFeaturesCodexHooks(updatedText);
  if (nextFeatureText !== updatedText) {
    changes.push("enable_codex_hooks_feature");
    updatedText = nextFeatureText;
  }

  const compactResult = mergeCompactPrompt(updatedText, allowCompactPromptReplace);
  if (compactResult.text !== updatedText) {
    changes.push(`compact_prompt_${compactResult.compact_prompt_status}`);
    updatedText = compactResult.text;
  } else if (compactResult.compact_prompt_status === "conflict_preserved") {
    changes.push("compact_prompt_conflict_preserved");
  }

  return {
    text: `${updatedText.replace(/\s*$/, "")}\n`,
    changes,
  };
}

function isPulseHook(hook) {
  const command = hook?.command || "";
  const status = hook?.statusMessage || "";
  return command.includes(".codex/hooks/pulse_") || status.startsWith("Pulse:");
}

function parseHooksJson(text) {
  if (!text.trim()) {
    return {};
  }
  return JSON.parse(text);
}

function cleanupLegacyPulseHookEntries(text) {
  const existing = text ? parseHooksJson(text) : {};
  const hooks = existing.hooks && typeof existing.hooks === "object" ? existing.hooks : {};
  const cleanedHooks = {};
  const changes = [];

  for (const [eventName, entries] of Object.entries(hooks)) {
    const currentEntries = Array.isArray(entries) ? entries : [];
    const nextEntries = [];

    for (const entry of currentEntries) {
      const hooksList = Array.isArray(entry?.hooks) ? entry.hooks : [];
      const preservedHooks = hooksList.filter((hook) => !isPulseHook(hook));
      if (preservedHooks.length === hooksList.length) {
        nextEntries.push(entry);
        continue;
      }
      if (preservedHooks.length > 0) {
        nextEntries.push({
          ...entry,
          hooks: preservedHooks,
        });
      }
    }

    if (nextEntries.length > 0) {
      cleanedHooks[eventName] = nextEntries;
    }
    if (JSON.stringify(currentEntries) !== JSON.stringify(nextEntries)) {
      changes.push(`remove_legacy_pulse_hooks_${eventName}`);
    }
  }

  const next = { ...existing };
  if (Object.keys(cleanedHooks).length > 0) {
    next.hooks = cleanedHooks;
  } else {
    delete next.hooks;
  }

  return {
    text: Object.keys(next).length > 0 ? `${JSON.stringify(next, null, 2)}\n` : "",
    changes,
  };
}

function legacyPulseHookConfigStatus(hooksText) {
  if (!hooksText) {
    return {
      exists: false,
      needs_cleanup: false,
      changes: [],
      text: "",
    };
  }

  try {
    const cleaned = cleanupLegacyPulseHookEntries(hooksText);
    return {
      exists: true,
      needs_cleanup: cleaned.text !== `${hooksText.replace(/\s*$/, "")}\n`,
      changes: cleaned.changes,
      text: cleaned.text,
    };
  } catch {
    return {
      exists: true,
      needs_cleanup: false,
      changes: [],
      text: `${hooksText.replace(/\s*$/, "")}\n`,
    };
  }
}

function legacyHookScriptsNeedCleanup(repoRoot) {
  const hooksDir = path.join(repoRoot, ".codex", "hooks");

  return LEGACY_HOOK_SCRIPT_FILENAMES.some((name) => fs.existsSync(path.join(hooksDir, name)));
}

function getManagedSupportScriptsDir(repoRoot) {
  return path.join(repoRoot, ".pulse", "scripts");
}

function getManagedSupportTemplatesDir(repoRoot) {
  return path.join(getManagedSupportScriptsDir(repoRoot), "templates", "works");
}

function supportScriptsNeedUpdate(repoRoot) {
  const supportDir = getManagedSupportScriptsDir(repoRoot);
  const templatesDir = getManagedSupportTemplatesDir(repoRoot);
  const inventoryPath = path.join(supportDir, MANAGED_SUPPORT_INVENTORY_FILE);
  const inventorySource = buildManagedDependencyInventory();

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

  if (!fs.existsSync(inventoryPath) || fs.readFileSync(inventoryPath, "utf8") !== inventorySource) {
    return true;
  }

  return false;
}

function legacySupportScriptsNeedCleanup(repoRoot) {
  return Object.keys(MANAGED_SUPPORT_FILES).some((name) =>
    fs.existsSync(path.join(repoRoot, ".codex", name)),
  );
}

function cleanupLegacyHookScripts(repoRoot) {
  const hooksDir = path.join(repoRoot, ".codex", "hooks");
  if (!fs.existsSync(hooksDir)) {
    return [];
  }

  const removed = [];
  for (const name of LEGACY_HOOK_SCRIPT_FILENAMES) {
    const target = path.join(hooksDir, name);
    if (fs.existsSync(target)) {
      fs.unlinkSync(target);
      removed.push(path.relative(repoRoot, target));
    }
  }

  if (fs.existsSync(hooksDir) && fs.readdirSync(hooksDir).length === 0) {
    fs.rmdirSync(hooksDir);
  }

  return removed;
}

function cleanupLegacySupportScripts(repoRoot) {
  const removed = [];

  for (const name of Object.keys(MANAGED_SUPPORT_FILES)) {
    const target = path.join(repoRoot, ".codex", name);
    if (fs.existsSync(target)) {
      fs.rmSync(target, { force: true });
      removed.push(path.relative(repoRoot, target));
    }
  }

  return removed;
}

function writeSupportScripts(repoRoot) {
  const supportDir = getManagedSupportScriptsDir(repoRoot);
  const templatesDir = getManagedSupportTemplatesDir(repoRoot);
  const inventoryPath = path.join(supportDir, MANAGED_SUPPORT_INVENTORY_FILE);
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
  fs.writeFileSync(inventoryPath, buildManagedDependencyInventory(), "utf8");
  fs.chmodSync(inventoryPath, 0o644);
  written.push(path.relative(repoRoot, inventoryPath));

  const harnessDir = path.join(repoRoot, ".pulse", "harness");
  fs.mkdirSync(harnessDir, { recursive: true });
  const harnessBacklogTarget = path.join(harnessDir, "HARNESS_BACKLOG.md");
  fs.copyFileSync(HARNESS_BACKLOG_TEMPLATE_PATH, harnessBacklogTarget);
  fs.chmodSync(harnessBacklogTarget, 0o644);
  written.push(path.relative(repoRoot, harnessBacklogTarget));

  return written;
}

function initializeWorkgraphFilesystem(repoRoot) {
  ensureWorkgraphFilesystem(repoRoot, { syncSchema: true });
  writeViews(repoRoot, loadItems(repoRoot));
  return getWorkgraphPaths(repoRoot);
}

function buildRuntimeBlockedPayload(repoRoot, action) {
  const runtime = getNodeRuntimeStatus();
  return {
    repo_root: repoRoot,
    status: "missing_runtime",
    action,
    requires_confirmation: false,
    actions: ["install_supported_node_runtime"],
    message: `Pulse requires Node.js ${MIN_NODE_MAJOR}+ before onboarding can continue. Install Node.js and rerun onboarding.`,
    details: {
      runtime,
    },
  };
}

export function checkRepo(repoRoot) {
  const runtime = getNodeRuntimeStatus();
  if (!runtime.supported) {
    return buildRuntimeBlockedPayload(repoRoot, "check");
  }

  const dependencyHealth = readDependencyHealthSafe(repoRoot);
  const dependencyWarning = buildDependencyWarningSummary(dependencyHealth);

  const pluginVersion = loadPluginVersion();
  const agentsPath = path.join(repoRoot, "AGENTS.md");
  const configPath = path.join(repoRoot, ".codex", "config.toml");
  const hooksPath = path.join(repoRoot, ".codex", "hooks.json");
  const onboardingPath = path.join(repoRoot, ".pulse", "onboarding.json");
  const statePath = path.join(repoRoot, ".pulse", "runtime", "state.json");
  const workgraphPaths = getWorkgraphPaths(repoRoot);

  const agentsText = readTextIfExists(agentsPath);
  const agentsExists = agentsText.trim() !== "";
  const managedAgents = agentsExists && managedAgentsPresent(agentsText);

  const configText = readTextIfExists(configPath);
  const hooksText = readTextIfExists(hooksPath);

  let onboarding = {};
  if (fs.existsSync(onboardingPath)) {
    try {
      onboarding = JSON.parse(fs.readFileSync(onboardingPath, "utf8"));
    } catch {
      onboarding = {};
    }
  }

  const compactPromptManaged = hasManagedCompactPrompt(configText);
  const compactPromptConflict = hasCompactPrompt(configText) && !compactPromptManaged;

  const actions = [];
  if (!agentsExists) {
    actions.push("create_AGENTS.md");
  } else if (!managedAgents) {
    actions.push("append_pulse_managed_block_to_AGENTS.md");
  }

  if (!configText) {
    actions.push("create_.codex/config.toml");
  } else {
    const projectDocMaxBytes = findProjectDocMaxBytes(configText);
    if (projectDocMaxBytes === undefined || projectDocMaxBytes < 65536) {
      actions.push("set_project_doc_max_bytes");
    }
    if (!isCodexHooksEnabled(configText)) {
      actions.push("enable_features.codex_hooks");
    }
    if (compactPromptConflict) {
      actions.push("compact_prompt_requires_confirmation");
    } else if (!compactPromptManaged) {
      actions.push("install_pulse_compact_prompt");
    }
  }

  const legacyHookConfig = legacyPulseHookConfigStatus(hooksText);
  if (legacyHookConfig.needs_cleanup) {
    actions.push("remove_legacy_pulse_hook_entries");
  }

  if (legacyHookScriptsNeedCleanup(repoRoot)) {
    actions.push("remove_legacy_pulse_hook_scripts");
  }

  if (legacySupportScriptsNeedCleanup(repoRoot)) {
    actions.push("remove_legacy_pulse_support_scripts");
  }

  if (supportScriptsNeedUpdate(repoRoot)) {
    actions.push("sync_pulse_support_scripts");
  }

  const legacyRuntimeMigration = collectLegacyRuntimeMigrationSummary(repoRoot);
  if (legacyRuntimeMigration.pending.length > 0) {
    actions.push("migrate_legacy_runtime_artifacts");
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
    actions.push("write_.pulse/onboarding.json");
  }

  return {
    repo_root: repoRoot,
    status: actions.length === 0 ? "up_to_date" : "needs_onboarding",
    actions,
    requires_confirmation: compactPromptConflict,
    details: {
      plugin_version: pluginVersion,
      agents_exists: agentsExists,
      agents_managed_block: managedAgents,
      config_exists: fs.existsSync(configPath),
      hooks_exists: legacyHookConfig.exists,
      legacy_hook_cleanup_actions: legacyHookConfig.changes,
      legacy_support_scripts: Object.keys(MANAGED_SUPPORT_FILES)
        .filter((name) => fs.existsSync(path.join(repoRoot, ".codex", name)))
        .map((name) => path.posix.join(".codex", name)),
      legacy_runtime_migration: legacyRuntimeMigration,
      compact_prompt_conflict: compactPromptConflict,
      onboarding_state: Object.keys(onboarding).length > 0 ? onboarding : null,
      state_exists: fs.existsSync(statePath),
      workgraph: {
        schema_exists: fs.existsSync(workgraphPaths.schemaPath),
        items_exists: fs.existsSync(workgraphPaths.itemsPath),
        views: Object.fromEntries(
          Object.entries(workgraphPaths.viewPaths).map(([name, filePath]) => [name, fs.existsSync(filePath)]),
        ),
      },
      runtime,
      dependency_health: dependencyHealth,
      dependency_warning: dependencyWarning,
    },
  };
}

export function applyRepo(repoRoot, allowCompactPromptReplace) {
  const runtime = getNodeRuntimeStatus();
  if (!runtime.supported) {
    return buildRuntimeBlockedPayload(repoRoot, "apply");
  }

  const pluginVersion = loadPluginVersion();
  const template = readTemplate();

  const agentsPath = path.join(repoRoot, "AGENTS.md");
  const configPath = path.join(repoRoot, ".codex", "config.toml");
  const hooksPath = path.join(repoRoot, ".codex", "hooks.json");
  const onboardingPath = path.join(repoRoot, ".pulse", "onboarding.json");
  const statePath = path.join(repoRoot, ".pulse", "runtime", "state.json");
  const checkpointsRootPath = path.join(repoRoot, ".pulse", "runtime", "checkpoints");
  const memoryRootPath = path.join(repoRoot, ".pulse", "memory");
  const memoryLearningsPath = path.join(memoryRootPath, "learnings");
  const memoryCorrectionsPath = path.join(memoryRootPath, "corrections");
  const memoryRatchetPath = path.join(memoryRootPath, "ratchet");
  const legacyRuntimeMigration = migrateLegacyRuntimeArtifacts(repoRoot);

  ensureParent(agentsPath);
  ensureParent(configPath);
  ensureParent(onboardingPath);
  ensureParent(statePath);
  fs.mkdirSync(checkpointsRootPath, { recursive: true });
  fs.mkdirSync(memoryLearningsPath, { recursive: true });
  fs.mkdirSync(memoryCorrectionsPath, { recursive: true });
  fs.mkdirSync(memoryRatchetPath, { recursive: true });

  const mergedAgents = mergeAgentsContent(readTextIfExists(agentsPath), template);
  fs.writeFileSync(agentsPath, mergedAgents.text, "utf8");

  const configResult = mergeCodexConfig(configPath, allowCompactPromptReplace);
  fs.writeFileSync(configPath, configResult.text, "utf8");

  const legacyHookConfig = legacyPulseHookConfigStatus(readTextIfExists(hooksPath));
  if (legacyHookConfig.needs_cleanup) {
    if (legacyHookConfig.text) {
      fs.writeFileSync(hooksPath, legacyHookConfig.text, "utf8");
    } else {
      fs.rmSync(hooksPath, { force: true });
    }
  }

  const defaultState = buildDefaultState();
  const nextState = normalizePulseState({
    ...defaultState,
    ...readJsonIfExists(statePath),
  });
  fs.writeFileSync(statePath, `${JSON.stringify(nextState, null, 2)}\n`, "utf8");
  syncPulseRuntimeArtifacts(repoRoot);
  const workgraphPaths = initializeWorkgraphFilesystem(repoRoot);

  const legacyHookScripts = cleanupLegacyHookScripts(repoRoot);
  const legacySupportScripts = cleanupLegacySupportScripts(repoRoot);
  const supportScripts = writeSupportScripts(repoRoot);

  const onboardingNotes = [];
  let status = "complete";
  if (configResult.changes.includes("compact_prompt_conflict_preserved")) {
    status = "partial";
    onboardingNotes.push(
      "Existing compact_prompt preserved; Pulse compaction recovery was not installed.",
    );
  }

  const onboardingPayload = {
    schema_version: ONBOARDING_SCHEMA_VERSION,
    plugin: "pulse",
    plugin_version: pluginVersion,
    installed_at: utcNow(),
    status,
    managed_assets: {
      agents_mode: mergedAgents.status,
      config_changes: configResult.changes,
      legacy_hook_cleanup: legacyHookConfig.changes,
      legacy_hook_scripts_removed: legacyHookScripts,
      legacy_support_scripts_removed: legacySupportScripts,
      support_scripts: supportScripts,
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
  if (
    legacyRuntimeMigration.migrated.length > 0 ||
    legacyRuntimeMigration.removed_duplicates.length > 0 ||
    legacyRuntimeMigration.conflicts.length > 0
  ) {
    onboardingPayload.managed_assets.migration_summary = legacyRuntimeMigration;
  }
  fs.writeFileSync(`${onboardingPath}`, `${JSON.stringify(onboardingPayload, null, 2)}\n`, "utf8");

  return {
    ...checkRepo(repoRoot),
    applied: true,
    result: onboardingPayload,
  };
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function parseCliArgs(argv) {
  const args = {
    repoRoot: undefined,
    apply: false,
    allowCompactPromptReplace: false,
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
    if (arg === "--allow-compact-prompt-replace") {
      args.allowCompactPromptReplace = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage: onboard_pulse.mjs [--repo-root <path>] [--apply] [--allow-compact-prompt-replace]",
          "",
          "Checks or applies /pulse onboard runtime onboarding.",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

export function main(argv = process.argv.slice(2)) {
  const args = parseCliArgs(argv);
  const repoRoot = resolveRepoRoot(args.repoRoot);
  const payload = args.apply
    ? applyRepo(repoRoot, args.allowCompactPromptReplace)
    : checkRepo(repoRoot);

  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
  return payload.status === "missing_runtime" ? 1 : 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  process.exitCode = main();
}
