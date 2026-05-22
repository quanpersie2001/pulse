#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { reservePaths } from "../../skills/workflow/scripts/reservation/store.mjs";
import { readPulseStatus } from "../../skills/workflow/scripts/runtime/read-model.mjs";
import { buildSessionLoad } from "../../skills/workflow/scripts/runtime/session-load.mjs";
import { readPulseState, writePulseState } from "../../skills/workflow/scripts/runtime/state.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("readPulseStatus exposes reservation compatibility summary keys", async () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  try {
    reservePaths(root, {
      agent: "agent-a",
      paths: ["docs/**"],
      ttlSeconds: 120,
    });

    const status = await readPulseStatus(root);
    const reservations = status.reservations;
    const expectedSessionLoad = buildSessionLoad(root);

    assert.equal(reservations.exists, true);
    assert.equal(typeof reservations.schema_version, "string");
    assert.equal(typeof reservations.updated_at, "string");
    assert.equal(typeof reservations.total, "number");
    assert.equal(typeof reservations.active_count, "number");
    assert.equal(typeof reservations.expired_count, "number");
    assert.equal(typeof reservations.released_count, "number");
    assert.equal(Array.isArray(reservations.active_agents), true);
    assert.equal(Array.isArray(reservations.active_reservations), true);
    assert.equal(reservations.active_count, 1);
    assert.deepEqual(reservations.active_agents, ["agent-a"]);
    assert.ok(status.session_load);
    assert.deepEqual(status.session_load, expectedSessionLoad);
    assert.equal(typeof status.session_load.next_command, "string");
    assert.ok(status.session_load.next_command.startsWith("pulse:workflow "));
    assert.equal(Object.prototype.hasOwnProperty.call(status.tooling_status, "next_skill"), false);
  } finally {
    cleanupTempRepo(root);
  }
});

test("readPulseStatus accepts legacy next-command aliases without re-emitting them", async () => {
  const root = mkTempRepo("pulse-read-model-runtime-");
  try {
    const runtimeDir = path.join(root, ".pulse", "runtime");
    fs.mkdirSync(runtimeDir, { recursive: true });
    fs.writeFileSync(
      path.join(runtimeDir, "state.json"),
      `${JSON.stringify({ next_skill_recommended: "validate" }, null, 2)}\n`,
      "utf8",
    );

    const status = await readPulseStatus(root);

    assert.equal(status.state_json.next_command, "pulse:workflow validate");
    assert.equal(Object.prototype.hasOwnProperty.call(status.state_json, "next_skill_recommended"), false);
    assert.equal(Object.prototype.hasOwnProperty.call(status.state_json, "next_command_recommended"), false);
  } finally {
    cleanupTempRepo(root);
  }
});

test("runtime state preserves normalized intake posture", async () => {
  const root = mkTempRepo("pulse-intake-state-");
  try {
    const written = writePulseState(root, {
      active_command: "intake",
      intake: {
        status: "awaiting_creation_confirmation",
        input_type: "harness_improvement",
        correlation_outcome: "existing_open_work",
        matched_item_ids: ["S-1", "", 7, "E-1"],
        linked_item_ids: ["S-2"],
        satisfaction_evidence_summary: "Existing story already covers the request.",
        lane: "normal",
        risk_flags: ["EXISTING_BEHAVIOR", "", null, "CI"],
        artifact_path: "works/epics/E-1/S-1/INTAKE.md",
        proposed_boundary: {
          kind: "STORY",
          title: "Improve intake workflow",
          path: "works/epics/E-1/S-1",
        },
        recommended_next_command: "explore",
      },
      unexpected_top_level_field: "dropped",
    });

    assert.deepEqual(written.intake, {
      status: "awaiting_creation_confirmation",
      input_type: "harness_improvement",
      correlation_outcome: "existing_open_work",
      matched_item_ids: ["S-1", "E-1"],
      linked_item_ids: ["S-2"],
      satisfaction_evidence_summary: "Existing story already covers the request.",
      lane: "normal",
      risk_flags: ["EXISTING_BEHAVIOR", "CI"],
      artifact_path: "works/epics/E-1/S-1/INTAKE.md",
      proposed_boundary: {
        kind: "STORY",
        title: "Improve intake workflow",
        path: "works/epics/E-1/S-1",
      },
      recommended_next_command: "pulse:workflow explore",
    });

    const read = readPulseState(root);
    assert.deepEqual(read.intake, written.intake);
    assert.equal(Object.prototype.hasOwnProperty.call(read, "unexpected_top_level_field"), false);

    const status = await readPulseStatus(root);
    assert.deepEqual(status.state_json.intake, written.intake);
  } finally {
    cleanupTempRepo(root);
  }
});
