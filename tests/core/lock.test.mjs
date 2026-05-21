#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  acquireJsonFileLock,
  inspectJsonFileLock,
  releaseJsonFileLock,
  removeStaleJsonFileLock,
} from "../../skills/workflow/scripts/core/lock.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("core lock acquires, inspects, and releases lock files", () => {
  const root = mkTempRepo("pulse-core-lock-");
  try {
    const lockPath = path.join(root, "nested", "store.json.lock");
    const lock = acquireJsonFileLock(lockPath, { command: "test command", owner: "agent-1" });

    assert.equal(fs.existsSync(lockPath), true);
    assert.equal(lock.path, lockPath);
    assert.equal(lock.metadata.command, "test command");
    assert.equal(lock.metadata.owner, "agent-1");

    const active = inspectJsonFileLock(lockPath);
    assert.equal(active.exists, true);
    assert.equal(active.stale, false);
    assert.equal(active.metadata.pid, process.pid);
    assert.equal(active.reason, "active");

    assert.equal(releaseJsonFileLock(lock), true);
    assert.equal(inspectJsonFileLock(lockPath).exists, false);
  } finally {
    cleanupTempRepo(root);
  }
});

test("core lock removes stale timeout locks during acquisition", () => {
  const root = mkTempRepo("pulse-core-lock-");
  try {
    const lockPath = path.join(root, "store.json.lock");
    fs.writeFileSync(lockPath, JSON.stringify({ pid: process.pid, hostname: os.hostname() }), "utf8");
    const oldTime = new Date(Date.now() - 10_000);
    fs.utimesSync(lockPath, oldTime, oldTime);

    const stale = inspectJsonFileLock(lockPath, { staleMs: 1 });
    assert.equal(stale.exists, true);
    assert.equal(stale.stale, true);
    assert.equal(stale.reason, "stale_timeout");

    const lock = acquireJsonFileLock(lockPath, { staleMs: 1, timeoutMs: 20, retryMs: 1 });
    assert.equal(lock.metadata.pid, process.pid);
    assert.equal(releaseJsonFileLock(lock), true);
  } finally {
    cleanupTempRepo(root);
  }
});

test("core lock rejects active locks after timeout", () => {
  const root = mkTempRepo("pulse-core-lock-");
  try {
    const lockPath = path.join(root, "store.json.lock");
    const lock = acquireJsonFileLock(lockPath);

    assert.throws(
      () => acquireJsonFileLock(lockPath, { timeoutMs: 1, retryMs: 1, timeoutMessage: "lock timeout" }),
      /lock timeout/,
    );

    releaseJsonFileLock(lock);
  } finally {
    cleanupTempRepo(root);
  }
});

test("core lock can report stale locks without removing them", () => {
  const root = mkTempRepo("pulse-core-lock-");
  try {
    const lockPath = path.join(root, "store.json.lock");
    fs.writeFileSync(lockPath, JSON.stringify({ pid: -1, hostname: os.hostname() }), "utf8");

    assert.throws(
      () => acquireJsonFileLock(lockPath, { removeStale: false, staleMessage: "stale lock" }),
      /stale lock/,
    );
    assert.equal(fs.existsSync(lockPath), true);
  } finally {
    cleanupTempRepo(root);
  }
});

test("core lock treats malformed and same-host dead-pid locks as stale", () => {
  const root = mkTempRepo("pulse-core-lock-");
  try {
    const malformedPath = path.join(root, "malformed.lock");
    fs.writeFileSync(malformedPath, "{", "utf8");
    assert.equal(inspectJsonFileLock(malformedPath).stale, true);
    assert.equal(removeStaleJsonFileLock(malformedPath), true);

    const deadPidPath = path.join(root, "dead-pid.lock");
    fs.writeFileSync(deadPidPath, JSON.stringify({ pid: -1, hostname: os.hostname() }), "utf8");
    const deadPid = inspectJsonFileLock(deadPidPath);
    assert.equal(deadPid.exists, true);
    assert.equal(deadPid.stale, true);
    assert.equal(deadPid.reason, "dead_process");
    assert.equal(removeStaleJsonFileLock(deadPidPath), true);
  } finally {
    cleanupTempRepo(root);
  }
});
