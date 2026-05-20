#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

import {
  ensureReservationStore,
  listReservations,
  readReservationStore,
  releaseReservations,
  reservePaths,
  sweepExpiredReservations,
  main as reservationsMain,
} from "../../scripts/pulse_reservations.mjs";
import { readPulseStatus } from "../../scripts/pulse_status_model.mjs";
import { buildSessionLoad } from "../../scripts/pulse_session_load.mjs";
import { resolveRepoRoot } from "../../scripts/pulse_paths.mjs";

function mkRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "pulse-reservations-runtime-"));
}

function cleanup(root) {
  fs.rmSync(root, { recursive: true, force: true });
}

function withCapturedStdout(run) {
  const writes = [];
  const originalWrite = process.stdout.write;
  process.stdout.write = (chunk, encoding, callback) => {
    writes.push(Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk));
    if (typeof callback === "function") {
      callback();
    }
    return true;
  };

  try {
    const returnValue = run();
    return { returnValue, output: writes.join("") };
  } finally {
    process.stdout.write = originalWrite;
  }
}

test("reservation module reserve/list/release lifecycle works with repo-root", () => {
  const root = mkRoot();
  try {
    ensureReservationStore(root);

    const reserved = reservePaths(root, {
      agent: "agent-a",
      beadId: "S-123",
      paths: ["skills/workflow/**"],
      ttlSeconds: 120,
      note: "runtime test",
    });

    assert.equal(reserved.ok, true);
    assert.equal(reserved.conflicts.length, 0);
    assert.equal(reserved.reservation.status, "active");

    const listedActive = listReservations(root, { activeOnly: true });
    assert.equal(listedActive.reservations.length, 1);
    assert.equal(listedActive.reservations[0].agent, "agent-a");

    const released = releaseReservations(root, { agent: "agent-a" });
    assert.equal(released.released_count, 1);

    const listedAfterRelease = listReservations(root, { status: "released" });
    assert.equal(listedAfterRelease.reservations.length, 1);
    assert.equal(listedAfterRelease.reservations[0].status, "released");
  } finally {
    cleanup(root);
  }
});

test("reservation conflict detection blocks overlapping paths across agents", () => {
  const root = mkRoot();
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
    cleanup(root);
  }
});

test("sweep marks already-expired active reservations without waiting", () => {
  const root = mkRoot();
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
            bead_id: "S-1",
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
    cleanup(root);
  }
});

test("CLI main uses --repo-root store even when cwd points elsewhere", () => {
  const root = mkRoot();
  const otherRoot = mkRoot();
  const originalCwd = process.cwd();
  try {
    process.chdir(otherRoot);

    const reserveCall = withCapturedStdout(() =>
      reservationsMain([
        "--repo-root",
        root,
        "reserve",
        "--agent",
        "agent-main",
        "--item",
        "S-42",
        "--path",
        "skills/workflow/tests/**",
        "--json",
      ]),
    );

    assert.equal(reserveCall.returnValue, 0);
    const reservePayload = JSON.parse(reserveCall.output);
    assert.equal(reservePayload.ok, true);
    assert.equal(reservePayload.reservation.agent, "agent-main");
    assert.equal(reservePayload.reservation.bead_id, "S-42");

    const listCall = withCapturedStdout(() =>
      reservationsMain(["--repo-root", root, "list", "--active-only", "--json"]),
    );
    assert.equal(listCall.returnValue, 0);
    const listPayload = JSON.parse(listCall.output);
    assert.equal(Array.isArray(listPayload.reservations), true);
    assert.equal(listPayload.reservations.length, 1);
    assert.equal(listPayload.reservations[0].agent, "agent-main");

    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "reservations.json")), true);
    assert.equal(fs.existsSync(path.join(otherRoot, ".pulse", "runtime", "reservations.json")), false);
  } finally {
    process.chdir(originalCwd);
    cleanup(root);
    cleanup(otherRoot);
  }
});

test("CLI main falls back to cwd when repo root is not passed", () => {
  const root = mkRoot();
  const originalCwd = process.cwd();
  try {
    process.chdir(root);

    const call = withCapturedStdout(() => reservationsMain(["list", "--json"]));
    assert.equal(call.returnValue, 0);

    const payload = JSON.parse(call.output);
    assert.equal(Array.isArray(payload.reservations), true);
    assert.equal(payload.reservations.length, 0);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "reservations.json")), false);
  } finally {
    process.chdir(originalCwd);
    cleanup(root);
  }
});

test("resolveRepoRoot resolves git top-level from nested cwd", () => {
  const root = mkRoot();
  try {
    execFileSync("git", ["init", "-q"], { cwd: root, stdio: "ignore" });
    const nested = path.join(root, "nested", "dir");
    fs.mkdirSync(nested, { recursive: true });

    const resolved = resolveRepoRoot({ cwd: nested, env: {} });
    assert.equal(fs.realpathSync.native(resolved), fs.realpathSync.native(root));
  } finally {
    cleanup(root);
  }
});

test("readPulseStatus exposes reservation compatibility summary keys", async () => {
  const root = mkRoot();
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
    cleanup(root);
  }
});
