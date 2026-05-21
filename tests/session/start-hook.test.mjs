#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

import { applyRepo } from "../../skills/workflow/scripts/onboard/apply.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { REPO_ROOT } from "../helpers/fixtures.mjs";

const SESSION_START_HOOK = path.join(REPO_ROOT, "hooks", "session-start.mjs");

function runSessionStart(payload, options = {}) {
  return spawnSync(process.execPath, [SESSION_START_HOOK], {
    cwd: options.cwd ?? REPO_ROOT,
    env: { ...process.env, ...(options.env ?? {}) },
    input: JSON.stringify(payload),
    encoding: "utf8",
  });
}

function parseHookOutput(result) {
  return JSON.parse(result.stdout).hookSpecificOutput;
}

test("SessionStart loads installed helper for downstream repos without repo-local scripts", () => {
  const root = mkTempRepo("pulse-session-start-");
  try {
    applyRepo(root, false);
    assert.equal(fs.existsSync(path.join(root, ".pulse", "scripts")), false);

    const result = runSessionStart({ cwd: root });

    assert.equal(result.status, 0, result.stderr);
    const output = parseHookOutput(result);
    assert.equal(output.hookEventName, "SessionStart");
    assert.match(output.additionalContext, /Pulse repo notes:/);
    assert.match(output.additionalContext, /Pulse is installed for this repo\./);
    assert.match(output.additionalContext, /# `pulse:workflow`/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("SessionStart ignores stale repo-local session shims by default", () => {
  const root = mkTempRepo("pulse-session-start-");
  try {
    applyRepo(root, false);
    const shimDir = path.join(root, ".pulse", "scripts");
    fs.mkdirSync(shimDir, { recursive: true });
    fs.writeFileSync(
      path.join(shimDir, "pulse_session_context.mjs"),
      "export async function buildPulseSessionStartContext() { return 'STALE LOCAL SHIM'; }\n",
      "utf8",
    );

    const result = runSessionStart({ cwd: root });

    assert.equal(result.status, 0, result.stderr);
    const output = parseHookOutput(result);
    assert.doesNotMatch(output.additionalContext, /STALE LOCAL SHIM/);
    assert.match(output.additionalContext, /# `pulse:workflow`/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("SessionStart compatibility mode can load repo-local session shims", () => {
  const root = mkTempRepo("pulse-session-start-");
  try {
    applyRepo(root, false);
    const shimDir = path.join(root, ".pulse", "scripts");
    fs.mkdirSync(shimDir, { recursive: true });
    fs.writeFileSync(
      path.join(shimDir, "pulse_session_context.mjs"),
      "export async function buildPulseSessionStartContext() { return 'COMPAT LOCAL SHIM'; }\n",
      "utf8",
    );

    const result = runSessionStart({ cwd: root }, { env: { PULSE_SESSION_START_COMPAT_SCRIPTS: "1" } });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(parseHookOutput(result).additionalContext, "COMPAT LOCAL SHIM");
  } finally {
    cleanupTempRepo(root);
  }
});
