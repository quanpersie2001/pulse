#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import { spawnSync } from "node:child_process";

import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

const STATUS_SCRIPT_PATH = path.join(REPO_ROOT, "skills", "workflow", "scripts", "pulse_status.mjs");

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

test("direct execution of pulse_status still works", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    const result = spawnSync(process.execPath, [STATUS_SCRIPT_PATH, "--repo-root", root, "--json"], {
      cwd: root,
      encoding: "utf8",
    });

    assert.equal(result.status, 0);
    assert.equal(result.stderr, "");

    const payload = JSON.parse(result.stdout);
    assert.ok(typeof payload.repo_root === "string");
    assert.ok(payload.session_load);
    assert.ok(typeof payload.session_load.next_command === "string");
    assert.ok(payload.session_load.next_command.length > 0);
  } finally {
    cleanupTempRepo(root);
  }
});
