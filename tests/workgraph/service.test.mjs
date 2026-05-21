#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  childrenOf,
  closeItem,
  createItem,
  doctor,
  graph,
  listItems,
  mutateDependencies,
  readyItems,
  reopenItem,
  showItem,
  updateItem,
} from "../../skills/workflow/scripts/workgraph/service.mjs";
import { getWorkgraphPaths } from "../../skills/workflow/scripts/workgraph_store.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

async function createStoryWithTask(root) {
  const epic = await createItem(root, { kind: "EPIC", title: "Service epic" });
  const story = await createItem(root, { kind: "STORY", parent: epic.item.id, title: "Service story" });
  const task = await createItem(root, { kind: "TASK", parent: story.item.id, title: "Service task" });
  return { epic: epic.item, story: story.item, task: task.item };
}

test("workgraph service creates items and returns decorated payloads", async () => {
  const root = mkTempRepo("pulse-workgraph-service-");
  try {
    const { epic, story, task } = await createStoryWithTask(root);

    assert.equal(epic.kind, "EPIC");
    assert.equal(story.parent_id, epic.id);
    assert.equal(task.parent_id, story.id);
    assert.equal(fs.existsSync(path.join(root, epic.content_path)), true);
    assert.equal(fs.existsSync(path.join(root, task.verification_path)), true);

    const shown = await showItem(root, { id: task.id });
    assert.equal(shown.command, "show");
    assert.equal(shown.item.id, task.id);
    assert.equal(Array.isArray(shown.item.children), true);
  } finally {
    cleanupTempRepo(root);
  }
});

test("workgraph service updates, filters, and mutates dependencies without CLI parsing", async () => {
  const root = mkTempRepo("pulse-workgraph-service-");
  try {
    const { story, task } = await createStoryWithTask(root);

    const updated = await updateItem(root, {
      id: task.id,
      status: "BLOCKED",
      blockedReason: "Waiting on API contract",
      blockedReasonProvided: true,
      owner: "agent-1",
      ownerProvided: true,
      addLabels: ["runtime"],
      addRisks: ["ci"],
    });
    assert.equal(updated.command, "update");
    assert.equal(updated.item.status, "BLOCKED");
    assert.equal(updated.item.owner, "agent-1");
    assert.deepEqual(updated.item.labels, ["runtime"]);
    assert.deepEqual(updated.item.risk_flags, ["CI"]);

    const dependency = await mutateDependencies(root, { mode: "add", id: task.id, dependencyId: story.id });
    assert.equal(dependency.command, "dep_add");
    assert.equal(dependency.dependency_id, story.id);

    const listed = await listItems(root, { owner: "agent-1", ownerProvided: true });
    assert.deepEqual(listed.items.map((item) => item.id), [task.id]);

    const ready = await readyItems(root);
    assert.equal(ready.command, "ready");
    assert.equal(ready.items.some((item) => item.id === task.id), false);

    const children = await childrenOf(root, { id: story.id });
    assert.deepEqual(children.items.map((item) => item.id), [task.id]);
  } finally {
    cleanupTempRepo(root);
  }
});

test("workgraph service closes, reopens, graphs, and doctors directly", async () => {
  const root = mkTempRepo("pulse-workgraph-service-");
  try {
    const standalone = await createItem(root, { kind: "EPIC", title: "Standalone epic" });

    const closed = await closeItem(root, { id: standalone.item.id });
    assert.equal(closed.command, "close");
    assert.equal(closed.item.status, "CLOSED");

    const reopened = await reopenItem(root, { id: standalone.item.id });
    assert.equal(reopened.command, "reopen");
    assert.equal(reopened.item.status, "OPEN");

    const graphPayload = await graph(root);
    assert.equal(graphPayload.command, "graph");
    assert.equal(graphPayload.graph.nodes.length, 1);

    const doctorPayload = await doctor(root);
    assert.equal(doctorPayload.command, "doctor");
    assert.equal(Array.isArray(doctorPayload.issues), true);
  } finally {
    cleanupTempRepo(root);
  }
});

test("workgraph doctor reports and fixes stale shared locks", async () => {
  const root = mkTempRepo("pulse-workgraph-service-");
  try {
    const paths = getWorkgraphPaths(root);
    fs.mkdirSync(path.dirname(paths.lockPath), { recursive: true });
    fs.writeFileSync(paths.lockPath, JSON.stringify({ pid: -1, hostname: os.hostname() }), "utf8");

    const report = await doctor(root);
    assert.equal(report.ok, false);
    assert.equal(report.issues.some((issue) => issue.code === "stale_lock"), true);
    assert.equal(fs.existsSync(paths.lockPath), true);

    const fixed = await doctor(root, { fix: true });
    assert.equal(fixed.fixed_actions.includes("remove_stale_lock"), true);
    assert.equal(fs.existsSync(paths.lockPath), false);
  } finally {
    cleanupTempRepo(root);
  }
});
