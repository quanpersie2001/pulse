#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

import {
  getPulseEntrypointPath,
  getScriptDir,
  getWorkflowSkillPath,
} from "./pulse_package_paths.mjs";
import { syncPulseRuntimeArtifacts } from "./pulse_runtime_sync.mjs";
import { readGitNexusReadiness } from "./pulse_gitnexus_readiness.mjs";
import { readPulseStatus } from "./pulse_status_model.mjs";

const SCRIPT_DIR = getScriptDir(import.meta.url);
const PULSE_ENTRYPOINT_PATH = getPulseEntrypointPath(SCRIPT_DIR);
const PULSE_COMMAND = `node ${JSON.stringify(PULSE_ENTRYPOINT_PATH)}`;
const WORKFLOW_SKILL_PATH = getWorkflowSkillPath(SCRIPT_DIR);

export function findPulseRepoRoot(start) {
  let candidate = path.resolve(start || ".");
  while (true) {
    if (fs.existsSync(path.join(candidate, ".pulse", "runtime", "onboarding.json"))) {
      return candidate;
    }
    if (fs.existsSync(path.join(candidate, ".git"))) {
      return candidate;
    }
    const parent = path.dirname(candidate);
    if (parent === candidate) {
      return candidate;
    }
    candidate = parent;
  }
}

export async function readHookPayload(stream = process.stdin) {
  const chunks = [];
  for await (const chunk of stream) {
    chunks.push(chunk);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return JSON.parse(raw || "{}");
}

function readPulseSkillText() {
  if (!fs.existsSync(WORKFLOW_SKILL_PATH)) {
    return "";
  }
  return fs.readFileSync(WORKFLOW_SKILL_PATH, "utf8").trim();
}

function buildPulseBootstrapBlock() {
  const skillText = readPulseSkillText();
  if (!skillText) {
    return "";
  }

  return [
    "<EXTREMELY_IMPORTANT>",
    "You have Pulse.",
    "",
    "Below is the full content of your `pulse:workflow use` session-entry skill. Use it to route safely before loading downstream Pulse skills:",
    "",
    skillText,
    "</EXTREMELY_IMPORTANT>",
  ].join("\n");
}

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return "";
}

function buildPulseSessionPostureSummary(status) {
  const runtimeSnapshot = status?.runtime_snapshot || {};
  const currentFeature = status?.current_feature || {};
  const handoffManifest = status?.handoff_manifest || {};
  const sessionLoad = status?.tooling_status?.session_load || runtimeSnapshot.session_load || {};
  const activeContext = sessionLoad.active_context || {};

  const phase = firstNonEmptyString(runtimeSnapshot.phase, currentFeature.phase, "idle");
  const feature = firstNonEmptyString(
    activeContext.active_item_id,
    activeContext.active_story_id,
    activeContext.active_epic_id,
    runtimeSnapshot.active_item_id,
    runtimeSnapshot.active_story_id,
    runtimeSnapshot.active_epic_id,
    runtimeSnapshot.active_feature,
    currentFeature.feature_key,
    "(none)",
  );
  const gate = firstNonEmptyString(runtimeSnapshot.gate, currentFeature.gate, "(none)");
  const gateStatus = firstNonEmptyString(runtimeSnapshot.gate_status, currentFeature.gate_status);
  const nextCommand = firstNonEmptyString(
    sessionLoad.next_command,
    runtimeSnapshot.next_command_recommended,
    runtimeSnapshot.next_command,
    currentFeature.next_skill_recommended,
    runtimeSnapshot.next_skill_recommended,
    status?.tooling_status?.next_command,
    status?.tooling_status?.next_skill,
  );

  const activeHandoffs = Array.isArray(handoffManifest.active) ? handoffManifest.active : [];
  const handoffSummary =
    activeHandoffs.length > 0
      ? activeHandoffs
          .slice(0, 2)
          .map((entry) => entry?.operator_summary || entry?.handoff_summary || entry?.path || "(unknown handoff)")
          .filter(Boolean)
          .join(" ; ")
      : "none";

  const routing = nextCommand
    ? `Recommended next workflow command: ${nextCommand}.`
    : "No explicit next workflow command is recorded yet; run pulse:workflow explore if you need to establish direction.";

  const gateSummary = gateStatus ? `${gate} (${gateStatus})` : gate;

  return (
    `Pulse session posture: phase=${phase}, feature=${feature}, gate=${gateSummary}. ` +
    `Active handoffs: ${activeHandoffs.length}. ${activeHandoffs.length > 0 ? `Top handoffs: ${handoffSummary}. ` : ""}` +
    routing
  );
}

export async function collectPulseSessionStartNotes(repoRoot, options = {}) {
  const { syncRuntimeArtifactsIfOnboarded = true } = options;
  const onboardingPath = path.join(repoRoot, ".pulse", "runtime", "onboarding.json");
  const criticalPatterns = path.join(repoRoot, ".pulse", "memory", "critical-patterns.md");

  const notes = [];
  if (fs.existsSync(onboardingPath)) {
    if (syncRuntimeArtifactsIfOnboarded) {
      syncPulseRuntimeArtifacts(repoRoot);
    }
    notes.push(
      `Pulse is installed for this repo. Read AGENTS.md, then run pulse:workflow use before substantive work. For a scout snapshot, run \`${PULSE_COMMAND} status --repo-root <repo> --json\`.`,
    );

    try {
      const status = await readPulseStatus(repoRoot);
      notes.push(buildPulseSessionPostureSummary(status));
    } catch {
      notes.push(
        `Pulse session posture could not be loaded from runtime artifacts; rerun pulse:workflow use to re-establish session context. Then run \`${PULSE_COMMAND} status --repo-root <repo> --json\` to inspect current handoff/next-step context.`,
      );
    }
  } else {
    notes.push("Pulse readiness has not been established for this repo. Run pulse:workflow use before continuing.");
  }

  if (fs.existsSync(criticalPatterns)) {
    notes.push(
      "If you move into planning, start with .pulse/memory/critical-patterns.md and then use pulse_status recall pointers for narrower learnings, corrections, and ratchet rules.",
    );
  }

  const gitNexusReadiness = await readGitNexusReadiness(repoRoot);
  if (gitNexusReadiness.configured) {
    notes.push(`gitnexus readiness: ${gitNexusReadiness.recommended_action}`);
  } else {
    notes.push(
      "GitNexus is not configured for this repo/session, so architecture discovery should use grep/file inspection fallback unless the MCP server is added.",
    );
  }

  return notes;
}

export async function buildPulseSessionStartContext(repoRoot, options = {}) {
  const {
    includeBootstrapSkill = false,
    syncRuntimeArtifactsIfOnboarded = true,
  } = options;

  const notes = await collectPulseSessionStartNotes(repoRoot, {
    syncRuntimeArtifactsIfOnboarded,
  });

  const sections = [];
  if (includeBootstrapSkill) {
    const bootstrap = buildPulseBootstrapBlock();
    if (bootstrap) {
      sections.push(bootstrap);
    }
  }
  if (notes.length > 0) {
    sections.push(`Pulse repo notes: ${notes.join(" ")}`);
  }

  return sections.join("\n\n").trim();
}
