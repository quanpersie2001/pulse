#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { main as workgraphMain } from "../../skills/workflow/scripts/cli/workgraph.mjs";
import { captureStdoutAsync } from "../helpers/capture-stdout.mjs";
import { importModuleInNode } from "../helpers/import-module.mjs";
import { cleanupTempRepo, initGitRepo, mkTempRepo } from "../helpers/temp-repo.mjs";
import { REPO_ROOT } from "../helpers/fixtures.mjs";
import { parseJsonOutput, spawnPulse } from "../helpers/spawn-pulse.mjs";

const SCRIPTS_DIR = path.join(REPO_ROOT, "skills", "workflow", "scripts");

test("cli/workgraph.mjs prefers --repo-root over env and cwd", async () => {
  const explicitRoot = mkTempRepo("cli-workgraph-runtime-");
  const envRoot = mkTempRepo("cli-workgraph-runtime-");
  const cwdRoot = mkTempRepo("cli-workgraph-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    process.env.PULSE_REPO_ROOT = envRoot;
    process.chdir(cwdRoot);

    const call = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", explicitRoot, "list", "--json"]),
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

test("cli/workgraph.mjs uses PULSE_REPO_ROOT when --repo-root is not provided", async () => {
  const envRoot = mkTempRepo("cli-workgraph-runtime-");
  const cwdRoot = mkTempRepo("cli-workgraph-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    process.env.PULSE_REPO_ROOT = envRoot;
    process.chdir(cwdRoot);

    const call = await captureStdoutAsync(() => workgraphMain(["list", "--json"]));
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

test("cli/workgraph.mjs resolves git root from nested cwd", async () => {
  const gitRoot = mkTempRepo("cli-workgraph-runtime-");
  const originalCwd = process.cwd();
  const previousEnv = process.env.PULSE_REPO_ROOT;

  try {
    delete process.env.PULSE_REPO_ROOT;
    initGitRepo(gitRoot);
    const nested = path.join(gitRoot, "nested", "dir");
    fs.mkdirSync(nested, { recursive: true });
    process.chdir(nested);

    const call = await captureStdoutAsync(() => workgraphMain(["list", "--json"]));
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

test("cli/workgraph.mjs rejects unknown flags, valued booleans, and extra positionals", async () => {
  const root = mkTempRepo("cli-workgraph-runtime-");
  try {
    const unknownFlag = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "list", "--statuz", "OPEN", "--json"]),
    );
    assert.equal(unknownFlag.returnValue, 1);
    assert.match(JSON.parse(unknownFlag.output).error, /Unknown argument: --statuz/);

    const valuedBoolean = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "list", "--json=false"]),
    );
    assert.equal(valuedBoolean.returnValue, 1);
    assert.match(valuedBoolean.output, /Unknown argument: --json=false/);

    const extraPositional = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "list", "extra", "--json"]),
    );
    assert.equal(extraPositional.returnValue, 1);
    assert.match(JSON.parse(extraPositional.output).error, /Unknown argument: extra/);

    const listFix = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "list", "--fix", "--json"]),
    );
    assert.equal(listFix.returnValue, 1);
    assert.match(JSON.parse(listFix.output).error, /Unknown argument: --fix/);

    const readyKind = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "ready", "--kind", "EPIC", "--json"]),
    );
    assert.equal(readyKind.returnValue, 1);
    assert.match(JSON.parse(readyKind.output).error, /Unknown argument: --kind/);

    const createStatus = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "create", "--kind", "EPIC", "--title", "Bad flag", "--status", "OPEN", "--json"]),
    );
    assert.equal(createStatus.returnValue, 1);
    assert.match(JSON.parse(createStatus.output).error, /Unknown argument: --status/);
  } finally {
    cleanupTempRepo(root);
  }
});

test("importing CLI modules does not execute their mains", () => {
  const root = mkTempRepo("cli-workgraph-runtime-");
  try {
    for (const scriptName of [
      "cli/status.mjs",
      "cli/reservation.mjs",
      "cli/workgraph.mjs",
      "cli/session-load.mjs",
      "core/package-paths.mjs",
      "cli/onboard.mjs",
    ]) {
      const result = importModuleInNode(path.join(SCRIPTS_DIR, scriptName), { root, name: scriptName, cwd: REPO_ROOT });

      assert.equal(result.status, 0, `${scriptName} import exited non-zero: ${result.stderr}`);
      assert.equal(result.stdout, "", `${scriptName} import wrote stdout`);
    }
  } finally {
    cleanupTempRepo(root);
  }
});

test("cli/workgraph.mjs create scaffolds files from workflow-owned work templates", async () => {
  const root = mkTempRepo("cli-workgraph-runtime-");
  try {
    const epicCall = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "create", "--kind", "EPIC", "--title", "Runtime path fix", "--json"]),
    );
    assert.equal(epicCall.returnValue, 0);
    const epic = JSON.parse(epicCall.output).item;
    assert.equal(fs.existsSync(path.join(root, epic.content_path)), true);
    assert.deepEqual(epic.linked_items, []);
    assert.match(fs.readFileSync(path.join(root, epic.content_path), "utf8"), /Runtime path fix/);

    const storyCall = await captureStdoutAsync(() =>
      workgraphMain([
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
      workgraphMain([
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

test("cli/workgraph.mjs links items, reports graph links, and keeps links out of ready blocking", async () => {
  const root = mkTempRepo("cli-workgraph-runtime-");
  try {
    const epicCall = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "create", "--kind", "EPIC", "--title", "Router epic", "--json"]),
    );
    const relatedEpicCall = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "create", "--kind", "EPIC", "--title", "Related epic", "--json"]),
    );
    const storyCall = await captureStdoutAsync(() =>
      workgraphMain([
        "--repo-root",
        root,
        "create",
        "--kind",
        "STORY",
        "--parent",
        JSON.parse(epicCall.output).item.id,
        "--title",
        "Router story",
        "--json",
      ]),
    );

    const epic = JSON.parse(epicCall.output).item;
    const relatedEpic = JSON.parse(relatedEpicCall.output).item;
    const story = JSON.parse(storyCall.output).item;

    const linkAdd = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "link", "add", story.id, relatedEpic.id, "--json"]),
    );
    assert.equal(linkAdd.returnValue, 0);
    const linkAddPayload = JSON.parse(linkAdd.output);
    assert.equal(linkAddPayload.command, "link_add");
    assert.equal(linkAddPayload.linked_item_id, relatedEpic.id);
    assert.deepEqual(linkAddPayload.item.linked_items, [relatedEpic.id]);

    const showCall = await captureStdoutAsync(() => workgraphMain(["--repo-root", root, "show", story.id, "--json"]));
    const shown = JSON.parse(showCall.output).item;
    assert.deepEqual(shown.linked_items, [relatedEpic.id]);
    assert.deepEqual(shown.reverse_links, []);

    const reverseShowCall = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "show", relatedEpic.id, "--json"]),
    );
    const reverseShown = JSON.parse(reverseShowCall.output).item;
    assert.deepEqual(reverseShown.reverse_links, [story.id]);

    const graphCall = await captureStdoutAsync(() => workgraphMain(["--repo-root", root, "graph", "--json"]));
    const graphPayload = JSON.parse(graphCall.output);
    assert.deepEqual(graphPayload.graph.edges.links, [{ from: story.id, to: relatedEpic.id }]);

    const readyCall = await captureStdoutAsync(() => workgraphMain(["--repo-root", root, "ready", "--json"]));
    const readyPayload = JSON.parse(readyCall.output);
    assert.equal(readyPayload.items.some((item) => item.id === story.id), true);

    const selfLink = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "link", "add", story.id, story.id, "--json"]),
    );
    assert.equal(selfLink.returnValue, 1);
    assert.match(JSON.parse(selfLink.output).error, /cannot link to itself/i);

    const missingLink = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "link", "add", story.id, "E-MISSING", "--json"]),
    );
    assert.equal(missingLink.returnValue, 1);
    assert.match(JSON.parse(missingLink.output).error, /No item matches lookup: E-MISSING/);

    const linkRm = await captureStdoutAsync(() =>
      workgraphMain(["--repo-root", root, "link", "rm", story.id, relatedEpic.id, "--json"]),
    );
    assert.equal(linkRm.returnValue, 0);
    const linkRmPayload = JSON.parse(linkRm.output);
    assert.equal(linkRmPayload.command, "link_rm");
    assert.deepEqual(linkRmPayload.item.linked_items, []);
  } finally {
    cleanupTempRepo(root);
  }
});

test("pulse router exposes workgraph list, ready, create, and link", () => {
  const root = mkTempRepo("cli-workgraph-runtime-");
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

    const relatedResult = spawnPulse([
      "workgraph",
      "create",
      "--repo-root",
      root,
      "--kind",
      "EPIC",
      "--title",
      "Router related item",
      "--json",
    ]);
    assert.equal(relatedResult.status, 0, relatedResult.stderr);
    const related = parseJsonOutput(relatedResult).item;

    const linkResult = spawnPulse([
      "workgraph",
      "link",
      "add",
      "--repo-root",
      root,
      item.id,
      related.id,
      "--json",
    ]);
    assert.equal(linkResult.status, 0, linkResult.stderr);
    assert.equal(parseJsonOutput(linkResult).command, "link_add");
  } finally {
    cleanupTempRepo(root);
  }
});
