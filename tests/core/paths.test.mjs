#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import {
  getPulsePaths,
  relativePosix,
  resolveRepoRoot,
  resolveSafeRepoRelativePath,
} from "../../skills/workflow/scripts/core/paths.mjs";
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

test("getPulsePaths and relativePosix return canonical runtime paths", () => {
  const root = path.resolve("/tmp/pulse-root");
  const paths = getPulsePaths(root);

  assert.equal(paths.toolingStatus, path.join(root, ".pulse", "runtime", "tooling-status.json"));
  assert.equal(paths.handoffManifest, path.join(root, ".pulse", "runtime", "handoffs", "manifest.json"));
  assert.equal(paths.criticalPatterns, path.join(root, ".pulse", "memory", "critical-patterns.md"));
  assert.equal(relativePosix(root, path.join(root, "works", "story", "SPEC.md")), "works/story/SPEC.md");
});

test("resolveSafeRepoRelativePath accepts only allowed normalized repo-relative paths", () => {
  const root = path.resolve("/tmp/pulse-root");

  for (const relativePath of [
    "AGENTS.md",
    ".pulse/runtime/handoffs/owner.json",
    ".pulse/memory/critical-patterns.md",
    "works/story/SPEC.md",
    "docs/README.md",
  ]) {
    assert.deepEqual(resolveSafeRepoRelativePath(root, relativePath), {
      relative: relativePath,
      absolute: path.join(root, ...relativePath.split("/")),
    });
  }

  for (const relativePath of [
    "",
    "/absolute",
    "../outside",
    "works/../AGENTS.md",
    "package.json",
    ".pulse/runtime/state.json",
    ".pulse/runtime/handoffs/../state.json",
    "works/%2e%2e/secrets",
  ]) {
    assert.equal(resolveSafeRepoRelativePath(root, relativePath), null);
  }
});
