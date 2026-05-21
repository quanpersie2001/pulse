#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import {
  ensureReservationStore,
  readReservationStore,
  reservePaths,
  sweepExpiredReservations,
} from "../../skills/workflow/scripts/pulse_reservation_store.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("reservation conflict detection blocks overlapping paths across agents", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  try {
    const first = reservePaths(root, {
      agent: "agent-a",
      paths: ["skills/workflow/scripts/**"],
    });
    assert.equal(first.ok, true);

    const second = reservePaths(root, {
      agent: "agent-b",
      paths: ["skills/workflow/scripts/pulse_state.mjs"],
    });

    assert.equal(second.ok, false);
    assert.equal(second.conflicts.length, 1);
    assert.equal(second.conflicts[0].agent, "agent-a");
  } finally {
    cleanupTempRepo(root);
  }
});

test("sweep marks already-expired active reservations without waiting", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  try {
    ensureReservationStore(root);
    const reservationsPath = path.join(root, ".pulse", "runtime", "reservations.json");
    const expiredAt = new Date(Date.now() - 60_000).toISOString();

    fs.writeFileSync(
      reservationsPath,
      `${JSON.stringify({
        schema_version: "1.0",
        updated_at: new Date().toISOString(),
        reservations: [
          {
            id: "resv-expired",
            agent: "agent-a",
            item_id: "S-1",
            paths: ["works/**"],
            created_at: new Date(Date.now() - 120_000).toISOString(),
            updated_at: new Date(Date.now() - 120_000).toISOString(),
            ttl_seconds: 30,
            expires_at: expiredAt,
            status: "active",
            released_at: null,
            note: "",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );

    const swept = sweepExpiredReservations(root);
    assert.equal(swept.swept_count, 1);
    assert.deepEqual(swept.swept_ids, ["resv-expired"]);

    const store = readReservationStore(root);
    assert.equal(store.reservations[0].status, "expired");
  } finally {
    cleanupTempRepo(root);
  }
});
