/**
 * Purpose: CLI facade for Pulse onboarding readiness checks and first-run apply.
 * Caller/flow: Invoked by pulse:workflow use through `pulse.mjs onboard check|apply` before session loading.
 * Reads/Writes: check reads target repo readiness; apply may back up/rebuild Pulse runtime artifacts via onboard/apply.mjs.
 * CLI args: check|apply with --repo-root, --resume-owner, --json, --help.
 * Ownership: Command surface only; onboarding decisions live under onboard/*.mjs modules.
 * Repo root rule: Uses onboarding resolver from onboard/package.mjs.
 */

import { assertBareBooleanOptions, assertKnownOptions, parseCliArgs as parseSharedCliArgs } from "./args.mjs";
import { normalizeIo, writeJson } from "./io.mjs";
import { applyRepo } from "../onboard/apply.mjs";
import { checkRepo } from "../onboard/check.mjs";
import { resolveRepoRoot } from "../onboard/package.mjs";

function parseCliArgs(argv, io = normalizeIo()) {
  if (argv.includes("--help") || argv.includes("-h")) {
    io.stdout.write(
      `${[
        "Usage: pulse.mjs onboard <check|apply> [--repo-root <path>] [--resume-owner <owner_id>] [--json]",
        "",
        "Checks or applies pulse:workflow use readiness and session loading.",
      ].join("\n")}\n`,
    );
    return null;
  }

  const parsed = parseSharedCliArgs(argv);
  assertKnownOptions(parsed, ["repo-root", "resume-owner", "json"]);
  assertBareBooleanOptions(parsed, ["json"]);

  const [command = "check", ...rest] = parsed.positionals;
  if (rest.length > 0) {
    throw new Error(`Unknown argument: ${rest[0]}`);
  }
  if (command !== "check" && command !== "apply") {
    throw new Error(`Unknown onboard command: ${command}`);
  }

  return {
    command,
    repoRoot: parsed.string("repo-root", undefined),
    resumeOwner: parsed.string("resume-owner"),
  };
}

export function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);
  const args = parseCliArgs(argv, io);
  if (!args) {
    return 0;
  }

  const repoRoot = resolveRepoRoot(args.repoRoot, context.env, context.cwd);
  const payload = args.command === "apply"
    ? applyRepo(repoRoot, { resumeOwner: args.resumeOwner })
    : checkRepo(repoRoot, { resumeOwner: args.resumeOwner });

  writeJson(payload, io.stdout);
  return payload.status === "FAIL" ? 1 : 0;
}
