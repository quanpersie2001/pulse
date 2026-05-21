#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { resolveRepoRoot } from "../../skills/workflow/scripts/pulse_paths.mjs";
import { resolveRepoRoot as resolveOnboardRepoRoot } from "../../skills/workflow/scripts/onboard_pulse.mjs";
import { cleanupTempRepo, initGitRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("resolveRepoRoot respects explicitRoot over PULSE_REPO_ROOT and cwd", () => {
  const cwdRoot = mkTempRepo("pulse-paths-");
  const envRoot = mkTempRepo("pulse-paths-");
  const explicitRoot = mkTempRepo("pulse-paths-");
  try {
    const resolved = resolveRepoRoot({ explicitRoot, env: { PULSE_REPO_ROOT: envRoot }, cwd: cwdRoot });
    assert.equal(resolved, path.resolve(explicitRoot));
    assert.equal(resolveOnboardRepoRoot(explicitRoot, { PULSE_REPO_ROOT: envRoot }, cwdRoot), path.resolve(explicitRoot));
  } finally {
    cleanupTempRepo(cwdRoot);
    cleanupTempRepo(envRoot);
    cleanupTempRepo(explicitRoot);
  }
});

test("resolveRepoRoot uses PULSE_REPO_ROOT when explicitRoot is missing", () => {
  const cwdRoot = mkTempRepo("pulse-paths-");
  const envRoot = mkTempRepo("pulse-paths-");
  try {
    const resolved = resolveRepoRoot({ env: { PULSE_REPO_ROOT: envRoot }, cwd: cwdRoot });
    assert.equal(resolved, path.resolve(envRoot));
    assert.equal(resolveOnboardRepoRoot(undefined, { PULSE_REPO_ROOT: envRoot }, cwdRoot), path.resolve(envRoot));
  } finally {
    cleanupTempRepo(cwdRoot);
    cleanupTempRepo(envRoot);
  }
});

test("resolveRepoRoot resolves git top-level from nested cwd", () => {
  const root = mkTempRepo("pulse-paths-");
  try {
    initGitRepo(root);
    const nested = path.join(root, "nested", "dir");
    fs.mkdirSync(nested, { recursive: true });

    const resolved = resolveRepoRoot({ cwd: nested, env: {} });
    assert.equal(fs.realpathSync.native(resolved), fs.realpathSync.native(root));
  } finally {
    cleanupTempRepo(root);
  }
});
