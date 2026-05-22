#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { applyRepo } from "../../skills/workflow/scripts/onboard/apply.mjs";
import { checkRepo } from "../../skills/workflow/scripts/onboard/check.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { parseJsonOutput, spawnPulse } from "../helpers/spawn-pulse.mjs";


test("checkRepo/applyRepo expose FAIL->PASS readiness without optional external CLI blockers", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    const before = checkRepo(root);
    assert.equal(before.status, "FAIL");

    const applied = applyRepo(root, false);
    assert.equal(applied.status, "PASS");
    assert.equal(applied.details.tooling_status_preview.status, "pass");
  } finally {
    cleanupTempRepo(root);
  }
});

test("repo-local .pulse/scripts shims are optional compatibility only", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const shimDir = path.join(root, ".pulse", "scripts");
    fs.mkdirSync(shimDir, { recursive: true });
    fs.writeFileSync(path.join(shimDir, "pulse_status.mjs"), "// stale compatibility shim\n", "utf8");

    const checked = checkRepo(root);
    assert.equal(checked.status, "PASS");
    assert.doesNotMatch(JSON.stringify(checked.actions), /sync_pulse_support_scripts|pulse_status\.mjs/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("passive archive docs do not trigger active routing drift warnings", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const archiveDir = path.join(root, "docs", "archive");
    fs.mkdirSync(archiveDir, { recursive: true });
    fs.writeFileSync(
      path.join(archiveDir, "archived-routing-notes.md"),
      "Historical notes: removed route names in archived docs should not affect active readiness.\n",
      "utf8",
    );

    const checked = checkRepo(root);
    assert.equal(checked.status, "PASS");
    const serializedWarnings = JSON.stringify(checked.warnings || []);
    assert.doesNotMatch(serializedWarnings, /preflight|using-pulse|dream/i);
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse router exposes onboard check and apply", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    const initialCheck = spawnPulse(["onboard", "check", "--repo-root", root, "--json"]);
    assert.equal(initialCheck.status, 1);
    assert.equal(parseJsonOutput(initialCheck).status, "FAIL");

    const applied = spawnPulse(["onboard", "apply", "--repo-root", root, "--json"]);
    assert.equal(applied.status, 0, applied.stderr);
    assert.equal(parseJsonOutput(applied).status, "PASS");

    const finalCheck = spawnPulse(["onboard", "check", "--repo-root", root, "--json"]);
    assert.equal(finalCheck.status, 0, finalCheck.stderr);
    assert.equal(parseJsonOutput(finalCheck).status, "PASS");
  } finally {
    cleanupTempRepo(root);
  }
});

