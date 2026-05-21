#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { parseJsonOutput, PULSE_PATH, spawnPulse } from "../helpers/spawn-pulse.mjs";

test("importing pulse router does not execute main", () => {
  const root = mkTempRepo("pulse-router-runtime-");
  try {
    const result = importModuleInNode(PULSE_PATH, { root, name: "pulse", cwd: REPO_ROOT });

    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "");
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse router renders help", () => {
  const result = spawnPulse(["--help"]);

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Usage: pulse\.mjs <command> \[options\]/);
  assert.match(result.stdout, /status/);
  assert.match(result.stdout, /ready/);
  assert.match(result.stdout, /reservation/);
});

test("pulse router delegates status, ready, and reservation list", () => {
  const root = mkTempRepo("pulse-router-runtime-");
  try {
    const statusResult = spawnPulse(["status", "--repo-root", root, "--json"]);
    assert.equal(statusResult.status, 0, statusResult.stderr);
    const statusPayload = parseJsonOutput(statusResult);
    assert.equal(statusPayload.repo_root, root);

    const syncedStatusResult = spawnPulse(["status", "--repo-root", root, "--json", "--sync"]);
    assert.equal(syncedStatusResult.status, 0, syncedStatusResult.stderr);
    const syncedStatusPayload = parseJsonOutput(syncedStatusResult);
    assert.equal(syncedStatusPayload.repo_root, root);

    const readyResult = spawnPulse(["ready", "--repo-root", root, "--json"]);
    assert.equal(readyResult.status, 0, readyResult.stderr);
    const readyPayload = parseJsonOutput(readyResult);
    assert.equal(readyPayload.command, "ready");
    assert.equal(Array.isArray(readyPayload.items), true);

    const reservationResult = spawnPulse([
      "reservation",
      "list",
      "--repo-root",
      root,
      "--active-only",
      "--json",
    ]);
    assert.equal(reservationResult.status, 0, reservationResult.stderr);
    const reservationPayload = parseJsonOutput(reservationResult);
    assert.equal(Array.isArray(reservationPayload.reservations), true);
  } finally {
    cleanupTempRepo(root);
  }
});
