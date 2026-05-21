#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";

import {
  assertBareBooleanOptions,
  assertKnownOptions,
  parseCliArgs,
} from "../../skills/workflow/scripts/cli/args.mjs";
import { writeJson, writePayload, writeText } from "../../skills/workflow/scripts/cli/io.mjs";

test("parseCliArgs captures positionals, inline values, separated values, repeated flags, and bare booleans", () => {
  const parsed = parseCliArgs([
    "create",
    "--repo-root",
    "/tmp/repo",
    "--label=frontend",
    "--label",
    "backend",
    "--json",
  ]);

  assert.deepEqual(parsed.positionals, ["create"]);
  assert.equal(parsed.string("repo-root"), "/tmp/repo");
  assert.deepEqual(parsed.list("label"), ["frontend", "backend"]);
  assert.equal(parsed.has("json"), true);
  assert.equal(parsed.boolean("json"), true);
  assert.equal(parsed.string("missing", "fallback"), "fallback");
});

test("CLI argument assertions reject unknown and valued boolean options", () => {
  const parsed = parseCliArgs(["--json=false", "--unknown"]);

  assert.throws(() => assertKnownOptions(parsed, ["json"]), /Unknown argument: --unknown/);
  assert.throws(() => assertBareBooleanOptions(parsed, ["json"]), /Unknown argument: --json=false/);
});

test("CLI IO helpers write text, JSON, and rendered payloads with trailing newlines", () => {
  const writes = [];
  const output = { write: (value) => writes.push(value) };

  writeText("hello", output);
  writeJson({ ok: true }, output);
  writePayload({ value: 3 }, { output, render: (payload) => `value=${payload.value}` });
  writePayload({ ok: false }, { json: true, output });

  assert.deepEqual(writes, [
    "hello\n",
    `${JSON.stringify({ ok: true }, null, 2)}\n`,
    "value=3\n",
    `${JSON.stringify({ ok: false }, null, 2)}\n`,
  ]);
});
