#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";

import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { parseJsonOutput, spawnPulse } from "../helpers/spawn-pulse.mjs";

const STATUS_SCRIPT_PATH = path.join(REPO_ROOT, "skills", "workflow", "scripts", "pulse_status.mjs");
const STATUS_ADAPTER_PATH = path.join(REPO_ROOT, "skills", "workflow", "scripts", "cli", "status.mjs");

test("importing pulse_status does not run main", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    const result = importModuleInNode(STATUS_SCRIPT_PATH, { root, name: "status", cwd: root });

    assert.equal(result.status, 0);
    assert.equal(result.stdout, "");
    assert.equal(result.stderr, "");
  } finally {
    cleanupTempRepo(root);
  }
});

test("importing cli status adapter does not run main", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    const result = importModuleInNode(STATUS_ADAPTER_PATH, { root, name: "status-adapter", cwd: root });

    assert.equal(result.status, 0);
    assert.equal(result.stdout, "");
    assert.equal(result.stderr, "");
  } finally {
    cleanupTempRepo(root);
  }
});

test("status CLI runs through pulse router", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    const result = spawnPulse(["status", "--repo-root", root, "--json"], { cwd: root });

    assert.equal(result.status, 0);
    assert.equal(result.stderr, "");

    const payload = parseJsonOutput(result);
    assert.ok(typeof payload.repo_root === "string");
    assert.ok(payload.session_load);
    assert.ok(typeof payload.session_load.next_command === "string");
    assert.ok(payload.session_load.next_command.length > 0);
  } finally {
    cleanupTempRepo(root);
  }
});
