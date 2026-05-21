#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { parseJsonOutput, PULSE_PATH, spawnPulse, spawnWorkflowScript } from "../helpers/spawn-pulse.mjs";

const CLI_DIR = path.join(REPO_ROOT, "skills", "workflow", "scripts", "cli");

function runAdapter(scriptName, args, root) {
  return spawnWorkflowScript(path.join(CLI_DIR, scriptName), args, { env: { PULSE_REPO_ROOT: root } });
}

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
  assert.match(helpResult.stdout, /Usage: pulse\.mjs <command> \[options\]/);
  assert.match(helpResult.stdout, /status \[--repo-root <repo>\] \[--json\] \[--sync\]/);
  assert.match(helpResult.stdout, /ready \[--repo-root <repo>\] \[--json\]/);
  assert.match(helpResult.stdout, /reservation <reserve\|release\|list\|sweep> \[options\]/);
  assert.match(helpResult.stdout, /session-load \[--repo-root <repo>\] \[--resume-owner <owner_id>\] \[--json\]/);
  assert.match(helpResult.stdout, /onboard <check\|apply> \[--repo-root <repo>\] \[--resume-owner <owner_id>\] \[--json\]/);
  assert.match(helpResult.stdout, /workgraph <create\|show\|list\|ready\|update\|close\|reopen\|dep\|children\|graph\|doctor> \[options\]/);
  assert.match(helpResult.stdout, /help \[command\]/);

  const commandHelpResult = spawnPulse(["help", "session-load"]);
  assert.equal(commandHelpResult.status, 0, commandHelpResult.stderr);
  assert.match(commandHelpResult.stdout, /Command:/);
  assert.match(commandHelpResult.stdout, /session-load \[--repo-root <repo>\]/);
});

test("cli adapters execute directly", () => {
  const root = mkTempRepo("pulse-router-runtime-");
  try {
    const statusResult = runAdapter("status.mjs", ["--repo-root", root, "--json"], root);
    assert.equal(statusResult.status, 0, statusResult.stderr);
    assert.equal(JSON.parse(statusResult.stdout).repo_root, root);

    const readyResult = runAdapter("ready.mjs", ["--repo-root", root, "--json"], root);
    assert.equal(readyResult.status, 0, readyResult.stderr);
    assert.equal(JSON.parse(readyResult.stdout).command, "ready");

    const reservationResult = runAdapter("reservation.mjs", ["list", "--repo-root", root, "--json"], root);
    assert.equal(reservationResult.status, 0, reservationResult.stderr);
    assert.equal(Array.isArray(JSON.parse(reservationResult.stdout).reservations), true);

    const sessionLoadResult = runAdapter("session-load.mjs", ["--repo-root", root, "--json"], root);
    assert.equal(sessionLoadResult.status, 0, sessionLoadResult.stderr);
    assert.equal(typeof JSON.parse(sessionLoadResult.stdout).posture, "string");

    const onboardResult = runAdapter("onboard.mjs", ["apply", "--repo-root", root, "--json"], root);
    assert.equal(onboardResult.status, 0, onboardResult.stderr);
    assert.equal(JSON.parse(onboardResult.stdout).status, "PASS");

    const workgraphResult = runAdapter("workgraph.mjs", ["list", "--repo-root", root, "--json"], root);
    assert.equal(workgraphResult.status, 0, workgraphResult.stderr);
    assert.equal(JSON.parse(workgraphResult.stdout).command, "list");
  } finally {
    cleanupTempRepo(root);
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
  assert.match(result.stderr, /Usage: pulse\.mjs <command> \[options\]/);
});
