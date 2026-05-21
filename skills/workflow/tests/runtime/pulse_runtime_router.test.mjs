#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const TEST_FILE = fileURLToPath(import.meta.url);
const TEST_DIR = path.dirname(TEST_FILE);
const REPO_ROOT = path.resolve(TEST_DIR, "..", "..", "..", "..");
const PULSE_PATH = path.join(REPO_ROOT, "skills", "workflow", "scripts", "pulse.mjs");

function mkRoot() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "pulse-router-runtime-"));
}

function cleanup(root) {
  fs.rmSync(root, { recursive: true, force: true });
}

test("importing pulse router does not execute main", () => {
  const root = mkRoot();
  try {
    const importerPath = path.join(root, "import-pulse.mjs");
    fs.writeFileSync(importerPath, `import ${JSON.stringify(pathToFileURL(PULSE_PATH).href)};\n`, "utf8");

    const result = spawnSync(process.execPath, [importerPath], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    });

    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout, "");
  } finally {
    cleanup(root);
  }
});

test("pulse router renders help", () => {
  const output = execFileSync(process.execPath, [PULSE_PATH, "--help"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
  });

  assert.match(output, /Usage: pulse\.mjs <command> \[options\]/);
  assert.match(output, /status/);
  assert.match(output, /ready/);
  assert.match(output, /reservation/);
});

test("pulse router delegates status, ready, and reservation list", () => {
  const root = mkRoot();
  try {
    const statusOutput = execFileSync(process.execPath, [PULSE_PATH, "status", "--repo-root", root, "--json"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    });
    const statusPayload = JSON.parse(statusOutput);
    assert.equal(statusPayload.repo_root, root);

    const syncedStatusOutput = execFileSync(
      process.execPath,
      [PULSE_PATH, "status", "--repo-root", root, "--json", "--sync"],
      {
        cwd: REPO_ROOT,
        encoding: "utf8",
      },
    );
    const syncedStatusPayload = JSON.parse(syncedStatusOutput);
    assert.equal(syncedStatusPayload.repo_root, root);

    const readyOutput = execFileSync(process.execPath, [PULSE_PATH, "ready", "--repo-root", root, "--json"], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    });
    const readyPayload = JSON.parse(readyOutput);
    assert.equal(readyPayload.command, "ready");
    assert.equal(Array.isArray(readyPayload.items), true);

    const reservationOutput = execFileSync(
      process.execPath,
      [PULSE_PATH, "reservation", "list", "--repo-root", root, "--active-only", "--json"],
      {
        cwd: REPO_ROOT,
        encoding: "utf8",
      },
    );
    const reservationPayload = JSON.parse(reservationOutput);
    assert.equal(Array.isArray(reservationPayload.reservations), true);
  } finally {
    cleanup(root);
  }
});
