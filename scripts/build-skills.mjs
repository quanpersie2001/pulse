#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { isDirectExecution } from "../skills/workflow/scripts/cli_execution.mjs";
import { getPulseCommand, PROVIDERS } from "./lib/providers.mjs";
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

function shouldTreatAsText(filePath) {
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
}

if (isDirectExecution(import.meta.url)) {
  buildSkills();
}
