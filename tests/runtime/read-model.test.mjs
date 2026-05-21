#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";

import { reservePaths } from "../../skills/workflow/scripts/pulse_reservation_store.mjs";
import { readPulseStatus } from "../../skills/workflow/scripts/pulse_status_model.mjs";
import { buildSessionLoad } from "../../skills/workflow/scripts/pulse_session_load.mjs";
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
  } finally {
    cleanupTempRepo(root);
  }
});
