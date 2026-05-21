#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { main as pulseWorkMain } from "../../skills/workflow/scripts/pulse_work.mjs";
import { captureStdoutAsync } from "../helpers/capture-stdout.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, initGitRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { parseJsonOutput, spawnPulse } from "../helpers/spawn-pulse.mjs";

const SCRIPTS_DIR = path.join(REPO_ROOT, "skills", "workflow", "scripts");

test("pulse_work.mjs prefers --repo-root over env and cwd", async () => {
  const explicitRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const envRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const cwdRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    process.env.PULSE_REPO_ROOT = envRoot;
    process.chdir(cwdRoot);

    const call = await captureStdoutAsync(() =>
      pulseWorkMain(["--repo-root", explicitRoot, "list", "--json"]),
    );

    assert.equal(call.returnValue, 0);
    const payload = JSON.parse(call.output);
    assert.equal(payload.command, "list");
    assert.equal(Array.isArray(payload.items), true);

    assert.equal(fs.existsSync(path.join(explicitRoot, ".pulse", "workgraph", "items.jsonl")), true);
    assert.equal(fs.existsSync(path.join(envRoot, ".pulse", "workgraph", "items.jsonl")), false);
    assert.equal(fs.existsSync(path.join(cwdRoot, ".pulse", "workgraph", "items.jsonl")), false);
  } finally {
    process.chdir(originalCwd);
    if (previousEnv === undefined) {
      delete process.env.PULSE_REPO_ROOT;
    } else {
      process.env.PULSE_REPO_ROOT = previousEnv;
    }
    cleanupTempRepo(explicitRoot);
    cleanupTempRepo(envRoot);
    cleanupTempRepo(cwdRoot);
  }
});

test("pulse_work.mjs uses PULSE_REPO_ROOT when --repo-root is not provided", async () => {
  const envRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const cwdRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    process.env.PULSE_REPO_ROOT = envRoot;
    process.chdir(cwdRoot);

    const call = await captureStdoutAsync(() => pulseWorkMain(["list", "--json"]));
    assert.equal(call.returnValue, 0);

    const payload = JSON.parse(call.output);
    assert.equal(payload.command, "list");
    assert.equal(Array.isArray(payload.items), true);

    assert.equal(fs.existsSync(path.join(envRoot, ".pulse", "workgraph", "items.jsonl")), true);
    assert.equal(fs.existsSync(path.join(cwdRoot, ".pulse", "workgraph", "items.jsonl")), false);
  } finally {
    process.chdir(originalCwd);
    if (previousEnv === undefined) {
      delete process.env.PULSE_REPO_ROOT;
    } else {
      process.env.PULSE_REPO_ROOT = previousEnv;
    }
    cleanupTempRepo(envRoot);
    cleanupTempRepo(cwdRoot);
  }
});

test("pulse_work.mjs resolves git root from nested cwd", async () => {
  const gitRoot = mkTempRepo("pulse_work.mjs-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    delete process.env.PULSE_REPO_ROOT;
    initGitRepo(gitRoot);
    const nested = path.join(gitRoot, "nested", "dir");
    fs.mkdirSync(nested, { recursive: true });
    process.chdir(nested);

    const call = await captureStdoutAsync(() => pulseWorkMain(["list", "--json"]));
    assert.equal(call.returnValue, 0);

    const payload = JSON.parse(call.output);
    assert.equal(payload.command, "list");
    assert.equal(Array.isArray(payload.items), true);

    assert.equal(fs.existsSync(path.join(gitRoot, ".pulse", "workgraph", "items.jsonl")), true);
    assert.equal(fs.existsSync(path.join(nested, ".pulse", "workgraph", "items.jsonl")), false);
  } finally {
    process.chdir(originalCwd);
    if (previousEnv === undefined) {
      delete process.env.PULSE_REPO_ROOT;
    } else {
      process.env.PULSE_REPO_ROOT = previousEnv;
    }
    cleanupTempRepo(gitRoot);
  }
});

test("importing CLI modules does not execute their mains", () => {
  const root = mkTempRepo("pulse_work.mjs-runtime-");
  try {
    for (const scriptName of [
      "pulse_status.mjs",
      "pulse_reservations.mjs",
      "pulse_work.mjs",
      "pulse_session_load.mjs",
      "pulse_package_paths.mjs",
      "onboard_pulse.mjs",
    ]) {
      const result = importModuleInNode(path.join(SCRIPTS_DIR, scriptName), { root, name: scriptName, cwd: REPO_ROOT });

      assert.equal(result.status, 0, `${scriptName} import exited non-zero: ${result.stderr}`);
      assert.equal(result.stdout, "", `${scriptName} import wrote stdout`);
    }
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse_work.mjs create scaffolds files from workflow-owned work templates", async () => {
  const root = mkTempRepo("pulse_work.mjs-runtime-");
  try {
    const epicCall = await captureStdoutAsync(() =>
      pulseWorkMain(["--repo-root", root, "create", "--kind", "EPIC", "--title", "Runtime path fix", "--json"]),
    );
    assert.equal(epicCall.returnValue, 0);
    const epic = JSON.parse(epicCall.output).item;
    assert.equal(fs.existsSync(path.join(root, epic.content_path)), true);
    assert.match(fs.readFileSync(path.join(root, epic.content_path), "utf8"), /Runtime path fix/);

    const storyCall = await captureStdoutAsync(() =>
      pulseWorkMain([
        "--repo-root",
        root,
        "create",
        "--kind",
        "STORY",
        "--parent",
        epic.id,
        "--title",
        "Lock runtime contract",
        "--json",
      ]),
    );
    assert.equal(storyCall.returnValue, 0);
    const story = JSON.parse(storyCall.output).item;
    assert.equal(fs.existsSync(path.join(root, story.content_path)), true);
    assert.match(fs.readFileSync(path.join(root, story.content_path), "utf8"), /Lock runtime contract/);

    const taskCall = await captureStdoutAsync(() =>
      pulseWorkMain([
        "--repo-root",
        root,
        "create",
        "--kind",
        "TASK",
        "--parent",
        story.id,
        "--title",
        "Prove runtime templates",
        "--json",
      ]),
    );
    assert.equal(taskCall.returnValue, 0);
    const task = JSON.parse(taskCall.output).item;
    assert.equal(fs.existsSync(path.join(root, task.content_path)), true);
    assert.equal(fs.existsSync(path.join(root, task.verification_path)), true);
    assert.match(fs.readFileSync(path.join(root, task.content_path), "utf8"), /Prove runtime templates/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse router exposes workgraph list, ready, and create", () => {
  const root = mkTempRepo("pulse_work.mjs-runtime-");
  try {
    const listResult = spawnPulse(["workgraph", "list", "--repo-root", root, "--json"]);
    assert.equal(listResult.status, 0, listResult.stderr);
    const listPayload = parseJsonOutput(listResult);
    assert.equal(listPayload.command, "list");
    assert.equal(Array.isArray(listPayload.items), true);

    const readyResult = spawnPulse(["ready", "--repo-root", root, "--json"]);
    assert.equal(readyResult.status, 0, readyResult.stderr);
    assert.equal(parseJsonOutput(readyResult).command, "ready");

    const createResult = spawnPulse([
      "workgraph",
      "create",
      "--repo-root",
      root,
      "--kind",
      "EPIC",
      "--title",
      "Router workgraph item",
      "--json",
    ]);
    assert.equal(createResult.status, 0, createResult.stderr);
    const item = parseJsonOutput(createResult).item;
    assert.equal(item.kind, "EPIC");
    assert.equal(fs.existsSync(path.join(root, item.content_path)), true);
  } finally {
    cleanupTempRepo(root);
  }
});
