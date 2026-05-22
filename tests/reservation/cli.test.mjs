#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import {
  ensureReservationStore,
  listReservations,
  releaseReservations,
  reservePaths,
  main as reservationsMain,
} from "../../skills/workflow/scripts/cli/reservation.mjs";
import { captureStdout } from "../helpers/capture-stdout.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { parseJsonOutput, spawnPulse } from "../helpers/spawn-pulse.mjs";

test("reservation module reserve/list/release lifecycle works with repo-root", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  try {
    ensureReservationStore(root);

    const reserved = reservePaths(root, {
      agent: "agent-a",
      itemId: "S-123",
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
    cleanupTempRepo(root);
  }
});

test("CLI main uses --repo-root store even when cwd points elsewhere", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  const otherRoot = mkTempRepo("pulse-reservations-runtime-");
  const originalCwd = process.cwd();
  try {
    process.chdir(otherRoot);

    const reserveCall = captureStdout(() =>
      reservationsMain([
        "--repo-root",
        root,
        "reserve",
        "--agent",
        "agent-main",
        "--item",
        "S-42",
        "--path",
        "tests/runtime/**",
        "--json",
      ]),
    );

    assert.equal(reserveCall.returnValue, 0);
    const reservePayload = JSON.parse(reserveCall.output);
    assert.equal(reservePayload.ok, true);
    assert.equal(reservePayload.reservation.agent, "agent-main");
    assert.equal(reservePayload.reservation.item_id, "S-42");

    const listCall = captureStdout(() =>
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
    cleanupTempRepo(root);
    cleanupTempRepo(otherRoot);
  }
});

test("CLI main falls back to cwd when repo root is not passed", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  const originalCwd = process.cwd();
  try {
    process.chdir(root);

    const call = captureStdout(() => reservationsMain(["list", "--json"]));
    assert.equal(call.returnValue, 0);

    const payload = JSON.parse(call.output);
    assert.equal(Array.isArray(payload.reservations), true);
    assert.equal(payload.reservations.length, 0);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "reservations.json")), false);
  } finally {
    process.chdir(originalCwd);
    cleanupTempRepo(root);
  }
});

test("pulse router exposes reservation reserve/list/release lifecycle", () => {
  const root = mkTempRepo("pulse-reservations-runtime-");
  try {
    const reserved = spawnPulse([
      "reservation",
      "reserve",
      "--repo-root",
      root,
      "--agent",
      "agent-router",
      "--item",
      "S-router",
      "--path",
      "skills/workflow/**",
      "--json",
    ]);
    assert.equal(reserved.status, 0, reserved.stderr);
    const reservedPayload = parseJsonOutput(reserved);
    assert.equal(reservedPayload.ok, true);
    assert.equal(reservedPayload.reservation.agent, "agent-router");

    const listed = spawnPulse(["reservation", "list", "--repo-root", root, "--active-only", "--json"]);
    assert.equal(listed.status, 0, listed.stderr);
    const listedPayload = parseJsonOutput(listed);
    assert.equal(listedPayload.reservations.length, 1);
    assert.equal(listedPayload.reservations[0].agent, "agent-router");

    const released = spawnPulse([
      "reservation",
      "release",
      "--repo-root",
      root,
      "--agent",
      "agent-router",
      "--json",
    ]);
    assert.equal(released.status, 0, released.stderr);
    assert.equal(parseJsonOutput(released).released_count, 1);
  } finally {
    cleanupTempRepo(root);
  }
});
