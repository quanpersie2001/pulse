#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { applyRepo } from "../../skills/workflow/scripts/onboard/apply.mjs";
import { assertNoUnresolvedRuntimePlaceholders } from "../../scripts/lib/render-pulse-placeholders.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

function assertNoRuntimeArtifactLeaks(root) {
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const entryPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(entryPath);
        continue;
      }
      const content = fs.readFileSync(entryPath, "utf8");
      const relativePath = path.relative(root, entryPath);
      assertNoUnresolvedRuntimePlaceholders(content, relativePath);
    }
  }
}

test("applyRepo keeps .pulse data-only by not copying canonical runtime scripts", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    const applied = applyRepo(root, false);

    assert.equal(applied.status, "PASS");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "scripts")), false);
    assert.ok(fs.existsSync(path.join(root, ".pulse", "harness", "HARNESS_BACKLOG.md")));
    assert.deepEqual(
      applied.result.managed_assets.support_assets,
      [path.join(".pulse", "harness", "HARNESS_BACKLOG.md")],
    );
  } finally {
    cleanupTempRepo(root);
  }
});

test("applyRepo writes session-aware tooling status and runtime state mirrors", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const toolingStatus = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "tooling-status.json"), "utf8"),
    );
    const state = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "state.json"), "utf8"),
    );

    assert.ok(toolingStatus.session);
    assert.ok(toolingStatus.session.posture);
    assert.ok(Object.prototype.hasOwnProperty.call(toolingStatus.session, "scout_findings"));
    assert.ok(Object.prototype.hasOwnProperty.call(toolingStatus.session, "resume_options"));
    assert.match(toolingStatus.tools.pulse_runtime_helper.command, /^node .+pulse\.mjs"? status --repo-root <repo> --json$/);
    assert.doesNotMatch(toolingStatus.tools.pulse_runtime_helper.command, /\{\{pulse_command\}\}|\{\{scripts_path\}\}|\{\{skills_path\}\}/);
    assert.ok(typeof toolingStatus.next_command === "string");
    assert.ok(toolingStatus.next_command.length > 0);
    assert.ok(toolingStatus.session_load);
    assert.equal(toolingStatus.session_load.posture, "fresh");
    assert.equal(toolingStatus.session_load.next_command, "pulse:workflow explore");

    assert.ok(state.session);
    assert.ok(typeof state.session.posture === "string");
    assert.ok(Array.isArray(state.session.scout_findings));
    assert.ok(Array.isArray(state.session.resume_options));
    assert.ok(typeof state.next_command === "string");
    assert.ok(state.next_command.length > 0);
    assert.equal(Object.prototype.hasOwnProperty.call(state, "next_command_recommended"), false);
    assert.equal(Object.prototype.hasOwnProperty.call(state, "next_skill_recommended"), false);
    assert.equal(Object.prototype.hasOwnProperty.call(state, "next_skill"), false);
    assert.ok(state.session_load);
    assert.equal(state.session_load.posture, "fresh");
    assertNoRuntimeArtifactLeaks(path.join(root, ".pulse"));
  } finally {
    cleanupTempRepo(root);
  }
});

test("applyRepo refreshes STATE.md with session-facing routing summary", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const stateMarkdown = fs.readFileSync(path.join(root, ".pulse", "runtime", "STATE.md"), "utf8");

    assert.match(stateMarkdown, /^# Pulse Runtime State/m);
    assert.match(stateMarkdown, /^Workflow command: use$/m);
    assert.match(stateMarkdown, /^Setup step: onboarding$/m);
    assert.match(stateMarkdown, /^Status: PASS$/m);
    assert.match(stateMarkdown, /^Next command: pulse:workflow /m);
    assert.match(stateMarkdown, /^Session posture: (fresh|resumable|active)$/m);
    assert.match(stateMarkdown, /^Open reservations: \d+$/m);
    assert.match(stateMarkdown, /^Resume options: \d+$/m);
  } finally {
    cleanupTempRepo(root);
  }
});

test("applyRepo backs up non-compliant .pulse in-place and restores safe memory data", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    const correctionsDir = path.join(root, ".pulse", "memory", "corrections");
    fs.mkdirSync(correctionsDir, { recursive: true });
    fs.writeFileSync(path.join(correctionsDir, "lesson.md"), "# Lesson\n", "utf8");
    fs.writeFileSync(path.join(root, ".pulse", "extra-state.json"), `${JSON.stringify({ phase: "archived" })}\n`, "utf8");

    const applied = applyRepo(root, false);
    assert.equal(applied.status, "PASS");

    const pulseEntries = fs.readdirSync(path.join(root, ".pulse"));
    const backupName = pulseEntries.find((entry) => entry.startsWith("backup-"));
    assert.ok(backupName);
    assert.ok(fs.existsSync(path.join(root, ".pulse", backupName, "memory", "corrections", "lesson.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "memory", "corrections", "lesson.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding", "pulse-reconstruction-brief.md")));
    assert.equal(fs.existsSync(path.join(root, ".pulse-backups")), false);
  } finally {
    cleanupTempRepo(root);
  }
});

test("applyRepo backs up non-compliant docs and works in-place with reconstruction briefs", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    fs.mkdirSync(path.join(root, "docs", "adr"), { recursive: true });
    fs.writeFileSync(path.join(root, "docs", "adr", "0001.md"), "# Decision\n", "utf8");
    fs.mkdirSync(path.join(root, "works", "tasks"), { recursive: true });
    fs.writeFileSync(path.join(root, "works", "tasks", "task.md"), "# Task\n", "utf8");

    const applied = applyRepo(root, false);
    assert.equal(applied.status, "PASS");

    const docsBackup = fs.readdirSync(path.join(root, "docs")).find((entry) => entry.startsWith("backup-"));
    const worksBackup = fs.readdirSync(path.join(root, "works")).find((entry) => entry.startsWith("backup-"));
    assert.ok(docsBackup);
    assert.ok(worksBackup);
    assert.ok(fs.existsSync(path.join(root, "docs", docsBackup, "adr", "0001.md")));
    assert.ok(fs.existsSync(path.join(root, "works", worksBackup, "tasks", "task.md")));
    assert.ok(fs.existsSync(path.join(root, "docs", "ARCHITECTURE.md")));
    assert.ok(fs.existsSync(path.join(root, "docs", "GLOSSARY.md")));
    assert.ok(fs.existsSync(path.join(root, "docs", "decisions")));
    assert.ok(fs.existsSync(path.join(root, "docs", "product")));
    assert.ok(fs.existsSync(path.join(root, "works", "epics")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding", "docs-regeneration-brief.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding", "works-reconstruction-brief.md")));
  } finally {
    cleanupTempRepo(root);
  }
});
