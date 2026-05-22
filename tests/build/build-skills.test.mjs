#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { buildSkills, shouldTreatAsText } from "../../scripts/build-skills.mjs";
import { getPulseCommand, PROVIDERS, WORKFLOW_SKILL_NAME } from "../../scripts/lib/providers.mjs";
import {
  assertNoUnresolvedRuntimePlaceholders,
  findUnresolvedRuntimePlaceholders,
  renderPulsePlaceholders,
} from "../../scripts/lib/render-pulse-placeholders.mjs";
import { REPO_ROOT } from "../helpers/fixtures.mjs";

const DIST_DIR = path.join(REPO_ROOT, "dist");
const RUNTIME_PLACEHOLDERS = ["{{pulse_command}}"];

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function collectFiles(root) {
  const files = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(entryPath));
    } else if (entry.isFile() && shouldTreatAsText(entryPath)) {
      files.push(entryPath);
    }
  }
  return files;
}

let skillsBuilt = false;

function ensureSkillsBuilt() {
  if (!skillsBuilt) {
    buildSkills();
    skillsBuilt = true;
  }
}

test("Claude and Codex plugin manifests ship the same skills", () => {
  const claudeSkills = readJson(path.join(REPO_ROOT, ".claude-plugin", "plugin.json")).skills;
  const codexSkills = readJson(path.join(REPO_ROOT, ".codex-plugin", "plugin.json")).skills;

  assert.deepEqual(codexSkills, claudeSkills);
});

test("providers render workflow skill under its normal skill name", () => {
  assert.equal(WORKFLOW_SKILL_NAME, "workflow");
  for (const provider of PROVIDERS) {
    assert.deepEqual(Object.keys(provider), ["name", "displayName", "providerTags", "configDir"], provider.name);
    assert.equal(typeof provider.displayName, "string", provider.name);
    assert.equal(provider.displayName.length > 0, true, provider.name);
    assert.equal(Array.isArray(provider.providerTags), true, provider.name);
    assert.equal(provider.providerTags.length > 0, true, provider.name);
    assert.equal(getPulseCommand(provider).includes(`${provider.configDir}/skills/workflow/scripts/pulse.mjs`), true, provider.name);
  }
});

test("buildSkills renders provider skill outputs without runtime placeholders", () => {
  assert.equal(fs.existsSync(path.join(REPO_ROOT, "scripts", "sync-skills.sh")), false);

  ensureSkillsBuilt();

  for (const provider of PROVIDERS) {
    const workflowScripts = path.join(
      DIST_DIR,
      provider.configDir,
      "skills",
      WORKFLOW_SKILL_NAME,
      "scripts",
    );
    assert.equal(fs.existsSync(path.join(workflowScripts, "pulse.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli_execution.mjs")), false, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "status.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "ready.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "intake.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "reservation.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "session-load.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "onboard.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "cli", "workgraph.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "core", "lock.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "runtime", "read-model.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "workgraph", "model.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "workgraph", "service.mjs")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "command-metadata.json")), true, provider.name);
    assert.equal(fs.existsSync(path.join(workflowScripts, "metadata")), false, provider.name);
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
    assert.doesNotMatch(content, /pulse_status\.mjs|pulse_reservations\.mjs/, relativePath);
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
  ensureSkillsBuilt();

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
  assert.deepEqual(findUnresolvedRuntimePlaceholders("Run {{pulse_command}} status"), ["{{pulse_command}}"]);
  assert.throws(
    () => assertNoUnresolvedRuntimePlaceholders("Run {{pulse_command}}", "example.md"),
    /example\.md contains unresolved runtime placeholder\(s\): \{\{pulse_command\}\}/,
  );
});
