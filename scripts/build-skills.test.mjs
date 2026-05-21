#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { buildSkills } from "./build-skills.mjs";
import { getPulseCommand, PROVIDERS } from "./lib/providers.mjs";
import {
  assertNoUnresolvedRuntimePlaceholders,
  findUnresolvedRuntimePlaceholders,
  renderPulsePlaceholders,
} from "./lib/render-pulse-placeholders.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const DIST_DIR = path.join(REPO_ROOT, "dist");
const RUNTIME_PLACEHOLDERS = ["{{pulse_command}}", "{{scripts_path}}", "{{skills_path}}"];

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function collectFiles(root) {
  const files = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

test("Claude and Codex plugin manifests ship the same skills", () => {
  const claudeSkills = readJson(path.join(REPO_ROOT, ".claude-plugin", "plugin.json")).skills;
  const codexSkills = readJson(path.join(REPO_ROOT, ".codex-plugin", "plugin.json")).skills;

  assert.deepEqual(codexSkills, claudeSkills);
});

test("buildSkills renders provider skill outputs without runtime placeholders", () => {
  assert.equal(fs.existsSync(path.join(REPO_ROOT, "scripts", "sync-skills.sh")), false);

  buildSkills();

  for (const provider of PROVIDERS) {
    const workflowRuntime = path.join(
      DIST_DIR,
      provider.configDir,
      "skills",
      provider.workflowSkillDir,
      "scripts",
      "pulse.mjs",
    );
    assert.equal(fs.existsSync(workflowRuntime), true, provider.name);
  }

  for (const filePath of collectFiles(DIST_DIR)) {
    const content = fs.readFileSync(filePath, "utf8");
    for (const placeholder of RUNTIME_PLACEHOLDERS) {
      assert.equal(content.includes(placeholder), false, filePath);
    }
  }
});

test("runtime-facing docs avoid unresolved legacy runtime placeholders", () => {
  const activeDocs = [
    "AGENTS.template.md",
    "CLAUDE.md",
    "CONTRIBUTING.md",
    "README.md",
    "docs/ARCHITECTURE.md",
    "docs/examples/golden-path.md",
    "skills/gitnexus/SKILL.md",
    "skills/workflow/references/execute/command.md",
    "skills/workflow/references/swarm/command.md",
  ];

  for (const relativePath of activeDocs) {
    const content = fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
    assert.doesNotMatch(content, /\{\{scripts_path\}\}|\{\{skills_path\}\}/, relativePath);
  }

  const gitNexusSkill = fs.readFileSync(path.join(REPO_ROOT, "skills", "gitnexus", "SKILL.md"), "utf8");
  assert.match(gitNexusSkill, /pulse-work status --repo-root <repo> --json/);
  assert.doesNotMatch(gitNexusSkill, /pulse_status\.mjs/);
});

test("runtime placeholder renderer replaces only supported source placeholder", () => {
  const claudeProvider = PROVIDERS.find((provider) => provider.name === "claude-code");
  const pulseCommand = getPulseCommand(claudeProvider);

  assert.equal(
    renderPulsePlaceholders("Run {{pulse_command}} status", { pulseCommand }),
    `Run ${pulseCommand} status`,
  );
  assert.deepEqual(findUnresolvedRuntimePlaceholders("{{scripts_path}} {{skills_path}}"), [
    "{{scripts_path}}",
    "{{skills_path}}",
  ]);
  assert.throws(
    () => assertNoUnresolvedRuntimePlaceholders("Run {{scripts_path}}", "example.md"),
    /example\.md contains unresolved runtime placeholder\(s\): \{\{scripts_path\}\}/,
  );
});
