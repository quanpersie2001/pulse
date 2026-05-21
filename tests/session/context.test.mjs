#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { applyRepo } from "../../skills/workflow/scripts/onboard/apply.mjs";
import {
  buildPulseSessionStartContext,
  collectPulseSessionStartNotes,
} from "../../skills/workflow/scripts/pulse_session_context.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("session-start context helper aligns with runtime session routing outputs", async () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const notes = await collectPulseSessionStartNotes(root, { syncRuntimeArtifactsIfOnboarded: true });
    const joined = notes.join("\n");

    assert.match(joined, /Pulse is installed for this repo\./);
    assert.match(joined, /Pulse session posture:/);
    assert.match(joined, /Recommended next workflow command: pulse:workflow /);
  } finally {
    cleanupTempRepo(root);
  }
});

test("session-start notes prefer gate-derived next_command over session-load default", async () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    const runtimeDir = path.join(root, ".pulse", "runtime");
    fs.mkdirSync(runtimeDir, { recursive: true });
    fs.writeFileSync(path.join(runtimeDir, "onboarding.json"), `${JSON.stringify({ status: "complete" })}\n`, "utf8");
    fs.writeFileSync(
      path.join(runtimeDir, "STATE.md"),
      [
        "Gate: GATE 2",
        "Gate status: approved",
        "Work shape status: approved",
        "Current work status: ready",
      ].join("\n"),
      "utf8",
    );

    const notes = await collectPulseSessionStartNotes(root);

    assert.match(notes.join("\n"), /Recommended next workflow command: pulse:workflow validate\./);
  } finally {
    cleanupTempRepo(root);
  }
});

test("bootstrap session context loads the workflow skill source", async () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  const previousPluginRoot = process.env.CLAUDE_PLUGIN_ROOT;
  try {
    process.env.CLAUDE_PLUGIN_ROOT = path.join(root, "missing-plugin-root");
    const context = await buildPulseSessionStartContext(root, {
      includeBootstrapSkill: true,
      syncRuntimeArtifactsIfOnboarded: false,
    });

    assert.match(context, /# `pulse:workflow`/);
    assert.match(context, /- `\{\{pulse_command\}\} \.\.\.` reads and coordinates runtime state through the installed workflow skill\./);
    assert.doesNotMatch(context, /\{\{pulse_command\}\} status/);
  } finally {
    if (previousPluginRoot === undefined) {
      delete process.env.CLAUDE_PLUGIN_ROOT;
    } else {
      process.env.CLAUDE_PLUGIN_ROOT = previousPluginRoot;
    }
    cleanupTempRepo(root);
  }
});
