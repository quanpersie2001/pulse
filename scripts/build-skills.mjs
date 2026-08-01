#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getPulseCommand, PROVIDERS, WORKFLOW_SKILL_NAME } from "./lib/providers.mjs";
import {
  assertNoUnresolvedRuntimePlaceholders,
  renderPulsePlaceholders,
} from "./lib/render-pulse-placeholders.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const DIST_DIR = path.join(REPO_ROOT, "dist");
const PLUGIN_MANIFEST_PATHS = [
  path.join(REPO_ROOT, ".claude-plugin", "plugin.json"),
  path.join(REPO_ROOT, ".codex-plugin", "plugin.json"),
];
const TEXT_EXTENSIONS = new Set([
  ".css",
  ".html",
  ".js",
  ".json",
  ".md",
  ".mjs",
  ".sh",
  ".svg",
  ".toml",
  ".txt",
  ".yaml",
  ".yml",
]);
const TEXT_FILENAMES = new Set(["SKILL.md"]);
const EXCLUDED_DIRS = new Set(["tests"]);
const LEGACY_CANONICAL_PATHS = [
  "items.jsonl",
  "runtime/state.json",
  "runtime/STATE.md",
  "runtime/reservations.json",
];
const LEGACY_GUIDANCE_MARKERS = [
  ".pulse/runtime",
  ".pulse/harness",
  ".pulse/scripts",
  ".pulse/workgraph/schema.json",
  ".pulse/workgraph/views/",
  "tooling-status.json",
  "session-load",
  "runtime mirror",
  "handoff manifest",
  "backup-",
  "backed-up",
  "pulse work doctor",
  "pulse work dep",
  "pulse work link",
  "workgraph create",
  "workgraph dep",
  "workgraph link",
  "workgraph views",
  "pulse work update",
  "pulse work close",
  "pulse work reopen",
  "--to done",
  "T-",
  "B-",
  "TASK",
  "BUG",
  "pulse status ",
  "pulse ready ",
  "pulse reservation ",
  "{{pulse_command}}",
  "the Rust `pulse` executable`",
  "content_dir=works/epics",
  "works/epics/",
];
const REQUIRED_WORKFLOW_CONTRACT_MARKERS = [
  "Canonical statuses are `DRAFT`, `SHAPED`, `READY`, `ACTIVE`, `VERIFYING`",
  "the canonical item status is `READY`",
  "--role implementation",
  "--risk low",
  "--materialization R1",
  "MutationOutcome.value",
  "Node.content_dir",
  "`content_dir` is exactly `works/<node-id>`",
  "content_dir=works/TK-12",
  "pulse:workflow use",
  "full close gate",
];

function isDirectExecution(metaUrl, entryPath = process.argv[1]) {
  if (!entryPath) {
    return false;
  }
  try {
    return fs.realpathSync(fileURLToPath(metaUrl)) === fs.realpathSync(entryPath);
  } catch {
    return false;
  }
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function readManifestSkills(manifestPath) {
  const skills = readJson(manifestPath).skills;
  if (!Array.isArray(skills)) {
    throw new Error(`${manifestPath} is missing a skills array`);
  }
  return skills;
}

function assertMatchingSkillManifests(manifests) {
  const [primaryManifest, ...otherManifests] = manifests;
  const primarySkills = JSON.stringify(primaryManifest.skills);

  for (const manifest of otherManifests) {
    if (JSON.stringify(manifest.skills) !== primarySkills) {
      throw new Error(`${manifest.path} skills do not match ${primaryManifest.path}`);
    }
  }
}

function parseSkillName(skillDir) {
  const skillPath = path.join(skillDir, "SKILL.md");
  const skillText = fs.readFileSync(skillPath, "utf8");
  const match = skillText.match(/^---\n[\s\S]*?^name:\s*([^\n]+)$/m);
  if (!match) {
    throw new Error(`${skillPath} is missing frontmatter name`);
  }
  return match[1].trim().replace(/^['"]|['"]$/g, "");
}

export function shouldTreatAsText(filePath) {
  return TEXT_EXTENSIONS.has(path.extname(filePath)) || TEXT_FILENAMES.has(path.basename(filePath));
}

function copyRenderedTree(sourceDir, outputDir, { pulseCommand }) {
  fs.mkdirSync(outputDir, { recursive: true });

  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    if (entry.isDirectory() && EXCLUDED_DIRS.has(entry.name)) {
      continue;
    }

    const sourcePath = path.join(sourceDir, entry.name);
    const outputPath = path.join(outputDir, entry.name);

    if (entry.isDirectory()) {
      copyRenderedTree(sourcePath, outputPath, { pulseCommand });
      continue;
    }

    if (entry.isSymbolicLink()) {
      fs.symlinkSync(fs.readlinkSync(sourcePath), outputPath);
      continue;
    }

    if (!entry.isFile()) {
      continue;
    }

    const mode = fs.statSync(sourcePath).mode;
    if (shouldTreatAsText(sourcePath)) {
      const rendered = renderPulsePlaceholders(fs.readFileSync(sourcePath, "utf8"), { pulseCommand });
      assertNoUnresolvedRuntimePlaceholders(rendered, outputPath);
      fs.writeFileSync(outputPath, rendered, { mode });
    } else {
      fs.copyFileSync(sourcePath, outputPath);
      fs.chmodSync(outputPath, mode);
    }
  }
}

function walkFiles(rootDir) {
  const files = [];
  const pending = [rootDir];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const filePath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(filePath);
      } else if (entry.isFile()) {
        files.push(filePath);
      }
    }
  }
  return files;
}

function assertRustRuntimeDistribution() {
  const workflowDir = path.join(DIST_DIR, ".codex", "skills", WORKFLOW_SKILL_NAME);
  if (fs.existsSync(path.join(workflowDir, "scripts"))) {
    throw new Error("workflow distribution must not contain a legacy scripts runtime");
  }

  for (const filePath of walkFiles(DIST_DIR)) {
    const relativePath = path.relative(DIST_DIR, filePath);
    const content = fs.readFileSync(filePath, "utf8");
    const forbiddenMarkers = [
      "pulse.mjs",
      ...LEGACY_CANONICAL_PATHS,
    ];
    if (relativePath.includes(`${path.sep}skills${path.sep}${WORKFLOW_SKILL_NAME}${path.sep}`)) {
      forbiddenMarkers.push(...LEGACY_GUIDANCE_MARKERS);
    }
    for (const forbidden of forbiddenMarkers) {
      if (relativePath.includes(forbidden) || content.includes(forbidden)) {
        throw new Error(`legacy canonical runtime reference ${forbidden} found in ${relativePath}`);
      }
    }
    if (/\bnode\s+[^\n]*pulse/.test(content)) {
      throw new Error(`Node public runtime invocation found in ${relativePath}`);
    }
  }

  for (const provider of PROVIDERS) {
    const workflowDir = path.join(DIST_DIR, provider.configDir, "skills", WORKFLOW_SKILL_NAME);
    const workflowText = walkFiles(workflowDir)
      .filter(shouldTreatAsText)
      .map((filePath) => fs.readFileSync(filePath, "utf8"))
      .join("\n");
    for (const marker of REQUIRED_WORKFLOW_CONTRACT_MARKERS) {
      if (!workflowText.includes(marker)) {
        throw new Error(`workflow distribution contract marker missing for ${provider.name}: ${marker}`);
      }
    }
    if (workflowText.includes("pulse:workflow onboard")) {
      throw new Error(`unrouted public workflow command remains for ${provider.name}`);
    }
  }
}

function getSkillSourceDirs() {
  const manifests = PLUGIN_MANIFEST_PATHS.map((manifestPath) => ({
    path: manifestPath,
    skills: readManifestSkills(manifestPath),
  }));
  assertMatchingSkillManifests(manifests);
  return manifests[0].skills.map((skillPath) => path.resolve(REPO_ROOT, skillPath));
}

export function buildSkills() {
  const skillSourceDirs = getSkillSourceDirs();
  fs.rmSync(DIST_DIR, { recursive: true, force: true });

  for (const provider of PROVIDERS) {
    const pulseCommand = getPulseCommand(provider);
    for (const skillDir of skillSourceDirs) {
      const skillName = parseSkillName(skillDir);
      const outputDir = path.join(DIST_DIR, provider.configDir, "skills", skillName);
      copyRenderedTree(skillDir, outputDir, { pulseCommand });
    }
  }
  assertRustRuntimeDistribution();
}

if (isDirectExecution(import.meta.url)) {
  buildSkills();
}
