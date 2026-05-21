#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { applyRepo } from "../../skills/workflow/scripts/onboard_pulse.mjs";
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
    assert.ok(typeof payload.tooling_status.next_command === "string");
    assert.equal(Object.prototype.hasOwnProperty.call(payload.tooling_status, "next_skill"), false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "runtime", "reservations.json")), false);
  } finally {
    cleanupTempRepo(root);
  }
});

test("status text renders canonical next_command labels", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    applyRepo(root, false);

    const result = spawnPulse(["status", "--repo-root", root], { cwd: root });

    assert.equal(result.status, 0);
    assert.equal(result.stderr, "");
    assert.match(result.stdout, /next_command: pulse:workflow /);
    assert.doesNotMatch(result.stdout, /next_skill/);
    assert.doesNotMatch(result.stdout, /next_command_recommended/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("status recommendations prefer gate-derived next_command over session-load default", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    fs.mkdirSync(path.join(root, ".pulse", "runtime"), { recursive: true });
    fs.writeFileSync(path.join(root, ".pulse", "runtime", "onboarding.json"), `${JSON.stringify({ status: "complete" })}\n`, "utf8");
    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "STATE.md"),
      [
        "Gate: GATE 2",
        "Gate status: approved",
        "Work shape status: approved",
        "Current work status: ready",
      ].join("\n"),
      "utf8",
    );

    const result = spawnPulse(["status", "--repo-root", root, "--json"], { cwd: root });

    assert.equal(result.status, 0, result.stderr);
    const payload = parseJsonOutput(result);
    assert.equal(payload.runtime_snapshot.next_command, "pulse:workflow validate");
    assert.equal(payload.recommended_actions[0], "Gate cleared. Manually invoke pulse:workflow validate when ready.");
  } finally {
    cleanupTempRepo(root);
  }
});

test("status does not mutate an existing reservation store", () => {
  const root = mkTempRepo("pulse-status-runtime-");
  try {
    const reservationsPath = path.join(root, ".pulse", "runtime", "reservations.json");
    fs.mkdirSync(path.dirname(reservationsPath), { recursive: true });
    const original = `${JSON.stringify({ schema_version: "1.0", updated_at: "2026-01-01T00:00:00.000Z", reservations: [] }, null, 2)}\n`;
    fs.writeFileSync(reservationsPath, original, "utf8");

    const result = spawnPulse(["status", "--repo-root", root, "--json", "--sync"], { cwd: root });

    assert.equal(result.status, 0, result.stderr);
    assert.equal(fs.readFileSync(reservationsPath, "utf8"), original);
  } finally {
    cleanupTempRepo(root);
  }
});
