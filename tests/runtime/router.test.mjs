#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
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

test("pulse router renders global and command help", () => {
  const helpResult = spawnPulse(["--help"]);

  assert.equal(helpResult.status, 0, helpResult.stderr);
  assert.match(helpResult.stdout, /Pulse runtime workflow CLI/);
  assert.match(helpResult.stdout, /Usage: pulse\.mjs \[OPTIONS\] <COMMAND>/);
  assert.match(helpResult.stdout, /status\s+Inspect Pulse runtime posture/);
  assert.match(helpResult.stdout, /ready\s+Show work items that are unblocked/);
  assert.match(helpResult.stdout, /reservation\s+Coordinate worker claims/);
  assert.match(helpResult.stdout, /session-load\s+Load the safe resume packet/);
  assert.match(helpResult.stdout, /onboard\s+Check or create required Pulse runtime/);
  assert.match(helpResult.stdout, /workgraph\s+Maintain canonical work items/);
  assert.match(helpResult.stdout, /Subcommands:/);
  assert.match(helpResult.stdout, /reserve\s+Claim a work item/);
  assert.match(helpResult.stdout, /check\s+Validate required \.pulse runtime\/workgraph files/);
  assert.match(helpResult.stdout, /create\s+Create an epic\/story\/task\/bug item/);
  assert.match(helpResult.stdout, /Options:/);
  assert.match(helpResult.stdout, /Examples:/);

  const commandHelpResult = spawnPulse(["help", "session-load"]);
  assert.equal(commandHelpResult.status, 0, commandHelpResult.stderr);
  assert.match(commandHelpResult.stdout, /Description:/);
  assert.match(commandHelpResult.stdout, /Command usage:/);
  assert.match(commandHelpResult.stdout, /Usage: pulse\.mjs session-load \[--repo-root <repo>\]/);

  const workgraphHelpResult = spawnPulse(["help", "workgraph"]);
  assert.equal(workgraphHelpResult.status, 0, workgraphHelpResult.stderr);
  assert.match(workgraphHelpResult.stdout, /Subcommands:/);
  assert.match(workgraphHelpResult.stdout, /create\s+Create an epic\/story\/task\/bug item/);
  assert.match(workgraphHelpResult.stdout, /dep\s+Manage blocking dependency edges/);
  assert.match(workgraphHelpResult.stdout, /dep add <id> <depends-on>/);
});

test("pulse router executes through direct and symlinked entrypoints", () => {
  const root = mkTempRepo("pulse-router-runtime-");
  const tempScriptRoot = mkTempRepo("pulse-router-runtime-");
  try {
    const symlinkPath = path.join(tempScriptRoot, "pulse_symlink.mjs");
    fs.symlinkSync(PULSE_PATH, symlinkPath);

    const directResult = spawnPulse(["status", "--repo-root", root, "--json"]);
    assert.equal(directResult.status, 0, directResult.stderr);
    assert.equal(parseJsonOutput(directResult).repo_root, root);

    const symlinkResult = spawnPulse(["status", "--repo-root", root, "--json"], { pulsePath: symlinkPath });
    assert.equal(symlinkResult.status, 0, symlinkResult.stderr);
    assert.equal(parseJsonOutput(symlinkResult).repo_root, root);
  } finally {
    cleanupTempRepo(root);
    cleanupTempRepo(tempScriptRoot);
  }
});

test("pulse router delegates command groups", () => {
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

    const sessionLoadResult = spawnPulse(["session-load", "--repo-root", root, "--json"]);
    assert.equal(sessionLoadResult.status, 0, sessionLoadResult.stderr);
    const sessionLoadPayload = parseJsonOutput(sessionLoadResult);
    assert.equal(typeof sessionLoadPayload.posture, "string");

    const onboardResult = spawnPulse(["onboard", "apply", "--repo-root", root, "--json"]);
    assert.equal(onboardResult.status, 0, onboardResult.stderr);
    const onboardPayload = parseJsonOutput(onboardResult);
    assert.equal(onboardPayload.status, "PASS");

    const workgraphResult = spawnPulse(["workgraph", "list", "--repo-root", root, "--json"]);
    assert.equal(workgraphResult.status, 0, workgraphResult.stderr);
    const workgraphPayload = parseJsonOutput(workgraphResult);
    assert.equal(workgraphPayload.command, "list");
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse router rejects unknown commands with help", () => {
  const result = spawnPulse(["unknown-command"]);

  assert.equal(result.status, 1);
  assert.match(result.stderr, /Unknown command: unknown-command/);
  assert.match(result.stderr, /Usage: pulse\.mjs \[OPTIONS\] <COMMAND>/);
});
