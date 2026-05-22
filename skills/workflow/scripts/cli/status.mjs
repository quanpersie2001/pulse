#!/usr/bin/env node

/**
 * Purpose: Render Pulse scout status as JSON or text.
 * Caller/flow: Invoked by operators and onboarding/status checks.
 * Reads/Writes: Reads runtime files via status model; writes stdout only.
 * CLI args: --repo-root, --json, --sync, --help.
 * Ownership: Read surface only; status model derives runtime records in memory.
 * Repo root rule: Uses shared resolver from core/paths.mjs.
 */

import { readPulseStatus } from "../runtime/read-model.mjs";
import { renderPulseStatus } from "../runtime/render.mjs";
import { resolveRepoRoot } from "../core/paths.mjs";
import { assertBareBooleanOptions, assertKnownOptions, parseCliArgs as parseSharedCliArgs } from "./args.mjs";
import { normalizeIo, writePayload } from "./io.mjs";

function parseCliArgs(argv, io = normalizeIo()) {
  if (argv.includes("--help") || argv.includes("-h")) {
    io.stdout.write(
      `${[
        "Usage:",
        "  pulse.mjs status [--repo-root <path>] [--json] [--sync]",
        "",
        "Shows a non-mutating Pulse status snapshot.",
        "The --sync flag is accepted for existing automation; status is derived in memory.",
      ].join("\n")}\n`,
    );
    return null;
  }

  const parsed = parseSharedCliArgs(argv);
  assertKnownOptions(parsed, ["repo-root", "json", "sync"]);
  assertBareBooleanOptions(parsed, ["json", "sync"]);
  if (parsed.positionals.length > 0) {
    throw new Error(`Unknown argument: ${parsed.positionals[0]}`);
  }

  return {
    repoRoot: parsed.string("repo-root", undefined),
    json: parsed.has("json"),
    command: "status",
    sync: parsed.has("sync"),
  };
}

export async function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);
  const args = parseCliArgs(argv, io);
  if (!args) {
    return 0;
  }
  const repoRoot = resolveRepoRoot({ explicitRoot: args.repoRoot, env: context.env, cwd: context.cwd });

  const status = await readPulseStatus(repoRoot);
  writePayload(status, { json: args.json, render: renderPulseStatus, output: io.stdout });
  return 0;
}

