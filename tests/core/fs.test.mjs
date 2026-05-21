#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import {
  copyPathIfExists,
  ensureDirectory,
  ensureParent,
  listDirectoryEntries,
  movePathIfExists,
  readJsonIfExists,
  readTextIfExists,
  writeJsonAtomic,
  writeTextAtomic,
} from "../../skills/workflow/scripts/core/fs.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("read helpers return file contents or safe empty/null fallbacks", () => {
  const root = mkTempRepo("pulse-core-fs-");
  try {
    const textPath = path.join(root, "text.txt");
    const jsonPath = path.join(root, "data.json");
    const badJsonPath = path.join(root, "bad.json");
    fs.writeFileSync(textPath, "hello", "utf8");
    fs.writeFileSync(jsonPath, JSON.stringify({ ok: true }), "utf8");
    fs.writeFileSync(badJsonPath, "{", "utf8");

    assert.equal(readTextIfExists(textPath), "hello");
    assert.equal(readTextIfExists(path.join(root, "missing.txt")), "");
    assert.deepEqual(readJsonIfExists(jsonPath), { ok: true });
    assert.equal(readJsonIfExists(badJsonPath), null);
    assert.equal(readJsonIfExists(path.join(root, "missing.json")), null);
  } finally {
    cleanupTempRepo(root);
  }
});

test("write and directory helpers create parents and preserve atomic output format", () => {
  const root = mkTempRepo("pulse-core-fs-");
  try {
    const dirPath = path.join(root, "nested");
    const textPath = path.join(dirPath, "text.txt");
    const jsonPath = path.join(dirPath, "data.json");

    ensureDirectory(dirPath);
    ensureParent(textPath);
    writeTextAtomic(textPath, "hello");
    writeJsonAtomic(jsonPath, { ok: true });

    assert.equal(fs.readFileSync(textPath, "utf8"), "hello");
    assert.equal(fs.readFileSync(jsonPath, "utf8"), `${JSON.stringify({ ok: true }, null, 2)}\n`);
    assert.deepEqual(listDirectoryEntries(dirPath).sort(), ["data.json", "text.txt"]);
  } finally {
    cleanupTempRepo(root);
  }
});

test("copy and move helpers no-op for missing sources and create target parents", () => {
  const root = mkTempRepo("pulse-core-fs-");
  try {
    const sourcePath = path.join(root, "source.txt");
    const copyPath = path.join(root, "copy", "source.txt");
    const movePath = path.join(root, "move", "source.txt");

    assert.equal(copyPathIfExists(path.join(root, "missing.txt"), copyPath), false);
    assert.equal(movePathIfExists(path.join(root, "missing.txt"), movePath), false);

    fs.writeFileSync(sourcePath, "content", "utf8");
    assert.equal(copyPathIfExists(sourcePath, copyPath), true);
    assert.equal(fs.readFileSync(copyPath, "utf8"), "content");
    assert.equal(movePathIfExists(sourcePath, movePath), true);
    assert.equal(fs.existsSync(sourcePath), false);
    assert.equal(fs.readFileSync(movePath, "utf8"), "content");
  } finally {
    cleanupTempRepo(root);
  }
});
