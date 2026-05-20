#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { checkRepo, applyRepo, resolveRepoRoot } from "../scripts/onboard/onboard_pulse.mjs";
import { collectPulseSessionStartNotes } from "../scripts/runtime/pulse_session_context.mjs";

function mkRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-routing-"));
}

test("resolveRepoRoot respects explicitRoot over PULSE_REPO_ROOT and cwd", () => {
  const cwdRoot = mkRoot();
  const envRoot = mkRoot();
  const explicitRoot = mkRoot();
  try {
    const resolved = resolveRepoRoot(explicitRoot, { PULSE_REPO_ROOT: envRoot }, cwdRoot);
    assert.equal(resolved, path.resolve(explicitRoot));
  } finally {
    fs.rmSync(cwdRoot, { recursive: true, force: true });
    fs.rmSync(envRoot, { recursive: true, force: true });
    fs.rmSync(explicitRoot, { recursive: true, force: true });
  }
});

test("resolveRepoRoot uses PULSE_REPO_ROOT when explicitRoot is missing", () => {
  const cwdRoot = mkRoot();
  const envRoot = mkRoot();
  try {
    const resolved = resolveRepoRoot(undefined, { PULSE_REPO_ROOT: envRoot }, cwdRoot);
    assert.equal(resolved, path.resolve(envRoot));
  } finally {
    fs.rmSync(cwdRoot, { recursive: true, force: true });
    fs.rmSync(envRoot, { recursive: true, force: true });
  }
});

test("checkRepo/applyRepo expose FAIL->PASS readiness without legacy br/bv blockers", () => {
  const root = mkRoot();
  try {
    const before = checkRepo(root);
    assert.equal(before.status, "FAIL");

    const beforeSerialized = JSON.stringify(before);
    assert.doesNotMatch(beforeSerialized, /\bbr\b/i);
    assert.doesNotMatch(beforeSerialized, /\bbv\b/i);

    const applied = applyRepo(root, false);
    assert.equal(applied.status, "PASS");
    assert.equal(applied.details.tooling_status_preview.status, "pass");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo keeps .pulse data-only by not copying canonical runtime scripts", () => {
  const root = mkRoot();
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
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("repo-local .pulse/scripts shims are optional compatibility only", () => {
  const root = mkRoot();
  try {
    applyRepo(root, false);

    const shimDir = path.join(root, ".pulse", "scripts");
    fs.mkdirSync(shimDir, { recursive: true });
    fs.writeFileSync(path.join(shimDir, "pulse_status.mjs"), "// stale compatibility shim\n", "utf8");

    const checked = checkRepo(root);
    assert.equal(checked.status, "PASS");
    assert.doesNotMatch(JSON.stringify(checked.actions), /sync_pulse_support_scripts|pulse_status\.mjs/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo writes session-aware tooling status and runtime state mirrors", () => {
  const root = mkRoot();
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
    assert.ok(typeof state.next_command_recommended === "string");
    assert.ok(state.next_command_recommended.length > 0);
    assert.ok(state.session_load);
    assert.equal(state.session_load.posture, "fresh");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo refreshes STATE.md with session-facing routing summary", () => {
  const root = mkRoot();
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
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("session-start context helper aligns with runtime session routing outputs", async () => {
  const root = mkRoot();
  try {
    applyRepo(root, false);

    const notes = await collectPulseSessionStartNotes(root, { syncRuntimeArtifactsIfOnboarded: true });
    const joined = notes.join("\n");

    assert.match(joined, /Pulse is installed for this repo\./);
    assert.match(joined, /Pulse session posture:/);
    assert.match(joined, /Recommended next workflow command: pulse:workflow /);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("session_load auto-loads a single handoff while next_command remains canonical", () => {
  const root = mkRoot();
  try {
    applyRepo(root, false);

    const manifestPath = path.join(root, ".pulse", "runtime", "handoffs", "manifest.json");
    fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
    fs.writeFileSync(
      manifestPath,
      `${JSON.stringify({
        updated_at: new Date().toISOString(),
        active: [
          {
            owner_id: "owner-a",
            owner_type: "workflow_command",
            surface: "pulse:workflow",
            active_command: "explore",
            active_epic_id: "E-0V9K4F",
            active_story_id: "S-0V9K4G",
            active_item_id: null,
            phase: "explore/context",
            summary: "resume later",
            path: ".pulse/runtime/handoffs/owner-a.json",
            next_action: "Please read docs and decide manually",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );

    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "handoffs", "owner-a.json"),
      `${JSON.stringify({
        owner_id: "owner-a",
        active_command: "explore",
        active_epic_id: "E-0V9K4F",
        active_story_id: "S-0V9K4G",
        summary: "resume later",
        next_action: "Please read docs and decide manually",
        read_first: [".pulse/runtime/handoffs/owner-a.json"],
      }, null, 2)}\n`,
      "utf8",
    );

    const reapplied = applyRepo(root, false);
    assert.match(reapplied.next_command, /^pulse:workflow\s+/);
    assert.notEqual(reapplied.next_command, "pulse:workflow Please read docs and decide manually");

    const toolingStatus = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "tooling-status.json"), "utf8"),
    );
    assert.match(toolingStatus.next_command, /^pulse:workflow\s+/);
    assert.notEqual(toolingStatus.next_command, "pulse:workflow Please read docs and decide manually");
    assert.equal(toolingStatus.session_load.posture, "conflicted");
    assert.equal(toolingStatus.session_load.selected_handoff.owner_id, "owner-a");
    assert.ok(toolingStatus.session_load.conflicts.some((entry) => entry.includes("S-0V9K4G")));
    assert.ok(toolingStatus.session_load.read_first.includes(".pulse/runtime/handoffs/owner-a.json"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("session_load requires selection for multiple handoffs and rejects unsafe read_first paths", () => {
  const root = mkRoot();
  try {
    applyRepo(root, false);

    const handoffsDir = path.join(root, ".pulse", "runtime", "handoffs");
    fs.writeFileSync(
      path.join(handoffsDir, "owner-a.json"),
      `${JSON.stringify({ summary: "a", read_first: ["../escape.md"] }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(handoffsDir, "owner-b.json"),
      `${JSON.stringify({ summary: "b" }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(handoffsDir, "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        active: [
          { owner_id: "owner-a", owner_type: "workflow_command", surface: "pulse:workflow", active_command: "plan", path: ".pulse/runtime/handoffs/owner-a.json", summary: "a" },
          { owner_id: "owner-b", owner_type: "workflow_command", surface: "pulse:workflow", active_command: "execute", path: ".pulse/runtime/handoffs/owner-b.json", summary: "b" },
        ],
      }, null, 2)}\n`,
      "utf8",
    );

    const unselected = checkRepo(root).details.tooling_status_preview.session_load;
    assert.equal(unselected.requires_selection, true);
    assert.equal(unselected.read_first.length, 0);

    const selected = checkRepo(root, { resumeOwner: "owner-a" }).details.tooling_status_preview.session_load;
    assert.equal(selected.selected_handoff.owner_id, "owner-a");
    assert.ok(selected.rejected_paths.includes("../escape.md"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("passive legacy archive docs do not trigger active routing drift warnings", () => {
  const root = mkRoot();
  try {
    applyRepo(root, false);

    const archiveDir = path.join(root, "docs", "archive");
    fs.mkdirSync(archiveDir, { recursive: true });
    fs.writeFileSync(
      path.join(archiveDir, "legacy-routing-notes.md"),
      "Historical notes: pulse:preflight and pulse:using-pulse and dream were old routes.\n",
      "utf8",
    );

    const checked = checkRepo(root);
    assert.equal(checked.status, "PASS");
    const serializedWarnings = JSON.stringify(checked.warnings || []);
    assert.doesNotMatch(serializedWarnings, /preflight|using-pulse|dream/i);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo backs up non-compliant .pulse in-place and migrates safe memory data", () => {
  const root = mkRoot();
  try {
    const correctionsDir = path.join(root, ".pulse", "memory", "corrections");
    fs.mkdirSync(correctionsDir, { recursive: true });
    fs.writeFileSync(path.join(correctionsDir, "lesson.md"), "# Lesson\n", "utf8");
    fs.writeFileSync(path.join(root, ".pulse", "state.json"), `${JSON.stringify({ phase: "legacy" })}\n`, "utf8");

    const applied = applyRepo(root, false);
    assert.equal(applied.status, "PASS");

    const pulseEntries = fs.readdirSync(path.join(root, ".pulse"));
    const backupName = pulseEntries.find((entry) => entry.startsWith("backup-"));
    assert.ok(backupName);
    assert.ok(fs.existsSync(path.join(root, ".pulse", backupName, "memory", "corrections", "lesson.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "memory", "corrections", "lesson.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding-migration", "pulse-migration-brief.md")));
    assert.equal(fs.existsSync(path.join(root, ".pulse-backups")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo backs up non-compliant docs and works in-place with migration briefs", () => {
  const root = mkRoot();
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
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding-migration", "docs-regeneration-brief.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding-migration", "works-migration-brief.md")));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
