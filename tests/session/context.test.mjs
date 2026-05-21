#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import path from "node:path";

import { applyRepo } from "../../skills/workflow/scripts/onboard_pulse.mjs";
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
