#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  readDependencyHealthSafe,
  normalizeDependencyTarget,
  uniqueSorted,
} from "./pulse_dependencies.mjs";
import { readGitNexusReadiness, syncPulseRuntimeArtifacts } from "./pulse_state.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const SCRIPT_DIR = path.dirname(SCRIPT_PATH);

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

export function buildPulseSessionDependencyWarning(repoRoot) {
  const dependencyHealth = readDependencyHealthSafe(repoRoot);

  const missingDependencies = Array.isArray(dependencyHealth?.missing_dependencies)
    ? dependencyHealth.missing_dependencies
    : [];
  if (missingDependencies.length === 0) {
    return "";
  }

  const affectedSkills = uniqueSorted(
    missingDependencies.flatMap((dependency) =>
      Array.isArray(dependency.required_by) ? dependency.required_by : [],
    ),
  );
  const missingCommands = uniqueSorted(
    missingDependencies
      .filter((dependency) => dependency.kind === "command")
      .flatMap((dependency) => normalizeDependencyTarget(dependency.target)),
  );
  const missingMcpServers = uniqueSorted(
    missingDependencies
      .filter((dependency) => dependency.kind === "mcp_server")
      .flatMap((dependency) => normalizeDependencyTarget(dependency.target)),
  );

  const affected = affectedSkills.length > 0 ? affectedSkills.join(", ") : "(unknown skills)";
  const commands = missingCommands.length > 0 ? missingCommands.join(", ") : "none";
  const mcpServers = missingMcpServers.length > 0 ? missingMcpServers.join(", ") : "none";

  return (
    `Dependency warning: ${missingDependencies.length} declared dependencies are missing, ` +
    `so some Pulse skills are degraded or unavailable. ` +
    `Affected skills: ${affected}. ` +
    `Missing commands: ${commands}. ` +
    `Missing MCP server configuration: ${mcpServers}.`
  );
}

function readPulseSkillText() {
  const candidates = [
    process.env.CLAUDE_PLUGIN_ROOT
      ? path.join(process.env.CLAUDE_PLUGIN_ROOT, "skills", "pulse", "SKILL.md")
      : "",
    path.resolve(SCRIPT_DIR, "..", "..", "SKILL.md"),
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return fs.readFileSync(candidate, "utf8").trim();
    }
  }

  return "";
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
    "Below is the full content of your `pulse:workflow onboard` bootstrap skill. Use it to route safely before loading downstream Pulse skills:",
    "",
    skillText,
    "</EXTREMELY_IMPORTANT>",
  ].join("\n");
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
      "Pulse onboarding is installed for this repo. Read AGENTS.md, then run node .pulse/scripts/pulse_status.mjs --json for a quick scout before substantive work.",
    );
  } else {
    notes.push("Onboarding readiness has not been established for this repo. Run pulse:workflow onboard before continuing.");
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

  const dependencyWarning = buildPulseSessionDependencyWarning(repoRoot);
  if (dependencyWarning) {
    notes.push(dependencyWarning);
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
