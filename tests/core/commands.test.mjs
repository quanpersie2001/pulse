#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";

import {
  assertWorkflowCommand,
  loadWorkflowCommandMetadata,
  normalizeWorkflowCommand,
  validWorkflowCommands,
} from "../../skills/workflow/scripts/core/commands.mjs";

test("workflow command metadata exposes known commands", () => {
  const metadata = loadWorkflowCommandMetadata();
  const commands = validWorkflowCommands();

  assert.equal(metadata.router, "pulse:workflow");
  assert.equal(commands.has("use"), true);
  assert.equal(commands.has("plan"), true);
});

test("normalizeWorkflowCommand accepts bare and routed known commands only", () => {
  for (const { value, expected } of [
    { value: "use", expected: "pulse:workflow use" },
    { value: " pulse:workflow plan ", expected: "pulse:workflow plan" },
    { value: "pulse:unknown", expected: "" },
    { value: "not-a-command", expected: "" },
    { value: "", expected: "" },
  ]) {
    assert.equal(normalizeWorkflowCommand(value), expected);
  }
});

test("assertWorkflowCommand returns normalized commands and rejects invalid values", () => {
  assert.equal(assertWorkflowCommand("execute"), "pulse:workflow execute");
  assert.throws(() => assertWorkflowCommand("pulse:workflow missing"), /Invalid workflow command/);
});
