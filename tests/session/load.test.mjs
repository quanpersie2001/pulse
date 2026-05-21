#!/usr/bin/env node

import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

import { applyRepo, checkRepo } from "../../skills/workflow/scripts/onboard_pulse.mjs";
import { cleanupTempRepo, mkTempRepo } from "../helpers/temp-repo.mjs";

test("session_load auto-loads a single handoff while next_command remains canonical", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const manifestPath = path.join(root, ".pulse", "runtime", "handoffs", "manifest.json");
    fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
    fs.writeFileSync(
      manifestPath,
      `${JSON.stringify({
        updated_at: new Date().toISOString(),
        active: [
          {
            owner_id: "owner-a",
            owner_type: "workflow_command",
            surface: "pulse:workflow",
            active_command: "explore",
            active_epic_id: "E-0V9K4F",
            active_story_id: "S-0V9K4G",
            active_item_id: null,
            phase: "explore/context",
            summary: "resume later",
            path: ".pulse/runtime/handoffs/owner-a.json",
            next_action: "Please read docs and decide manually",
          },
        ],
      }, null, 2)}\n`,
      "utf8",
    );

    fs.writeFileSync(
      path.join(root, ".pulse", "runtime", "handoffs", "owner-a.json"),
      `${JSON.stringify({
        owner_id: "owner-a",
        active_command: "explore",
        active_epic_id: "E-0V9K4F",
        active_story_id: "S-0V9K4G",
        summary: "resume later",
        next_action: "Please read docs and decide manually",
        read_first: [".pulse/runtime/handoffs/owner-a.json"],
      }, null, 2)}\n`,
      "utf8",
    );

    const reapplied = applyRepo(root, false);
    assert.match(reapplied.next_command, /^pulse:workflow\s+/);
    assert.notEqual(reapplied.next_command, "pulse:workflow Please read docs and decide manually");

    const toolingStatus = JSON.parse(
      fs.readFileSync(path.join(root, ".pulse", "runtime", "tooling-status.json"), "utf8"),
    );
    assert.match(toolingStatus.next_command, /^pulse:workflow\s+/);
    assert.notEqual(toolingStatus.next_command, "pulse:workflow Please read docs and decide manually");
    assert.equal(toolingStatus.session_load.posture, "conflicted");
    assert.equal(toolingStatus.session_load.selected_handoff.owner_id, "owner-a");
    assert.ok(toolingStatus.session_load.conflicts.some((entry) => entry.includes("S-0V9K4G")));
    assert.ok(toolingStatus.session_load.read_first.includes(".pulse/runtime/handoffs/owner-a.json"));
  } finally {
    cleanupTempRepo(root);
  }
});

test("session_load requires selection for multiple handoffs and rejects unsafe read_first paths", () => {
  const root = mkTempRepo("pulse-onboard-routing-");
  try {
    applyRepo(root, false);

    const handoffsDir = path.join(root, ".pulse", "runtime", "handoffs");
    fs.writeFileSync(
      path.join(handoffsDir, "owner-a.json"),
      `${JSON.stringify({ summary: "a", read_first: ["../escape.md"] }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(handoffsDir, "owner-b.json"),
      `${JSON.stringify({ summary: "b" }, null, 2)}\n`,
      "utf8",
    );
    fs.writeFileSync(
      path.join(handoffsDir, "manifest.json"),
      `${JSON.stringify({
        schema_version: "1.0",
        active: [
          { owner_id: "owner-a", owner_type: "workflow_command", surface: "pulse:workflow", active_command: "plan", path: ".pulse/runtime/handoffs/owner-a.json", summary: "a" },
          { owner_id: "owner-b", owner_type: "workflow_command", surface: "pulse:workflow", active_command: "execute", path: ".pulse/runtime/handoffs/owner-b.json", summary: "b" },
        ],
      }, null, 2)}\n`,
      "utf8",
    );

    const unselected = checkRepo(root).details.tooling_status_preview.session_load;
    assert.equal(unselected.requires_selection, true);
    assert.equal(unselected.read_first.length, 0);

    const selected = checkRepo(root, { resumeOwner: "owner-a" }).details.tooling_status_preview.session_load;
    assert.equal(selected.selected_handoff.owner_id, "owner-a");
    assert.ok(selected.rejected_paths.includes("../escape.md"));
  } finally {
    cleanupTempRepo(root);
  }
});
