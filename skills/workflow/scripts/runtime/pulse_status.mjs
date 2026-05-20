#!/usr/bin/env node

import path from "node:path";
import { fileURLToPath } from "node:url";

import { syncPulseRuntimeArtifacts } from "./pulse_state.mjs";
import { readPulseStatus } from "./pulse_status_model.mjs";
import { renderPulseStatus } from "./pulse_status_render.mjs";
import { resolveRepoRoot } from "./pulse_paths.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));

function parseCliArgs(argv) {
  const args = {
    repoRoot: undefined,
    json: false,
    command: "status",
    sync: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--repo-root") {
      args.repoRoot = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith("--repo-root=")) {
      args.repoRoot = arg.slice("--repo-root=".length);
      continue;
    }
    if (arg === "--json") {
      args.json = true;
      continue;
    }
    if (arg === "--sync") {
      args.sync = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage:",
          "  pulse_status.mjs [--repo-root <path>] [--json] [--sync]",
          "",
          "Shows a non-mutating Pulse status snapshot.",
          "Use --sync to refresh persisted runtime artifacts before rendering status.",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseCliArgs(argv);
  const repoRoot = resolveRepoRoot(args.repoRoot);


  if (args.sync) {
    syncPulseRuntimeArtifacts(repoRoot);
  }

  const status = await readPulseStatus(repoRoot);
  process.stdout.write(
    args.json ? `${JSON.stringify(status, null, 2)}\n` : `${renderPulseStatus(status)}\n`,
  );
  return 0;
}

if (process.argv[1]) {
  process.exitCode = await main();
}
