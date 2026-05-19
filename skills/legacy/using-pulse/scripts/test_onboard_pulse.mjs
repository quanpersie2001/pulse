#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

import { applyRepo, checkRepo, getNodeRuntimeStatus } from "../../../workflow/scripts/runtime/onboard_pulse.mjs";
import { buildPulseDependencyReport } from "../../../workflow/scripts/runtime/pulse_dependencies.mjs";
import { syncPulseRuntimeArtifacts } from "../../../workflow/scripts/runtime/pulse_state.mjs";

const LOCAL_USING_PULSE_SKILL_PATH = fileURLToPath(new URL("../SKILL.md", import.meta.url));
const LOCAL_REPO_ROOT = fileURLToPath(new URL("../../../", import.meta.url));

test("applyRepo creates repo-local Pulse helpers under .pulse/scripts", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    const result = applyRepo(root, false);

    assert.equal(result.result.status, "complete");
    assert.equal(result.status, "up_to_date");
    assert.equal(result.details.runtime.supported, true);
    assert.ok(fs.existsSync(path.join(root, "AGENTS.md")));
    assert.match(fs.readFileSync(path.join(root, "AGENTS.md"), "utf8"), /Pulse Workflow/);
    assert.ok(fs.existsSync(path.join(root, ".codex", "config.toml")));
    assert.equal(fs.existsSync(path.join(root, ".codex", "hooks.json")), false);
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "onboarding.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "state.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime")));
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "current-feature.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "runtime-snapshot.json")), false);
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "schema.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "items.jsonl")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "views", "active.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "views", "closed.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "views", "ready.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "workgraph", "views", "graph.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "checkpoints")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "memory", "learnings")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "memory", "corrections")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "memory", "ratchet")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "harness", "HARNESS_BACKLOG.md")));
    assert.equal(fs.existsSync(path.join(root, ".pulse", "harness", "HARNESS.md")), false);
    assert.ok(fs.existsSync(path.join(root, ".pulse", "runtime", "reservations.json")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse-work")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_work.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "workgraph_store.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "workgraph_templates.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "templates", "works", "epic-README.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "templates", "works", "story-SPEC.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "templates", "works", "verification.md")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_session_context.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_state.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_status.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_dependencies.mjs")));
    assert.ok(fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_reservations.mjs")));
    assert.equal(fs.existsSync(path.join(root, ".codex", "pulse_session_context.mjs")), false);
    assert.equal(fs.existsSync(path.join(root, ".codex", "pulse_state.mjs")), false);
    assert.equal(fs.existsSync(path.join(root, ".codex", "pulse_status.mjs")), false);
    assert.equal(fs.existsSync(path.join(root, ".codex", "pulse_dependencies.mjs")), false);
    assert.equal(fs.existsSync(path.join(root, ".codex", "pulse_reservations.mjs")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkRepo reports onboarding marker path, legacy marker presence, and domain normalization actions", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    fs.mkdirSync(path.join(root, ".pulse"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "onboarding.json"),
      `${JSON.stringify({ plugin_version: "0.9.0", status: "complete" }, null, 2)}\n`,
      "utf8",
    );

    const result = checkRepo(root);

    assert.equal(result.status, "needs_onboarding");
    assert.ok(result.actions.includes("migrate_legacy_onboarding_marker"));
    assert.ok(result.actions.includes("normalize_.pulse_structure"));
    assert.ok(result.actions.includes("normalize_docs_structure"));
    assert.ok(result.actions.includes("normalize_works_structure"));
    assert.equal(result.details.onboarding_marker_path, ".pulse/runtime/onboarding.json");
    assert.equal(result.details.legacy_onboarding_marker_exists, true);
    assert.equal(result.details.domain_status.pulse, "non_compliant");
    assert.equal(result.details.domain_status.docs, "missing");
    assert.equal(result.details.domain_status.works, "missing");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo migrates legacy runtime files into canonical runtime layout", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    fs.mkdirSync(path.join(root, ".pulse", "handoffs"), { recursive: true });
    fs.mkdirSync(path.join(root, ".pulse", "checkpoints", "legacy-feature"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "state.json"),
      `${JSON.stringify({
        active_feature: "legacy-feature",
        active_skill: "pulse:planning",
        phase: "planning",
        gate: "GATE 2",
        gate_status: "approved",
        handoff_manifest: ".pulse/handoffs/manifest.json",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "STATE.md"),
      [
        "Focus: legacy-feature",
        "Phase: planning",
        "Gate: GATE 2",
        "Gate status: approved",
      ].join("\n") + "\n",
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "tooling-status.json"),
      `${JSON.stringify({
        status: "PASS",
        requested_mode: "swarm",
        recommended_mode: "swarm",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "reservations.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:00:00.000Z",
        reservations: [
          {
            id: "legacy-reservation",
            agent: "worker-blue-lake",
            bead_id: "BEAD-014",
            paths: ["skills/swarming/SKILL.md"],
            created_at: "2026-04-16T10:00:00.000Z",
            updated_at: "2026-04-16T10:00:00.000Z",
            ttl_seconds: null,
            expires_at: null,
            status: "active",
            released_at: null,
            note: "legacy reservation",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "handoffs", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:06:00.000Z",
        active: [
          {
            owner_id: "planning",
            owner_type: "phase",
            skill: "pulse:planning",
            feature: "legacy-feature",
            path: ".pulse/handoffs/planning.json",
            phase: "planning/phase-1",
            next_action: "Resume planning",
            summary: "Legacy planning handoff",
            status: "ready_to_resume",
            paused_at: "2026-04-16T10:06:00.000Z",
            reason: "context_critical",
            read_first: [".pulse/STATE.md", "history/legacy-feature/CONTEXT.md"],
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "handoffs", "planning.json"),
      `${JSON.stringify({
        schema_version: "2.0",
        handoff_id: "planning-2026-04-16T10:06:00Z",
        owner_type: "phase",
        owner_id: "planning",
        skill: "pulse:planning",
        feature: "legacy-feature",
        phase: "planning/phase-1",
        status: "ready_to_resume",
        paused_at: "2026-04-16T10:06:00.000Z",
        reason: "context_critical",
        next_action: "Resume planning",
        read_first: [".pulse/STATE.md", "history/legacy-feature/CONTEXT.md"],
        summary: "Legacy planning handoff",
        payload: {},
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "checkpoints", "legacy-feature", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:07:00.000Z",
        checkpoints: [
          {
            checkpoint_id: "legacy-1",
            path: "legacy-1.json",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "checkpoints", "legacy-feature", "legacy-1.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        checkpoint_id: "legacy-1",
        feature: "legacy-feature",
        created_at: "2026-04-16T10:07:00.000Z",
        summary: "Legacy checkpoint",
        next_action: "Resume planning",
        captured: {
          phase: "planning/phase-1",
          gate: "GATE 2",
          mode: "standard_feature",
          story: "",
          bead: "",
        },
        links: {
          handoff: ".pulse/handoffs/planning.json",
          runtime_snapshot: ".pulse/state.json",
        },
        blockers: [],
        memory_hooks: {},
      }, null, 2)}\n`,
      "utf8",
    );

    const checked = checkRepo(root);
    assert.equal(checked.status, "needs_onboarding");
    assert.ok(checked.actions.includes("migrate_legacy_runtime_artifacts"));

    const applied = applyRepo(root, false);
    const migratedState = JSON.parse(fs.readFileSync(path.join(root, ".pulse", "runtime", "state.json"), "utf8"));
    const migratedManifest = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "handoffs", "manifest.json"), "utf8"),
    );
    const migratedHandoff = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "handoffs", "planning.json"), "utf8"),
    );
    const migratedCheckpoint = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "checkpoints", "legacy-feature", "legacy-1.json"), "utf8"),
    );

    assert.equal(applied.status, "up_to_date");
    assert.ok(applied.result.managed_assets.migration_summary.migrated.length > 0);
    assert.equal(migratedState.active_feature, "legacy-feature");
    assert.equal(migratedState.handoff_manifest, ".pulse/runtime/handoffs/manifest.json");
    assert.equal(migratedManifest.active[0].path, ".pulse/runtime/handoffs/planning.json");
    assert.equal(migratedManifest.active[0].read_first[0], ".pulse/runtime/STATE.md");
    assert.equal(migratedHandoff.read_first[0], ".pulse/runtime/STATE.md");
    assert.equal(migratedCheckpoint.links.handoff, ".pulse/runtime/handoffs/planning.json");
    assert.equal(migratedCheckpoint.links.runtime_snapshot, ".pulse/runtime/state.json");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "state.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "STATE.md")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "tooling-status.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "reservations.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "handoffs")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "checkpoints")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("installed pulse-work wrapper manages workgraph state in an onboarded repo", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-workgraph-"));

  try {
    applyRepo(root, false);
    const helperPath = path.join(root, ".pulse", "scripts", "pulse-work");
    const run = (...args) =>
      JSON.parse(execFileSync(helperPath, [...args, "--json"], { cwd: root, encoding: "utf8" }));

    const epic = run("create", "--kind", "EPIC", "--title", "Operator Surface");
    const story = run("create", "--kind", "STORY", "--title", "Plan Runtime Delivery", "--parent", epic.item.id);
    const taskOne = run("create", "--kind", "TASK", "--title", "Implement CLI", "--parent", story.item.id);
    const taskTwo = run("create", "--kind", "TASK", "--title", "Ship wrapper", "--parent", story.item.id);

    const children = run("children", story.item.id);
    const updatedTaskTwo = run(
      "update",
      taskTwo.item.id,
      "--owner",
      "dev-a",
      "--priority",
      "1",
      "--add-label",
      "runtime",
      "--add-risk",
      "data",
    );
    const listedTasks = run("list", "--kind", "TASK", "--owner", "dev-a", "--label", "runtime");
    const dependencyAdded = run("dep", "add", taskTwo.item.id, taskOne.item.id);
    const blockedTask = run("show", taskTwo.item.id);
    const readyBeforeClose = run("ready");

    const verificationPath = path.join(root, ...taskOne.item.verification_path.split("/"));
    fs.writeFileSync(verificationPath, "", "utf8");

    let closeFailure = null;
    try {
      run("close", taskOne.item.id);
    } catch (error) {
      closeFailure = JSON.parse(error.stdout);
    }

    fs.writeFileSync(
      verificationPath,
      [
        "---",
        `id: ${taskOne.item.id}`,
        "status: IN_PROGRESS",
        "---",
        "",
        "# Verification",
        "",
        "## Evidence Summary",
        "Closed after smoke coverage.",
        "",
        "## Commands Run",
        "- pulse-work ready --json",
        "",
        "## Observed Outputs",
        "- task became ready once its dependency cleared",
        "",
        "## Attempts",
        "- None.",
        "",
        "## Artifacts",
        "- None.",
        "",
        "## Unresolved Gaps",
        "None.",
      ].join("\n") + "\n",
      "utf8",
    );

    let closeLeakFailure = null;
    try {
      run("close", taskOne.item.id);
    } catch (error) {
      closeLeakFailure = JSON.parse(error.stdout);
    }

    fs.writeFileSync(
      verificationPath,
      [
        "---",
        `id: ${taskOne.item.id}`,
        "---",
        "",
        "# Verification",
        "",
        "## Evidence Summary",
        "Closed after smoke coverage.",
        "",
        "## Commands Run",
        "- pulse-work ready --json",
        "",
        "## Observed Outputs",
        "- task became ready once its dependency cleared",
        "",
        "## Attempts",
        "- None.",
        "",
        "## Artifacts",
        "- None.",
        "",
        "## Unresolved Gaps",
        "None.",
      ].join("\n") + "\n",
      "utf8",
    );

    const closedTaskOne = run("close", taskOne.item.id);
    const readyAfterClose = run("ready");
    const unblockedTask = run("show", taskTwo.item.id);
    const reopenedTaskOne = run("reopen", taskOne.item.id);
    const dependencyRemoved = run("dep", "rm", taskTwo.item.id, taskOne.item.id);
    const graph = run("graph");
    const doctor = run("doctor");

    assert.deepEqual(children.items.map((item) => item.id).sort(), [taskOne.item.id, taskTwo.item.id].sort());
    assert.equal(updatedTaskTwo.item.owner, "dev-a");
    assert.equal(updatedTaskTwo.item.priority, 1);
    assert.deepEqual(updatedTaskTwo.item.labels, ["runtime"]);
    assert.deepEqual(updatedTaskTwo.item.risk_flags, ["DATA"]);
    assert.deepEqual(listedTasks.items.map((item) => item.id), [taskTwo.item.id]);
    assert.equal(dependencyAdded.dependency_id, taskOne.item.id);
    assert.deepEqual(blockedTask.item.blocked_by_dependencies, [taskOne.item.id]);
    assert.equal(blockedTask.item.ready, false);
    assert.ok(readyBeforeClose.items.some((item) => item.id === taskOne.item.id));
    assert.ok(readyBeforeClose.items.every((item) => item.id !== taskTwo.item.id));
    assert.equal(closeFailure?.ok, false);
    assert.match(closeFailure?.error || "", /verification\.md/i);
    assert.equal(closeLeakFailure?.ok, false);
    assert.match(closeLeakFailure?.error || "", /leaks metadata keys: status/i);
    assert.equal(closedTaskOne.item.status, "CLOSED");
    assert.ok(readyAfterClose.items.some((item) => item.id === taskTwo.item.id));
    assert.deepEqual(unblockedTask.item.blocked_by_dependencies, []);
    assert.equal(unblockedTask.item.ready, true);
    assert.equal(reopenedTaskOne.item.status, "OPEN");
    assert.equal(dependencyRemoved.dependency_id, taskOne.item.id);
    assert.equal(graph.graph.nodes.length, 4);
    assert.equal(graph.graph.edges.hierarchy.length, 3);
    assert.equal(graph.graph.edges.dependencies.length, 0);
    assert.equal(doctor.ok, true);

    const taskOneReadmePath = path.join(root, ...taskOne.item.content_path.split("/"));
    const readmeText = fs.readFileSync(taskOneReadmePath, "utf8");
    fs.writeFileSync(
      taskOneReadmePath,
      readmeText.replace(`id: ${taskOne.item.id}`, "id: TASK-99999"),
      "utf8",
    );
    let doctorWithMismatchedId = null;
    try {
      run("doctor");
    } catch (error) {
      doctorWithMismatchedId = JSON.parse(error.stdout);
    }
    assert.equal(doctorWithMismatchedId?.ok, false);
    assert.match(JSON.stringify(doctorWithMismatchedId?.issues || []), /frontmatter_id_mismatch/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("packaged Codex hook assets are present and point at packaged hook scripts", () => {
  const pluginManifest = JSON.parse(
    fs.readFileSync(path.join(LOCAL_REPO_ROOT, ".codex-plugin", "plugin.json"), "utf8"),
  );
  const hooksConfig = JSON.parse(
    fs.readFileSync(path.join(LOCAL_REPO_ROOT, "hooks", "codex-hooks.json"), "utf8"),
  );

  assert.equal(pluginManifest.hooks, "./hooks/codex-hooks.json");
  assert.ok(Array.isArray(hooksConfig.hooks?.SessionStart));
  assert.equal(hooksConfig.hooks.SessionStart[0]?.matcher, "startup|resume");
  assert.equal(
    hooksConfig.hooks.SessionStart[0]?.hooks?.[0]?.command,
    'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/session-start.mjs"',
  );
  assert.equal(
    hooksConfig.hooks.PreToolUse[0]?.hooks?.[0]?.command,
    'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/pre-tool-use.mjs"',
  );
  assert.equal(
    hooksConfig.hooks.Stop[0]?.hooks?.[0]?.command,
    'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/hooks/stop.mjs"',
  );
});

test("packaged Codex hook commands execute packaged hook scripts", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));
  const hooksConfig = JSON.parse(
    fs.readFileSync(path.join(LOCAL_REPO_ROOT, "hooks", "codex-hooks.json"), "utf8"),
  );

  try {
    applyRepo(root, false);
    fs.cpSync(path.join(LOCAL_REPO_ROOT, "hooks"), path.join(root, "hooks"), { recursive: true });
    fs.mkdirSync(path.join(root, "skills", "using-pulse", "scripts"), { recursive: true });
    fs.copyFileSync(
      path.join(LOCAL_REPO_ROOT, "skills", "using-pulse", "scripts", "pulse_session_context.mjs"),
      path.join(root, "skills", "using-pulse", "scripts", "pulse_session_context.mjs"),
    );

    const sessionStart = JSON.parse(
      execFileSync("sh", ["-lc", hooksConfig.hooks.SessionStart[0].hooks[0].command], {
        cwd: root,
        input: JSON.stringify({ cwd: root }),
        encoding: "utf8",
      }),
    );
    const preToolUse = JSON.parse(
      execFileSync("sh", ["-lc", hooksConfig.hooks.PreToolUse[0].hooks[0].command], {
        cwd: root,
        input: JSON.stringify({ tool_input: { command: "bv" } }),
        encoding: "utf8",
      }),
    );
    const stop = JSON.parse(
      execFileSync("sh", ["-lc", hooksConfig.hooks.Stop[0].hooks[0].command], {
        cwd: root,
        input: JSON.stringify({}),
        encoding: "utf8",
      }),
    );

    assert.equal(sessionStart.hookSpecificOutput?.hookEventName, "SessionStart");
    assert.match(sessionStart.hookSpecificOutput?.additionalContext || "", /Pulse repo notes:/);
    assert.equal(preToolUse.continue, true);
    assert.match(preToolUse.systemMessage || "", /(migration warning|deprecated|pulse-work)/i);
    assert.equal(stop.continue, true);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("packaged Claude hook assets are present and register SessionStart bootstrap", () => {
  const hooksConfig = JSON.parse(
    fs.readFileSync(path.join(LOCAL_REPO_ROOT, "hooks", "hooks.json"), "utf8"),
  );

  assert.ok(fs.existsSync(path.join(LOCAL_REPO_ROOT, "hooks", "session-start.mjs")));
  assert.ok(Array.isArray(hooksConfig.hooks?.SessionStart));
  assert.equal(hooksConfig.hooks.SessionStart[0]?.matcher, "startup|clear|compact");
  assert.equal(
    hooksConfig.hooks.SessionStart[0]?.hooks?.[0]?.command,
    'node "${CLAUDE_PLUGIN_ROOT}/hooks/session-start.mjs"',
  );
  assert.equal(hooksConfig.hooks.SessionStart[0]?.hooks?.[0]?.async, false);
});

test("packaged Claude SessionStart hook prefers repo-local helper context in onboarded repos", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);

    const stdout = execFileSync(
      "node",
      [path.join(LOCAL_REPO_ROOT, "hooks", "session-start.mjs")],
      {
        cwd: root,
        input: JSON.stringify({ cwd: root }),
        encoding: "utf8",
        env: {
          ...process.env,
          CLAUDE_PLUGIN_ROOT: LOCAL_REPO_ROOT,
        },
      },
    );

    const payload = JSON.parse(stdout);
    const additionalContext = payload.hookSpecificOutput?.additionalContext || "";

    assert.equal(payload.hookSpecificOutput?.hookEventName, "SessionStart");
    assert.doesNotMatch(additionalContext, /You have Pulse\./);
    assert.match(additionalContext, /Pulse repo notes:/);
    assert.match(additionalContext, /Pulse onboarding is installed for this repo\./);
    assert.match(additionalContext, /node \.pulse\/scripts\/pulse_status\.mjs --json/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("packaged Claude SessionStart hook falls back to using-pulse bootstrap before onboarding", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    const stdout = execFileSync(
      "node",
      [path.join(LOCAL_REPO_ROOT, "hooks", "session-start.mjs")],
      {
        cwd: root,
        input: JSON.stringify({ cwd: root }),
        encoding: "utf8",
        env: {
          ...process.env,
          CLAUDE_PLUGIN_ROOT: LOCAL_REPO_ROOT,
        },
      },
    );

    const payload = JSON.parse(stdout);
    const additionalContext = payload.hookSpecificOutput?.additionalContext || "";

    assert.equal(payload.hookSpecificOutput?.hookEventName, "SessionStart");
    assert.match(additionalContext, /You have Pulse\./);
    assert.match(additionalContext, /\/pulse onboard/);
    assert.match(additionalContext, /Pulse repo notes:/);
    assert.match(additionalContext, /Onboarding readiness has not been established for this repo\./);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("installed pulse reservations helper reserves, conflicts, and releases paths", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    const helperPath = path.join(root, ".pulse", "scripts", "pulse_reservations.mjs");

    const firstReservation = JSON.parse(
      execFileSync(
        "node",
        [
          helperPath,
          "reserve",
          "--agent",
          "worker-a",
          "--bead",
          "BEAD-101",
          "--path",
          "src/runtime/adapter.md",
          "--json",
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const conflict = JSON.parse(
      execFileSync(
        "node",
        [
          helperPath,
          "reserve",
          "--agent",
          "worker-b",
          "--bead",
          "BEAD-102",
          "--path",
          "src/runtime/adapter.md",
          "--json",
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const listed = JSON.parse(
      execFileSync(
        "node",
        [helperPath, "list", "--active-only", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const released = JSON.parse(
      execFileSync(
        "node",
        [helperPath, "release", "--agent", "worker-a", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );

    assert.equal(firstReservation.ok, true);
    assert.equal(firstReservation.reservation.agent, "worker-a");
    assert.deepEqual(firstReservation.reservation.paths, ["src/runtime/adapter.md"]);
    assert.equal(conflict.ok, false);
    assert.equal(conflict.conflicts.length, 1);
    assert.equal(conflict.conflicts[0].agent, "worker-a");
    assert.equal(listed.reservations.length, 1);
    assert.equal(listed.reservations[0].agent, "worker-a");
    assert.equal(released.released_count, 1);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo appends managed block to existing agents instructions", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    fs.writeFileSync(path.join(root, "AGENTS.md"), "# Existing instructions\n", "utf8");

    applyRepo(root, false);
    const agentsText = fs.readFileSync(path.join(root, "AGENTS.md"), "utf8");

    assert.match(agentsText, /# Existing instructions/);
    assert.match(agentsText, /<!-- PULSE:START -->/);
    assert.equal((agentsText.match(/<!-- PULSE:START -->/g) || []).length, 1);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo preserves an existing compact_prompt without explicit replace", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    const codexDir = path.join(root, ".codex");
    fs.mkdirSync(codexDir, { recursive: true });
    fs.writeFileSync(path.join(codexDir, "config.toml"), 'compact_prompt = """keep me"""\n', "utf8");

    const result = applyRepo(root, false);
    const configText = fs.readFileSync(path.join(codexDir, "config.toml"), "utf8");

    assert.match(configText, /compact_prompt = """keep me"""/);
    assert.equal(result.result.status, "partial");
    assert.match(JSON.stringify(result.result), /compact_prompt/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkRepo flags legacy .codex hook registration and stale python hook files for cleanup", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    const hooksDir = path.join(root, ".codex", "hooks");
    fs.mkdirSync(hooksDir, { recursive: true });
    fs.writeFileSync(
      path.join(root, ".codex", "hooks.json"),
      JSON.stringify(
        {
          hooks: {
            SessionStart: [
              {
                matcher: "startup|resume",
                hooks: [
                  {
                    type: "command",
                    command:
                      'python3 "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/.codex/hooks/pulse_session_start.py"',
                    statusMessage: "Pulse: session bootstrap",
                  },
                ],
              },
            ],
          },
        },
        null,
        2,
      ),
      "utf8",
    );
    fs.writeFileSync(path.join(hooksDir, "pulse_session_start.py"), "# legacy\n", "utf8");

    const result = checkRepo(root);

    assert.equal(result.status, "needs_onboarding");
    assert.ok(result.actions.includes("remove_legacy_pulse_hook_entries"));
    assert.ok(result.actions.includes("remove_legacy_pulse_hook_scripts"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("applyRepo removes legacy .codex hook registration without taking ownership of stale repo-local hook files", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    // These files model a pre-packaged-hooks install. Current runtime contract
    // executes hooks/* directly; onboarding only cleans up the old .codex wiring.
    fs.mkdirSync(path.join(root, ".codex", "hooks"), { recursive: true });
    fs.writeFileSync(path.join(root, ".codex", "hooks", "pulse_session_start.mjs"), "// stale\n", "utf8");
    fs.writeFileSync(path.join(root, ".codex", "hooks", "pulse_session_start.py"), "# legacy\n", "utf8");
    fs.writeFileSync(
      path.join(root, ".codex", "hooks.json"),
      `${JSON.stringify(
        {
          hooks: {
            SessionStart: [
              {
                matcher: "startup|resume",
                hooks: [
                  {
                    type: "command",
                    command: 'node "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/.codex/hooks/pulse_session_start.mjs"',
                    statusMessage: "Pulse: session bootstrap",
                  },
                ],
              },
            ],
            PreToolUse: [
              {
                matcher: "Write",
                hooks: [
                  {
                    type: "command",
                    command: "node ./custom-hook.mjs",
                    statusMessage: "Custom hook",
                  },
                ],
              },
            ],
          },
        },
        null,
        2,
      )}\n`,
      "utf8",
    );

    const result = applyRepo(root, false);
    const hooksConfig = JSON.parse(fs.readFileSync(path.join(root, ".codex", "hooks.json"), "utf8"));
    const runtimeHook = fs.readFileSync(
      path.join(root, ".codex", "hooks", "pulse_session_start.mjs"),
      "utf8",
    );

    assert.equal(result.status, "up_to_date");
    assert.deepEqual(result.result.managed_assets.legacy_hook_cleanup, ["remove_legacy_pulse_hooks_SessionStart"]);
    assert.deepEqual(result.result.managed_assets.legacy_hook_scripts_removed, [".codex/hooks/pulse_session_start.py"]);
    assert.equal(hooksConfig.hooks.SessionStart, undefined);
    assert.equal(hooksConfig.hooks.PreToolUse[0].hooks[0].statusMessage, "Custom hook");
    assert.equal(fs.existsSync(path.join(root, ".codex", "hooks", "pulse_session_start.py")), false);
    assert.equal(runtimeHook, "// stale\n");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("pulse status scout renders json for an onboarded repo", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    fs.rmSync(path.join(root, ".pulse", "runtime", "current-feature.json"), { force: true });
    fs.rmSync(path.join(root, ".pulse", "runtime", "runtime-snapshot.json"), { force: true });

    const stdout = execFileSync("node", [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "--json"], {
      cwd: root,
      encoding: "utf8",
    });

    const payload = JSON.parse(stdout);
    const normalizedRoot = fs.realpathSync.native(root);
    const normalizedPayloadRoot = fs.realpathSync.native(payload.repo_root);
    assert.equal(normalizedPayloadRoot, normalizedRoot);
    assert.equal(payload.state_json.exists, true);
    assert.equal(payload.current_feature.exists, true);
    assert.equal(payload.current_feature.feature_key, "");
    assert.equal(payload.current_feature.phase, "idle");
    assert.equal(payload.current_feature.status, "idle");
    assert.equal(payload.runtime_snapshot.exists, true);
    assert.equal(payload.runtime_snapshot.active_feature, "");
    assert.equal(payload.runtime_snapshot.phase, "idle");
    assert.equal(payload.runtime_snapshot.active_skill, "pulse");
    assert.equal(payload.runtime_snapshot.source.state_json, ".pulse/runtime/state.json");
    assert.equal(payload.runtime_snapshot.source.state_markdown, ".pulse/runtime/STATE.md");
    assert.equal(payload.runtime_snapshot.source.current_feature, "");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "current-feature.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "runtime-snapshot.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "checkpoints")), true);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "memory", "learnings")), true);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "memory", "corrections")), true);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "memory", "ratchet")), true);
    assert.equal(payload.checkpoints.root_exists, true);
    assert.equal(payload.checkpoints.count, 0);
    assert.equal(payload.memory_recall.root_exists, true);
    assert.equal(payload.memory_recall.critical_patterns, "");
    assert.equal(payload.handoff_manifest.active_count, 0);
    assert.equal(payload.reservations.exists, true);
    assert.equal(payload.reservations.total, 0);
    assert.equal(payload.reservations.active_count, 0);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("syncPulseRuntimeArtifacts computes runtime state without persisting deprecated top-level mirrors", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "sync-feature",
        active_skill: "pulse:planning",
        phase: "planning",
        requested_mode: "swarm",
        recommended_mode: "single-worker",
        next_action: "manual_invoke",
        next_skill_recommended: "pulse:executing",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      "Focus: markdown-feature\nPhase: execution\nGate: GATE 3\nGate status: approved\nNext action: manual_invoke\nNext skill recommended: pulse:executing\n",
      "utf8",
    );

    const synced = syncPulseRuntimeArtifacts(root);

    assert.equal(synced.current_feature.feature_key, "sync-feature");
    assert.equal(synced.current_feature.phase, "planning");
    assert.equal(synced.current_feature.gate, "GATE 3");
    assert.equal(synced.current_feature.gate_status, "approved");
    assert.equal(synced.current_feature.next_action, "manual_invoke");
    assert.equal(synced.runtime_snapshot.active_feature, "sync-feature");
    assert.equal(synced.runtime_snapshot.active_skill, "pulse:planning");
    assert.equal(synced.runtime_snapshot.phase, "planning");
    assert.equal(synced.runtime_snapshot.requested_mode, "swarm");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "current-feature.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "runtime-snapshot.json")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("syncPulseRuntimeArtifacts treats '(none)' feature placeholders as empty pointers", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      "Focus: (none)\nPhase: preflight\n",
      "utf8",
    );

    const synced = syncPulseRuntimeArtifacts(root);

    assert.equal(synced.current_feature.feature_key, "");
    assert.equal(synced.current_feature.status, "idle");
    assert.equal(synced.runtime_snapshot.active_feature, "");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "current-feature.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "runtime-snapshot.json")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("pulse status scout recommends manual invocation after an approved gate by default", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "manual-gate-feature",
        active_skill: "pulse:validating",
        phase: "validating",
        gate: "GATE 3",
        gate_status: "approved",
        requested_mode: "execution-only",
        recommended_mode: "single-worker",
        next_action: "manual_invoke",
        next_skill_recommended: "pulse:executing",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      [
        "Focus: manual-gate-feature",
        "Phase: go-mode/gate-3",
        "Gate: GATE 3",
        "Gate status: approved",
        "Next action: manual_invoke",
        "Next skill recommended: pulse:executing",
      ].join("\n") + "\n",
      "utf8",
    );

    const jsonStdout = execFileSync(
      "node",
      [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "--json", "--sync"],
      { cwd: root, encoding: "utf8" },
    );
    const textStdout = execFileSync(
      "node",
      [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "--sync"],
      { cwd: root, encoding: "utf8" },
    );

    const payload = JSON.parse(jsonStdout);
    assert.equal(payload.state_json.next_action, "manual_invoke");
    assert.equal(payload.state_json.next_skill_recommended, "pulse:executing");
    assert.equal(payload.current_feature.gate, "GATE 3");
    assert.equal(payload.current_feature.gate_status, "approved");
    assert.equal(payload.current_feature.next_action, "manual_invoke");
    assert.equal(payload.current_feature.next_skill_recommended, "pulse:executing");
    assert.equal(payload.runtime_snapshot.gate, "GATE 3");
    assert.equal(payload.runtime_snapshot.gate_status, "approved");
    assert.equal(payload.runtime_snapshot.next_action, "manual_invoke");
    assert.equal(payload.runtime_snapshot.next_skill_recommended, "pulse:executing");
    assert.equal(payload.recommended_actions[0], "Gate cleared. Manually invoke pulse:executing when ready.");
    assert.match(textStdout, /gate_status: approved/);
    assert.match(textStdout, /next_action: manual_invoke/);
    assert.match(textStdout, /next_skill_recommended: pulse:executing/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("pulse status scout surfaces current-feature, runtime snapshot, canonical handoff summaries, and targeted recall guidance", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);

    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "operator-surface-foundation",
        active_skill: "pulse:planning",
        phase: "planning",
        gate: "GATE 2",
        gate_status: "approved",
        requested_mode: "swarm",
        recommended_mode: "swarm",
        next_action: "manual_invoke",
        next_skill_recommended: "pulse:validating",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      [
        "Focus: operator-surface-foundation",
        "Phase: planning",
        "Gate: GATE 2",
        "Gate status: approved",
        "Next action: manual_invoke",
        "Next skill recommended: pulse:validating",
      ].join("\n") + "\n",
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "reservations.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:05:30.000Z",
        reservations: [
          {
            id: "resv-1",
            agent: "worker-blue-lake",
            bead_id: "BEAD-014",
            paths: ["skills/swarming/SKILL.md"],
            created_at: "2026-04-16T10:05:00.000Z",
            updated_at: "2026-04-16T10:05:30.000Z",
            ttl_seconds: null,
            expires_at: null,
            status: "active",
            released_at: null,
            note: "editing swarm contract"
          }
        ]
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "critical-patterns.md"),
      `${"# Critical patterns\n"}${"A".repeat(25000)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "learnings", "20260401-operator-surface-foundation.md"),
      [
        "---",
        "date: 2026-04-01",
        "feature: operator-surface-foundation",
        "categories: [pattern]",
        "severity: standard",
        "tags: [operator, surface, planning]",
        "applies_when: planning operator surfaces for the current feature",
        "scope: [skills/using-pulse/scripts/pulse_state.mjs]",
        "signals: [planning, operator surface]",
        "---",
        "",
        "# Learning: Operator Surface Foundation",
        "",
        "**Category:** pattern",
        "**Severity:** standard",
        "**Tags:** [operator, surface]",
        "**Applicable-when:** planning operator surfaces for the current feature",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "learnings", "20260301-operator-surface-foundation.md"),
      [
        "---",
        "date: 2026-03-01",
        "feature: operator-surface-foundation",
        "categories: [pattern]",
        "severity: standard",
        "tags: [operator, foundation]",
        "applies_when: reopening operator foundation work",
        "scope: [history/operator-surface-foundation/CONTEXT.md]",
        "signals: [foundation, feature]",
        "---",
        "",
        "# Learning: Older Operator Surface Foundation",
        "",
        "**Category:** pattern",
        "**Severity:** standard",
        "**Tags:** [operator, foundation]",
        "**Applicable-when:** reopening operator foundation work",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "corrections", "20260402-planning-gate.md"),
      [
        "---",
        "date: 2026-04-02",
        "feature: operator-surface-foundation",
        "severity: critical",
        "tags: [planning, gate]",
        "applies_when: planning gate reviews are about to start",
        "scope: [skills/planning/SKILL.md]",
        "signals: [planning, gate]",
        "---",
        "",
        "# Correction: Planning Gate",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "corrections", "20260302-planning-gate.md"),
      [
        "---",
        "date: 2026-03-02",
        "feature: operator-surface-foundation",
        "severity: standard",
        "tags: [planning, gate]",
        "applies_when: planning gate reviews are about to start",
        "scope: [skills/planning/SKILL.md]",
        "signals: [planning, gate]",
        "---",
        "",
        "# Correction: Older Planning Gate",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "ratchet", "20260403-planning-ratchet.md"),
      [
        "---",
        "date: 2026-04-03",
        "feature: operator-surface-foundation",
        "severity: critical",
        "tags: [planning, ratchet]",
        "applies_when: validating planning changes before execution",
        "scope: [skills/validating/SKILL.md]",
        "signals: [planning, ratchet]",
        "---",
        "",
        "# Ratchet: Planning Ratchet",
      ].join("\n"),
      "utf8",
    );
    fs.mkdirSync(path.join(root, ".pulse", "runtime", "checkpoints", "operator-surface-foundation"), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "checkpoints", "operator-surface-foundation", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:07:00.000Z",
        checkpoints: [
          {
            checkpoint_id: "2026-04-16T10-07-00Z-planning",
            path: "2026-04-16T10-07-00Z-planning.json",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "checkpoints", "operator-surface-foundation", "2026-04-16T10-07-00Z-planning.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        checkpoint_id: "2026-04-16T10-07-00Z-planning",
        feature: "operator-surface-foundation",
        created_at: "2026-04-16T10:07:00.000Z",
        summary: "Planning is complete and validating is next",
        next_action: "Run pulse:validating for the current phase",
        captured: {
          phase: "planning/phase-4",
          gate: "GATE 2",
          mode: "standard_feature",
          story: "Story 2",
          bead: "BEAD-014",
        },
        links: {
          context: "history/operator-surface-foundation/CONTEXT.md",
          handoff: ".pulse/runtime/handoffs/planning.json",
          runtime_snapshot: ".pulse/runtime/state.json",
          verification: ".pulse/runs/operator-surface-foundation/verification/",
        },
        blockers: ["Awaiting validation approval"],
        memory_hooks: {
          critical_patterns: ".pulse/memory/critical-patterns.md",
          learnings: [".pulse/memory/learnings/operator-surface-foundation.md"],
          corrections: [".pulse/memory/corrections/planning-gate.md"],
          ratchet: [".pulse/memory/ratchet/planning-ratchet.md"],
        },
      }, null, 2)}\n`,
      "utf8",
    );
    fs.mkdirSync(path.join(root, ".pulse", "runtime", "handoffs"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "handoffs", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T10:06:00.000Z",
        active: [
          {
            owner_id: "planning",
            owner_type: "phase",
            skill: "pulse:planning",
            feature: "operator-surface-foundation",
            path: ".pulse/runtime/handoffs/planning.json",
            phase: "planning/phase-4",
            next_action: "Create remaining task beads",
            summary: "Discovery and approach are complete",
            status: "ready_to_resume",
            paused_at: "2026-04-16T10:06:00.000Z",
            reason: "context_critical",
            read_first: [".pulse/runtime/STATE.md", "history/operator-surface-foundation/CONTEXT.md"],
          },
          {
            owner_id: "worker-blue-lake",
            owner_type: "worker",
            skill: "pulse:executing",
            feature: "operator-surface-foundation",
            path: ".pulse/runtime/handoffs/worker-blue-lake.json",
            phase: "execution/phase-4",
            next_action: "Resume bead implementation",
            summary: "Verification is pending after the code change",
            status: "ready_to_resume",
            paused_at: "2026-04-16T10:06:30.000Z",
            reason: "context_critical",
            read_first: ["AGENTS.md", ".pulse/runtime/STATE.md", "history/operator-surface-foundation/CONTEXT.md"],
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.mkdirSync(path.join(root, "history", "operator-surface-foundation"), { recursive: true });
    fs.writeFileSync(
      path.join(root, "history", "operator-surface-foundation", "lifecycle-summary.md"),
      [
        "# Lifecycle Summary",
        "",
        "## Approved artifacts",
        "- Context: history/operator-surface-foundation/CONTEXT.md",
        "- Approach: history/operator-surface-foundation/approach.md",
        "- Phase plan: history/operator-surface-foundation/phase-plan.md",
        "",
        "## Gate outcomes",
        "- GATE 1: approved",
        "- GATE 2: approved",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(path.join(root, "history", "operator-surface-foundation", "CONTEXT.md"), "# Context\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "operator-surface-foundation", "approach.md"), "# Approach\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "operator-surface-foundation", "phase-plan.md"), "# Phase Plan\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "operator-surface-foundation", "phase-4-contract.md"), "# Contract\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "operator-surface-foundation", "phase-4-story-map.md"), "# Story Map\n", "utf8");
    fs.mkdirSync(path.join(root, "history", "operator-surface-foundation", "verification"), { recursive: true });
    fs.mkdirSync(path.join(root, "docs", "adr"), { recursive: true });
    fs.writeFileSync(path.join(root, "CONTEXT.md"), "# Root Context\n", "utf8");
    fs.writeFileSync(path.join(root, "docs", "adr", "0001-boundaries.md"), "# Boundaries\n", "utf8");
    fs.writeFileSync(
      path.join(root, ".pulse", "project-docs.json"),
      `${JSON.stringify({
        status: "mapped",
        mode: "single-context",
        context: {
          root: "CONTEXT.md",
          map: "",
          entries: [],
        },
        adrs: {
          enabled: true,
          dir: "docs/adr",
        },
        notes: ["Glossary is stable at repo root."],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, "history", "operator-surface-foundation", "verification", "final-review.md"),
      "# Final Review\n",
      "utf8",
    );

    const jsonStdout = execFileSync(
      "node",
      [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "--json"],
      {
        cwd: root,
        encoding: "utf8",
      },
    );
    const textStdout = execFileSync("node", [path.join(root, ".pulse", "scripts", "pulse_status.mjs")], {
      cwd: root,
      encoding: "utf8",
    });

    const payload = JSON.parse(jsonStdout);
    assert.equal(payload.feature, undefined);
    assert.equal(payload.current_feature.exists, true);
    assert.equal(payload.current_feature.feature_key, "operator-surface-foundation");
    assert.equal(payload.current_feature.phase, "planning");
    assert.equal(payload.current_feature.gate, "GATE 2");
    assert.equal(payload.runtime_snapshot.exists, true);
    assert.equal(payload.runtime_snapshot.active_feature, "operator-surface-foundation");
    assert.equal(payload.runtime_snapshot.source.current_feature, "");
    assert.equal(payload.state_json.active_feature, "operator-surface-foundation");
    assert.equal(payload.handoff_manifest.active_count, 2);
    assert.equal(payload.handoff_manifest.active.length, 2);
    assert.equal(payload.handoff_manifest.active[0].owner_id, "planning");
    assert.equal(payload.reservations.exists, true);
    assert.equal(payload.reservations.total, 1);
    assert.equal(payload.reservations.active_count, 1);
    assert.equal(payload.project_docs.exists, true);
    assert.equal(payload.project_docs.status, "mapped");
    assert.equal(payload.project_docs.mode, "single-context");
    assert.equal(payload.project_docs.mapping_path, ".pulse/project-docs.json");
    assert.equal(payload.project_docs.context.root, "CONTEXT.md");
    assert.equal(payload.project_docs.adrs.dir, "docs/adr");
    assert.deepEqual(payload.reservations.active_agents, ["worker-blue-lake"]);
    assert.equal(
      payload.handoff_manifest.active[0].operator_summary,
      "planning | via pulse:planning | feature=operator-surface-foundation | phase=planning/phase-4 | next=Create remaining task beads | summary=Discovery and approach are complete | path=.pulse/runtime/handoffs/planning.json",
    );
    assert.equal(
      payload.handoff_manifest.active[1].operator_summary,
      "worker-blue-lake | via pulse:executing | feature=operator-surface-foundation | phase=execution/phase-4 | next=Resume bead implementation | summary=Verification is pending after the code change | path=.pulse/runtime/handoffs/worker-blue-lake.json",
    );
    assert.equal(payload.checkpoints.root_exists, true);
    assert.equal(payload.checkpoints.feature, "operator-surface-foundation");
    assert.equal(payload.checkpoints.count, 1);
    assert.equal(payload.checkpoints.latest.checkpoint_id, "2026-04-16T10-07-00Z-planning");
    assert.equal(
      payload.checkpoints.latest.operator_summary,
      "2026-04-16T10-07-00Z-planning | phase=planning/phase-4 | gate=GATE 2 | next=Run pulse:validating for the current phase | summary=Planning is complete and validating is next | path=.pulse/runtime/checkpoints/operator-surface-foundation/2026-04-16T10-07-00Z-planning.json",
    );
    assert.equal(payload.memory_recall.critical_patterns, ".pulse/memory/critical-patterns.md");
    assert.deepEqual(payload.memory_recall.learnings, [
      ".pulse/memory/learnings/20260401-operator-surface-foundation.md",
      ".pulse/memory/learnings/20260301-operator-surface-foundation.md",
    ]);
    assert.deepEqual(payload.memory_recall.corrections, [
      ".pulse/memory/corrections/20260402-planning-gate.md",
      ".pulse/memory/corrections/20260302-planning-gate.md",
    ]);
    assert.deepEqual(payload.memory_recall.ratchet, [
      ".pulse/memory/ratchet/20260403-planning-ratchet.md",
    ]);
    assert.deepEqual(payload.memory_recall.recall_pack, [
      {
        kind: "critical-patterns",
        path: ".pulse/memory/critical-patterns.md",
        reason: "global planning baseline",
      },
      {
        kind: "correction",
        path: ".pulse/memory/corrections/20260402-planning-gate.md",
        reason: "matched feature:operator, feature:surface, feature:foundation, phase-tag:planning, phase:planning, scope:planning, severity:critical",
      },
      {
        kind: "correction",
        path: ".pulse/memory/corrections/20260302-planning-gate.md",
        reason: "matched feature:operator, feature:surface, feature:foundation, phase-tag:planning, phase:planning, scope:planning",
      },
      {
        kind: "ratchet",
        path: ".pulse/memory/ratchet/20260403-planning-ratchet.md",
        reason: "matched feature:operator, feature:surface, feature:foundation, phase-tag:planning, phase:planning, severity:critical",
      },
      {
        kind: "learning",
        path: ".pulse/memory/learnings/20260401-operator-surface-foundation.md",
        reason: "matched feature:operator, feature:surface, feature:foundation, phase-tag:planning, phase:planning",
      },
      {
        kind: "learning",
        path: ".pulse/memory/learnings/20260301-operator-surface-foundation.md",
        reason: "matched feature:operator, feature:surface, feature:foundation",
      },
    ]);
    assert.ok(payload.memory_recall.hygiene.warnings.includes(
      "critical-patterns.md is getting large; review for compact, globally useful guidance only.",
    ));
    assert.ok(payload.memory_recall.hygiene.warnings.includes(
      "Possible duplicate learnings: operator-surface-foundation.",
    ));
    assert.ok(payload.memory_recall.hygiene.warnings.includes(
      "Possible duplicate corrections: planning-gate.",
    ));
    assert.ok(payload.next_reads.includes(".pulse/runtime/handoffs/manifest.json"));
    assert.ok(payload.next_reads.includes(".pulse/project-docs.json"));
    assert.ok(payload.next_reads.includes("CONTEXT.md"));
    assert.ok(payload.next_reads.includes("docs/adr"));
    assert.ok(payload.next_reads.includes(".pulse/runtime/handoffs/planning.json"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/CONTEXT.md"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/approach.md"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/phase-plan.md"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/phase-4-contract.md"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/phase-4-story-map.md"));
    assert.ok(payload.next_reads.includes("history/operator-surface-foundation/lifecycle-summary.md"));
    assert.ok(
      payload.next_reads.includes(
        ".pulse/runtime/checkpoints/operator-surface-foundation/2026-04-16T10-07-00Z-planning.json",
      ),
    );
    assert.ok(payload.next_reads.includes(".pulse/memory/critical-patterns.md"));
    assert.ok(payload.next_reads.includes(".pulse/memory/corrections/20260302-planning-gate.md"));
    assert.deepEqual(payload.history_lifecycle, {
      feature: "operator-surface-foundation",
      exists: true,
      lifecycle_summary: "history/operator-surface-foundation/lifecycle-summary.md",
      approved_artifacts: [
        "history/operator-surface-foundation/CONTEXT.md",
        "history/operator-surface-foundation/approach.md",
        "history/operator-surface-foundation/phase-plan.md",
      ],
      verification: ["history/operator-surface-foundation/verification/final-review.md"],
      memory_promotions: [],
      lifecycle_signals: [
        "history/operator-surface-foundation/phase-4-contract.md",
        "history/operator-surface-foundation/phase-4-story-map.md",
      ],
      next_reads: [
        "history/operator-surface-foundation/lifecycle-summary.md",
        "history/operator-surface-foundation/CONTEXT.md",
        "history/operator-surface-foundation/approach.md",
        "history/operator-surface-foundation/phase-plan.md",
        "history/operator-surface-foundation/phase-4-contract.md",
        "history/operator-surface-foundation/phase-4-story-map.md",
        "history/operator-surface-foundation/verification/final-review.md",
      ],
      self_sufficient: true,
    });
    assert.deepEqual(payload.memory_recall.schema_summary, {
      selected_entries: 5,
      strong_schema_entries: 5,
      metadata_first_ranking: true,
      fallback_to_filename_tokens: false,
    });
    assert.ok(payload.recommended_actions.some((item) => item.includes("mapped project docs")));
    assert.ok(payload.recommended_actions.some((item) => item.includes("targeted recall pack")));
    assert.ok(payload.recommended_actions.some((item) => item.includes("durable audit pass without reopening live runtime state")));
    assert.ok(payload.recommended_actions.some((item) => item.includes("Memory hygiene warning")));
    assert.match(textStdout, /Feature: operator-surface-foundation/);
    assert.match(textStdout, /Operator surface:/);
    assert.match(textStdout, /Project docs:/);
    assert.match(textStdout, /Status: mapped/);
    assert.match(textStdout, /Current feature snapshot: present/);
    assert.match(textStdout, /Runtime snapshot: present/);
    assert.match(textStdout, /active_feature: operator-surface-foundation/);
    assert.match(textStdout, /Active reservations: 1/);
    assert.match(textStdout, /active_agents: worker-blue-lake/);
    assert.match(textStdout, /Checkpoint root: present/);
    assert.match(textStdout, /checkpoint_count: 1/);
    assert.match(textStdout, /History lifecycle: present/);
    assert.match(textStdout, /self_sufficient: yes/);
    assert.match(textStdout, /approved_artifacts: history\/operator-surface-foundation\/CONTEXT.md, history\/operator-surface-foundation\/approach.md, history\/operator-surface-foundation\/phase-plan.md/);
    assert.match(textStdout, /Memory recall root: present/);
    assert.match(textStdout, /critical_patterns: \.pulse\/memory\/critical-patterns\.md/);
    assert.match(textStdout, /recall_pack:/);
    assert.match(textStdout, /critical-patterns: \.pulse\/memory\/critical-patterns\.md \(global planning baseline\)/);
    assert.match(textStdout, /schema_summary: 5\/5 strong-schema entries selected; metadata_first=yes; filename_fallback=no/);
    assert.match(textStdout, /hygiene_warnings:/);
    assert.match(textStdout, /Possible duplicate learnings: operator-surface-foundation\./);
    assert.match(textStdout, /Active handoffs: 2/);
    assert.match(
      textStdout,
      /planning \| via pulse:planning \| feature=operator-surface-foundation \| phase=planning\/phase-4 \| next=Create remaining task beads \| summary=Discovery and approach are complete \| path=.pulse\/runtime\/handoffs\/planning.json/,
    );
    assert.match(
      textStdout,
      /2026-04-16T10-07-00Z-planning \| phase=planning\/phase-4 \| gate=GATE 2 \| next=Run pulse:validating for the current phase \| summary=Planning is complete and validating is next \| path=.pulse\/runtime\/checkpoints\/operator-surface-foundation\/2026-04-16T10-07-00Z-planning.json/,
    );
    assert.match(
      textStdout,
      /worker-blue-lake \| via pulse:executing \| feature=operator-surface-foundation \| phase=execution\/phase-4 \| next=Resume bead implementation \| summary=Verification is pending after the code change \| path=.pulse\/runtime\/handoffs\/worker-blue-lake.json/,
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkpoint commands save, list, show, diff, and resume-brief through installed pulse_status", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    fs.mkdirSync(path.join(root, "history", "checkpoint-ops"), { recursive: true });
    fs.writeFileSync(
      path.join(root, "history", "checkpoint-ops", "CONTEXT.md"),
      "# Context\n",
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, "history", "checkpoint-ops", "lifecycle-summary.md"),
      [
        "# Lifecycle Summary",
        "",
        "## Approved artifacts",
        "- Context: history/checkpoint-ops/CONTEXT.md",
        "- Approach: history/checkpoint-ops/approach.md",
        "- Phase plan: history/checkpoint-ops/phase-plan.md",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(path.join(root, "history", "checkpoint-ops", "approach.md"), "# Approach\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "checkpoint-ops", "phase-plan.md"), "# Phase Plan\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "checkpoint-ops", "phase-5-contract.md"), "# Contract\n", "utf8");
    fs.mkdirSync(path.join(root, "history", "checkpoint-ops", "verification"), { recursive: true });
    fs.writeFileSync(path.join(root, "history", "checkpoint-ops", "verification", "final-review.md"), "# Final Review\n", "utf8");
    fs.mkdirSync(path.join(root, ".pulse", "runs", "checkpoint-ops", "verification"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "checkpoint-ops",
        active_skill: "pulse:validating",
        phase: "validating",
        gate: "GATE 3",
        gate_status: "approved",
        requested_mode: "swarm",
        recommended_mode: "swarm",
        next_action: "manual_invoke",
        next_skill_recommended: "pulse:executing",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      [
        "Focus: checkpoint-ops",
        "Phase: validating",
        "Gate: GATE 3",
        "Gate status: approved",
        "Next action: manual_invoke",
        "Next skill recommended: pulse:executing",
      ].join("\n") + "\n",
      "utf8",
    );
    fs.mkdirSync(path.join(root, ".pulse", "runtime", "handoffs"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "handoffs", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T11:01:00.000Z",
        active: [
          {
            owner_id: "planning",
            owner_type: "phase",
            skill: "pulse:planning",
            feature: "checkpoint-ops",
            path: ".pulse/runtime/handoffs/planning.json",
            phase: "planning/phase-5",
            next_action: "Review the current phase contract",
            summary: "Planning is complete and validation is queued",
            status: "ready_to_resume",
            paused_at: "2026-04-16T11:01:00.000Z",
            reason: "context_critical",
            read_first: [".pulse/runtime/STATE.md", "history/checkpoint-ops/CONTEXT.md"],
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "critical-patterns.md"),
      "# Critical patterns\n",
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "memory", "learnings", "checkpoint-ops.md"),
      "learning\n",
      "utf8",
    );

    const saveOne = JSON.parse(
      execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "save",
          "--json",
          "--summary",
          "Validation approved and execution is next",
          "--next-action",
          "Open pulse:executing",
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const saveTwo = JSON.parse(
      execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "save",
          "--json",
          "--summary",
          "Execution finished and review is next",
          "--next-action",
          "Open pulse:reviewing",
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );

    const listPayload = JSON.parse(
      execFileSync(
        "node",
        [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "checkpoint", "list", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const showPayload = JSON.parse(
      execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "show",
          "--json",
          "--checkpoint-id",
          saveOne.checkpoint.checkpoint_id,
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const diffPayload = JSON.parse(
      execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "diff",
          "--json",
          "--from",
          saveOne.checkpoint.checkpoint_id,
          "--to",
          saveTwo.checkpoint.checkpoint_id,
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const resumePayload = JSON.parse(
      execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "resume-brief",
          "--json",
          "--checkpoint-id",
          saveTwo.checkpoint.checkpoint_id,
        ],
        { cwd: root, encoding: "utf8" },
      ),
    );

    assert.equal(saveOne.ok, true);
    assert.equal(saveOne.feature, "checkpoint-ops");
    assert.equal(saveOne.checkpoint.links.context, "history/checkpoint-ops/CONTEXT.md");
    assert.equal(saveOne.checkpoint.links.handoff, ".pulse/runtime/handoffs/planning.json");
    assert.equal(saveOne.checkpoint.links.runtime_snapshot, ".pulse/runtime/state.json");
    assert.equal(saveOne.checkpoint.links.verification, "history/checkpoint-ops/verification/");
    assert.equal(saveOne.checkpoint.links.lifecycle_summary, "history/checkpoint-ops/lifecycle-summary.md");
    assert.equal(saveOne.checkpoint.memory_hooks.critical_patterns, ".pulse/memory/critical-patterns.md");
    assert.deepEqual(saveOne.checkpoint.memory_hooks.learnings, [".pulse/memory/learnings/checkpoint-ops.md"]);
    assert.equal(saveTwo.ok, true);
    assert.equal(listPayload.ok, true);
    assert.equal(listPayload.checkpoints.count, 2);
    assert.equal(listPayload.checkpoints.latest.checkpoint_id, saveTwo.checkpoint.checkpoint_id);
    assert.equal(showPayload.ok, true);
    assert.equal(showPayload.checkpoint.checkpoint_id, saveOne.checkpoint.checkpoint_id);
    assert.equal(diffPayload.ok, true);
    assert.equal(diffPayload.diff.fields.summary.changed, true);
    assert.equal(diffPayload.diff.fields.next_action.changed, true);
    assert.equal(resumePayload.ok, true);
    assert.equal(resumePayload.resume_brief.checkpoint.checkpoint_id, saveTwo.checkpoint.checkpoint_id);
    assert.equal(resumePayload.resume_brief.lifecycle_summary, "history/checkpoint-ops/lifecycle-summary.md");
    assert.match(resumePayload.resume_brief.rendered_handoff_summary, /## Handoff Summary/);
    assert.match(resumePayload.resume_brief.rendered_resume_briefing, /## Resume Briefing/);
    assert.match(resumePayload.resume_brief.rendered_transfer_block, /PULSE TRANSFER/);
    assert.ok(resumePayload.resume_brief.next_reads.includes(saveTwo.checkpoint.path));
    assert.ok(resumePayload.resume_brief.next_reads.includes("history/checkpoint-ops/lifecycle-summary.md"));
    assert.equal(
      resumePayload.resume_brief.note,
      "Checkpoints are advisory snapshots. Current handoffs and state files remain authoritative; use lifecycle-summary.md as the durable audit trail when present.",
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkpoint commands prefer canonical history verification paths and fall back to legacy runtime paths", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);

    fs.mkdirSync(path.join(root, "history", "canonical-verification", "verification"), { recursive: true });
    fs.writeFileSync(path.join(root, "history", "canonical-verification", "CONTEXT.md"), "# Context\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "canonical-verification", "approach.md"), "# Approach\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "canonical-verification", "phase-plan.md"), "# Phase Plan\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "canonical-verification", "phase-1-contract.md"), "# Contract\n", "utf8");
    fs.writeFileSync(path.join(root, "history", "canonical-verification", "lifecycle-summary.md"), "# Lifecycle Summary\n", "utf8");
    fs.mkdirSync(path.join(root, ".pulse", "runs", "canonical-verification", "verification"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "canonical-verification",
        active_skill: "pulse:reviewing",
        phase: "reviewing",
        gate: "GATE 4",
        gate_status: "approved",
      }, null, 2)}\n`,
      "utf8",
    );

    const saveHistoryPreferred = JSON.parse(
      execFileSync(
        "node",
        [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "checkpoint", "save", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );

    assert.equal(saveHistoryPreferred.ok, true);
    assert.equal(saveHistoryPreferred.checkpoint.links.verification, "history/canonical-verification/verification/");
    assert.equal(saveHistoryPreferred.checkpoint.links.lifecycle_summary, "history/canonical-verification/lifecycle-summary.md");

    fs.rmSync(path.join(root, "history", "canonical-verification", "verification"), { recursive: true, force: true });

    const saveLegacyFallback = JSON.parse(
      execFileSync(
        "node",
        [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "checkpoint", "save", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );

    assert.equal(saveLegacyFallback.ok, true);
    assert.equal(saveLegacyFallback.checkpoint.links.verification, ".pulse/runs/canonical-verification/verification/");
    assert.equal(saveLegacyFallback.checkpoint.links.lifecycle_summary, "history/canonical-verification/lifecycle-summary.md");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("onboarding ignores legacy migration artifacts instead of moving them", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);

    fs.mkdirSync(path.join(root, ".pulse", "verification", "legacy-only"), { recursive: true });
    fs.writeFileSync(
      path.join(root, ".pulse", "verification", "legacy-only", "final-review.md"),
      "# Final Review\nlegacy only\n",
      "utf8",
    );
    fs.mkdirSync(path.join(root, "history", "learning", "learnings"), { recursive: true });
    fs.writeFileSync(
      path.join(root, "history", "learning", "learnings", "legacy-note.md"),
      "# Learning: Legacy Note\n\nKeep this where it is.\n",
      "utf8",
    );

    const checked = checkRepo(root);
    assert.equal(checked.status, "up_to_date");
    assert.equal(checked.actions.includes("migrate_legacy_learning_memory"), false);
    assert.equal(checked.actions.includes("migrate_legacy_critical_patterns"), false);
    assert.equal(checked.actions.includes("migrate_legacy_verification_artifacts"), false);

    const reapplied = applyRepo(root, false);
    assert.equal(reapplied.status, "up_to_date");
    assert.equal("migration_summary" in reapplied.result.managed_assets, false);
    assert.equal(
      fs.readFileSync(path.join(root, ".pulse", "verification", "legacy-only", "final-review.md"), "utf8"),
      "# Final Review\nlegacy only\n",
    );
    assert.equal(
      fs.readFileSync(path.join(root, "history", "learning", "learnings", "legacy-note.md"), "utf8"),
      "# Learning: Legacy Note\n\nKeep this where it is.\n",
    );
    assert.equal(fs.existsSync(path.join(root, "history", "legacy-only", "verification", "final-review.md")), false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkpoint commands fail soft for malformed entries, missing selectors, and invalid save inputs", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    const featureDir = path.join(root, ".pulse", "runtime", "checkpoints", "soft-fail-feature");
    fs.mkdirSync(featureDir, { recursive: true });
    fs.mkdirSync(path.join(featureDir, "beads-pre-rebuild-20260424T185318Z"), { recursive: true });
    fs.writeFileSync(path.join(featureDir, "beads.db"), "sqlite-cache\n", "utf8");
    fs.writeFileSync(path.join(featureDir, "beads.db-wal"), "wal\n", "utf8");
    fs.writeFileSync(path.join(featureDir, "beads.db-shm"), "shm\n", "utf8");
    fs.writeFileSync(path.join(featureDir, "issues.jsonl"), "{}\n", "utf8");
    fs.writeFileSync(path.join(featureDir, "config.yaml"), "path: beads\n", "utf8");
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "state.json"),
      `${JSON.stringify({
        active_feature: "soft-fail-feature",
        active_skill: "pulse:planning",
        phase: "planning",
        gate: "GATE 2",
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "checkpoints", "soft-fail-feature", "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: "2026-04-16T12:00:00.000Z",
        checkpoints: [
          { checkpoint_id: "valid", path: "valid.json" },
          { checkpoint_id: "broken", path: "broken.json" },
          { checkpoint_id: "missing", path: "missing.json" },
        ],
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(featureDir, "valid.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        checkpoint_id: "valid",
        feature: "soft-fail-feature",
        created_at: "2026-04-16T12:01:00.000Z",
        summary: "Valid checkpoint",
        next_action: "Continue validating checkpoint hygiene.",
        captured: { phase: "planning", gate: "GATE 2", mode: "standard_feature", story: "", bead: "" },
        links: {},
        blockers: [],
        memory_hooks: {},
      }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(featureDir, "broken.json"),
      "{not-json}\n",
      "utf8",
    );

    const listPayload = JSON.parse(
      execFileSync(
        "node",
        [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "checkpoint", "list", "--json"],
        { cwd: root, encoding: "utf8" },
      ),
    );
    const listText = execFileSync(
      "node",
      [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "checkpoint", "list"],
      { cwd: root, encoding: "utf8" },
    );
    const statusText = execFileSync(
      "node",
      [path.join(root, ".pulse", "scripts", "pulse_status.mjs")],
      { cwd: root, encoding: "utf8" },
    );

    let showExitCode = 0;
    let showStdout = "";
    try {
      showStdout = execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "show",
          "--json",
          "--checkpoint-id",
          "missing-entry",
        ],
        { cwd: root, encoding: "utf8" },
      );
    } catch (error) {
      showExitCode = error.status;
      showStdout = error.stdout;
    }

    let invalidSaveExitCode = 0;
    let invalidSaveStdout = "";
    try {
      invalidSaveStdout = execFileSync(
        "node",
        [
          path.join(root, ".pulse", "scripts", "pulse_status.mjs"),
          "checkpoint",
          "save",
          "--json",
          "--feature",
          "../escape-feature",
          "--checkpoint-id",
          "../outside",
        ],
        { cwd: root, encoding: "utf8" },
      );
    } catch (error) {
      invalidSaveExitCode = error.status;
      invalidSaveStdout = error.stdout;
    }
    const invalidSave = JSON.parse(invalidSaveStdout);

    const missingFeatureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));
    let missingFeatureSaveExitCode = 0;
    let missingFeatureSaveStdout = "";
    try {
      applyRepo(missingFeatureRoot, false);
      try {
        missingFeatureSaveStdout = execFileSync(
          "node",
          [
            path.join(missingFeatureRoot, ".pulse", "scripts", "pulse_status.mjs"),
            "checkpoint",
            "save",
            "--json",
          ],
          { cwd: missingFeatureRoot, encoding: "utf8" },
        );
      } catch (error) {
        missingFeatureSaveExitCode = error.status;
        missingFeatureSaveStdout = error.stdout;
      }
    } finally {
      fs.rmSync(missingFeatureRoot, { recursive: true, force: true });
    }
    const missingFeatureSave = JSON.parse(missingFeatureSaveStdout);

    assert.equal(listPayload.ok, true);
    assert.equal(listPayload.checkpoints.count, 1);
    assert.equal(listPayload.checkpoints.latest.checkpoint_id, "valid");
    assert.deepEqual(listPayload.checkpoints.invalid_checkpoint_files, [
      ".pulse/runtime/checkpoints/soft-fail-feature/broken.json",
    ]);
    assert.deepEqual(listPayload.checkpoints.manifest_reference_issues, [
      ".pulse/runtime/checkpoints/soft-fail-feature/broken.json",
      ".pulse/runtime/checkpoints/soft-fail-feature/missing.json",
    ]);
    assert.deepEqual(listPayload.checkpoints.foreign_artifacts, [
      ".pulse/runtime/checkpoints/soft-fail-feature/beads-pre-rebuild-20260424T185318Z/",
      ".pulse/runtime/checkpoints/soft-fail-feature/beads.db",
      ".pulse/runtime/checkpoints/soft-fail-feature/beads.db-shm",
      ".pulse/runtime/checkpoints/soft-fail-feature/beads.db-wal",
      ".pulse/runtime/checkpoints/soft-fail-feature/config.yaml",
      ".pulse/runtime/checkpoints/soft-fail-feature/issues.jsonl",
    ]);
    assert.match(listText, /Checkpoint warnings:/);
    assert.match(listText, /Foreign artifacts:/);
    assert.match(statusText, /checkpoint_warnings:/);
    assert.match(statusText, /beads-pre-rebuild-20260424T185318Z\//);
    assert.equal(showExitCode, 1);
    assert.equal(JSON.parse(showStdout).ok, false);
    assert.equal(JSON.parse(showStdout).error, "Checkpoint not found.");
    assert.equal(invalidSaveExitCode, 1);
    assert.equal(invalidSave.ok, false);
    assert.equal(invalidSave.error, "feature must not contain path traversal segments.");
    assert.equal(fs.existsSync(path.join(root, ".pulse", "outside.json")), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "escape-feature")), false);
    assert.equal(missingFeatureSaveExitCode, 1);
    assert.equal(missingFeatureSave.ok, false);
    assert.equal(missingFeatureSave.error, "Cannot save checkpoint without an active feature.");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("getNodeRuntimeStatus enforces the minimum supported major version", () => {
  assert.equal(getNodeRuntimeStatus("18.0.0").supported, true);
  assert.equal(getNodeRuntimeStatus("17.9.1").supported, false);
});

test("dependency report distinguishes dependency-free packaged skills from uncovered ones", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-coverage-"));
  const skillsRoot = path.join(root, "skills");

  try {
    const alphaDir = path.join(skillsRoot, "alpha");
    const betaDir = path.join(skillsRoot, "beta");
    fs.mkdirSync(alphaDir, { recursive: true });
    fs.mkdirSync(betaDir, { recursive: true });

    fs.writeFileSync(
      path.join(alphaDir, "SKILL.md"),
      [
        "---",
        "name: alpha",
        "metadata:",
        "  dependencies: []",
        "---",
        "",
        "# alpha",
        "",
      ].join("\n"),
      "utf8",
    );
    fs.writeFileSync(
      path.join(betaDir, "SKILL.md"),
      [
        "---",
        "name: beta",
        "description: uncovered fixture",
        "---",
        "",
        "# beta",
        "",
      ].join("\n"),
      "utf8",
    );

    const report = buildPulseDependencyReport({
      repoRoot: root,
      skillsRoot,
      globalCodexConfigPath: path.join(root, "missing-global.toml"),
      commandProbe: () => ({ available: true, detail: "unused in coverage test" }),
    });

    assert.equal(report.summary.skills_total, 2);
    assert.equal(report.summary.skills_covered, 1);
    assert.equal(report.summary.skills_dependency_free, 1);
    assert.equal(report.summary.skills_uncovered, 1);
    assert.equal(report.summary.skills_available, 1);
    assert.equal(report.summary.declared_dependencies, 0);
    assert.deepEqual(report.uncovered_skills.map((skill) => skill.skill_name), ["beta"]);

    const alpha = report.skills.find((skill) => skill.skill_name === "alpha");
    const beta = report.skills.find((skill) => skill.skill_name === "beta");
    assert.equal(alpha?.coverage_status, "dependency_free");
    assert.equal(alpha?.status, "available");
    assert.equal(beta?.coverage_status, "uncovered");
    assert.equal(beta?.status, "uncovered");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("dependency helper marks missing command and missing mcp_server dependencies", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-deps-"));
  const skillsRoot = path.join(root, "skills");

  try {
    const alphaDir = path.join(skillsRoot, "alpha");
    fs.mkdirSync(alphaDir, { recursive: true });
    fs.writeFileSync(
      path.join(alphaDir, "SKILL.md"),
      [
        "---",
        "name: alpha",
        "metadata:",
        "  dependencies:",
        "    - id: must-have-command",
        "      kind: command",
        "      command: definitely-missing-command",
        "      missing_effect: unavailable",
        "      reason: required",
        "    - id: am-server",
        "      kind: mcp_server",
        "      server_names: [mcp_agent_mail]",
        "      config_sources: [repo_codex_config, global_codex_config]",
        "      missing_effect: degraded",
        "      reason: coordination",
        "---",
        "",
        "# alpha",
        "",
      ].join("\n"),
      "utf8",
    );

    const report = buildPulseDependencyReport({
      repoRoot: root,
      skillsRoot,
      globalCodexConfigPath: path.join(root, "missing-global.toml"),
      commandProbe: () => ({ available: false, detail: "missing in test" }),
    });

    assert.equal(report.summary.skills_total, 1);
    assert.equal(report.summary.skills_available, 0);
    assert.equal(report.summary.skills_unavailable, 1);
    assert.equal(report.summary.missing_dependencies, 2);
    assert.equal(report.skills[0].status, "unavailable");
    assert.deepEqual(
      report.skills[0].missing_dependencies.map((dependency) => dependency.id).sort(),
      ["am-server", "must-have-command"],
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkRepo reports dependency health summary without blocking onboarding status", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    const payload = checkRepo(root);

    assert.equal(payload.status, "up_to_date");
    assert.ok(payload.details.dependency_health);
    assert.ok(typeof payload.details.dependency_health.summary.skills_total === "number");
    assert.ok(typeof payload.details.dependency_health.summary.skills_uncovered === "number");
    assert.ok(Array.isArray(payload.details.dependency_health.skills));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("installed pulse_status falls back to packaged dependency inventory in onboarded repos", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);

    assert.equal(
      fs.existsSync(path.join(root, ".pulse", "scripts", "pulse_dependency_inventory.json")),
      true,
    );
    assert.equal(fs.existsSync(path.join(root, "skills")), false);

    const payload = JSON.parse(
      execFileSync("node", [path.join(root, ".pulse", "scripts", "pulse_status.mjs"), "--json"], {
        cwd: root,
        encoding: "utf8",
      }),
    );

    assert.ok(payload.dependency_health.summary.skills_total > 0);
    assert.ok(payload.dependency_health.summary.skills_covered > 0);
    assert.ok(payload.dependency_health.skills.some((skill) => skill.skill_name === "using-pulse"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("checkRepo promotes missing dependency data into an operator-facing warning summary", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    const skillsRoot = path.join(root, "skills");
    const alphaDir = path.join(skillsRoot, "alpha");
    fs.mkdirSync(alphaDir, { recursive: true });
    fs.writeFileSync(
      path.join(alphaDir, "SKILL.md"),
      [
        "---",
        "name: alpha",
        "metadata:",
        "  dependencies:",
        "    - id: missing-cli",
        "      kind: command",
        "      command: definitely-missing-command",
        "      missing_effect: unavailable",
        "      reason: required for test",
        "    - id: missing-server",
        "      kind: mcp_server",
        "      server_names: [definitely_missing_mcp_server_name]",
        "      config_sources: [repo_codex_config, global_codex_config]",
        "      missing_effect: degraded",
        "      reason: required for test",
        "---",
        "",
        "# alpha",
        "",
      ].join("\n"),
      "utf8",
    );

    const payload = checkRepo(root);
    const warning = payload.details.dependency_warning;

    assert.equal(warning.status, "warning");
    assert.equal(warning.missing_dependencies_count, 2);
    assert.deepEqual(warning.affected_skills, ["alpha"]);
    assert.match(warning.message, /Dependency warning:/);
    assert.match(warning.message, /alpha/);
    assert.match(warning.message, /Missing commands: definitely-missing-command/);
    assert.match(
      warning.message,
      /Missing MCP server configuration: definitely_missing_mcp_server_name/,
    );
    assert.equal(warning.missing_commands.length, 1);
    assert.equal(warning.missing_commands[0].command, "definitely-missing-command");
    assert.equal(warning.missing_mcp_servers.length, 1);
    assert.deepEqual(warning.missing_mcp_servers[0].servers, ["definitely_missing_mcp_server_name"]);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("installed pulse_status text distinguishes missing commands from missing MCP config", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pulse-onboard-"));

  try {
    applyRepo(root, false);
    const skillsRoot = path.join(root, "skills");
    const alphaDir = path.join(skillsRoot, "alpha");
    fs.mkdirSync(alphaDir, { recursive: true });
    fs.writeFileSync(
      path.join(alphaDir, "SKILL.md"),
      [
        "---",
        "name: alpha",
        "metadata:",
        "  dependencies:",
        "    - id: missing-cli",
        "      kind: command",
        "      command: definitely-missing-command",
        "      missing_effect: unavailable",
        "      reason: required for test",
        "    - id: missing-server",
        "      kind: mcp_server",
        "      server_names: [definitely_missing_mcp_server_name]",
        "      config_sources: [repo_codex_config, global_codex_config]",
        "      missing_effect: degraded",
        "      reason: required for test",
        "---",
        "",
        "# alpha",
        "",
      ].join("\n"),
      "utf8",
    );

    const stdout = execFileSync("node", [path.join(root, ".pulse", "scripts", "pulse_status.mjs")], {
      cwd: root,
      encoding: "utf8",
    });
    assert.match(stdout, /Missing commands:/);
    assert.match(stdout, /Missing MCP server configuration:/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("packaged swarm contracts use native runtime adapters instead of Agent Mail", () => {
  const swarmingSkill = fs.readFileSync(
    path.join(LOCAL_REPO_ROOT, "skills", "swarming", "SKILL.md"),
    "utf8",
  );
  const executingSkill = fs.readFileSync(
    path.join(LOCAL_REPO_ROOT, "skills", "executing", "SKILL.md"),
    "utf8",
  );
  const runtimeAdapter = fs.readFileSync(
    path.join(LOCAL_REPO_ROOT, "skills", "swarming", "references", "runtime-adapter-spec.md"),
    "utf8",
  );
  const swarmingAppendix = fs.readFileSync(
    path.join(LOCAL_REPO_ROOT, "skills", "swarming", "references", "swarming-appendix.md"),
    "utf8",
  );
  const runtimeAppendix = fs.readFileSync(
    path.join(LOCAL_REPO_ROOT, "skills", "executing", "references", "runtime-appendix.md"),
    "utf8",
  );

  for (const text of [swarmingSkill, executingSkill, runtimeAdapter, swarmingAppendix, runtimeAppendix]) {
    assert.doesNotMatch(text, /Agent Mail/);
    assert.doesNotMatch(text, /mcp_agent_mail/);
  }

  assert.match(runtimeAdapter, /TeamCreate/);
  assert.match(runtimeAdapter, /SendMessage/);
  assert.match(runtimeAdapter, /Codex/);
  assert.match(runtimeAdapter, /pulse_reservations\.mjs/);
  assert.match(swarmingAppendix, /Worker Prompt Template/);
  assert.match(swarmingAppendix, /\[ONLINE\]/);
  assert.match(swarmingAppendix, /\[DONE\]/);
  assert.match(swarmingAppendix, /\[BLOCKED\]/);
  assert.match(swarmingAppendix, /\[FILE CONFLICT\]/);
  assert.match(runtimeAppendix, /latest coordinator updates/);
});

test("using-pulse messaging references /pulse onboard instead of legacy preflight wording", () => {
  const skillText = fs.readFileSync(LOCAL_USING_PULSE_SKILL_PATH, "utf8");

  assert.match(skillText, /pure router \+ scout/i);
  assert.match(skillText, /\/pulse onboard/i);
  assert.doesNotMatch(skillText, /pulse:preflight/i);
  assert.doesNotMatch(skillText, /run onboard_pulse\.mjs --apply/i);
});

test("packaged Pulse inventory has full dependency coverage", () => {
  const report = buildPulseDependencyReport({ repoRoot: LOCAL_REPO_ROOT });
  const skillText = fs.readFileSync(LOCAL_USING_PULSE_SKILL_PATH, "utf8");
  const pluginMcp = JSON.parse(
    fs.readFileSync(
      path.join(LOCAL_REPO_ROOT, ".mcp.json"),
      "utf8",
    ),
  );

  assert.equal(report.summary.skills_total, report.summary.skills_covered);
  assert.equal(report.summary.skills_uncovered, 0);
  assert.deepEqual(report.uncovered_skills, []);

  assert.match(skillText, /## Dependency Declaration Contract/);
  assert.match(skillText, /kind: command/);
  assert.match(skillText, /kind: mcp_server/);
  assert.match(skillText, /metadata\.dependencies: \[\]/);
  assert.match(skillText, /bash scripts\/sync-skills\.sh --dry-run/);
  assert.equal(pluginMcp.gitnexus.type, "stdio");
  assert.equal(pluginMcp.gitnexus.command, "npx");
  assert.deepEqual(pluginMcp.gitnexus.args, ["-y", "gitnexus@1.6.3", "mcp"]);
  assert.deepEqual(pluginMcp.gitnexus.includeTools, [
    "list_repos",
    "query",
    "context",
    "impact",
    "api_impact",
    "route_map",
    "shape_check",
    "detect_changes",
  ]);
});
