#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { buildSkills } from "./build-skills.mjs";
import { getPulseCommand, PROVIDERS, WORKFLOW_SKILL_NAME } from "./lib/providers.mjs";
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

test("providers render workflow skill under its normal skill name", () => {
  assert.equal(WORKFLOW_SKILL_NAME, "workflow");
  for (const provider of PROVIDERS) {
    assert.deepEqual(Object.keys(provider), ["name", "configDir", "skillsRoot"], provider.name);
    assert.equal(getPulseCommand(provider).includes("/skills/workflow/scripts/pulse.mjs"), true, provider.name);
  }
});

test("buildSkills renders provider skill outputs without runtime placeholders", () => {
  assert.equal(fs.existsSync(path.join(REPO_ROOT, "scripts", "sync-skills.sh")), false);

  buildSkills();

  for (const provider of PROVIDERS) {
    const workflowRuntime = path.join(
      DIST_DIR,
      provider.configDir,
      "skills",
      WORKFLOW_SKILL_NAME,
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

test("runtime-facing source docs use the semantic runtime command placeholder", () => {
  const activeDocs = [
    "AGENTS.template.md",
    "AGENTS.md",
    "CLAUDE.md",
    "CONTRIBUTING.md",
    "README.md",
    "docs/ARCHITECTURE.md",
    "docs/examples/golden-path.md",
    "skills/gitnexus/SKILL.md",
    "skills/workflow/SKILL.md",
    "skills/workflow/references/execute/command.md",
    "skills/workflow/references/swarm/command.md",
    "skills/workflow/references/swarm/runtime-adapter-spec.md",
    "skills/workflow/references/use/command.md",
    "skills/workflow/references/use/readiness.md",
  ];

  for (const relativePath of activeDocs) {
    const content = fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
    assert.doesNotMatch(content, /\{\{scripts_path\}\}|\{\{skills_path\}\}|pulse_status\.mjs|pulse_reservations\.mjs/, relativePath);
  }

  const runtimeSourceDocs = [
    "AGENTS.template.md",
    "CLAUDE.md",
    "README.md",
    "docs/ARCHITECTURE.md",
    "docs/examples/golden-path.md",
    "skills/gitnexus/SKILL.md",
    "skills/workflow/references/execute/command.md",
    "skills/workflow/references/swarm/command.md",
  ];

  for (const relativePath of runtimeSourceDocs) {
    const content = fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
    assert.match(content, /\{\{pulse_command\}\}/, relativePath);
  }
});

test("representative rendered docs contain concrete runtime commands", () => {
  buildSkills();

  for (const providerName of ["claude-code", "agents", "codex"]) {
    const provider = PROVIDERS.find((entry) => entry.name === providerName);
    const pulseCommand = getPulseCommand(provider);
    const routerOverview = path.join(DIST_DIR, provider.configDir, "skills", WORKFLOW_SKILL_NAME, "SKILL.md");
    const commandDocs = [
      path.join(DIST_DIR, provider.configDir, "skills", WORKFLOW_SKILL_NAME, "references", "execute", "command.md"),
      path.join(DIST_DIR, provider.configDir, "skills", "gitnexus", "SKILL.md"),
    ];

    const escapedPulseCommand = pulseCommand.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    assert.match(fs.readFileSync(routerOverview, "utf8"), new RegExp(escapedPulseCommand), routerOverview);

    for (const docPath of commandDocs) {
      const content = fs.readFileSync(docPath, "utf8");
      assert.match(content, new RegExp(`${escapedPulseCommand} (status|ready|reservation)`), docPath);
      assert.doesNotMatch(content, /\{\{pulse_command\}\}/, docPath);
    }
  }
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
