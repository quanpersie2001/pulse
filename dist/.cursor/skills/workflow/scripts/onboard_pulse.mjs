#!/usr/bin/env node

/**
 * Purpose: Compatibility wrapper for Pulse onboarding checks and apply.
 * Caller/flow: Prefer pulse.mjs onboard check|apply; this module preserves legacy imports and direct --apply use.
 * Reads/Writes: Delegates to split onboarding services.
 * CLI args: --repo-root, --apply, --resume-owner, --help.
 * Ownership: Compatibility layer only; onboarding behavior lives under ./onboard/.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs via onboard/package.mjs.
 */

import fs from "node:fs";
import { fileURLToPath } from "node:url";

import { main as runOnboardCommand } from "./cli/onboard.mjs";
import { applyRepo as applyRepoService } from "./onboard/apply.mjs";

export {
  buildReadinessStatus,
  buildRuntimeBlockedPayload,
  buildToolingStatusOptions,
  buildToolingStatusPayload,
  getNodeRuntimeStatus,
  ONBOARDING_MARKER_PATH,
  ONBOARDING_SCHEMA_VERSION,
  readOnboardingState,
  utcNow,
  WORKFLOW_COMMAND,
  writeStateMarkdownFromTooling,
} from "./onboard/state.mjs";
export { checkRepo } from "./onboard/check.mjs";
export { loadPluginVersion, PULSE_COMMAND, resolveRepoRoot } from "./onboard/package.mjs";

export function applyRepo(repoRoot, allowCompactPromptReplaceOrOptions = {}, maybeOptions = {}) {
  const options = typeof allowCompactPromptReplaceOrOptions === "object" && allowCompactPromptReplaceOrOptions !== null
    ? allowCompactPromptReplaceOrOptions
    : maybeOptions;
  return applyRepoService(repoRoot, options);
}

function isDirectExecution(metaUrl, entryPath = process.argv[1]) {
  if (!entryPath) {
    return false;
  }
  try {
    return fs.realpathSync(fileURLToPath(metaUrl)) === fs.realpathSync(entryPath);
  } catch {
    return false;
  }
}

export function main(argv = process.argv.slice(2), context = {}) {
  if (argv.includes("--apply")) {
    return runOnboardCommand(["apply", ...argv.filter((arg) => arg !== "--apply")], context);
  }
  return runOnboardCommand(argv, context);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
