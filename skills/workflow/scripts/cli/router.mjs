/**
 * Purpose: Route pulse.mjs commands to their focused CLI modules.
 * Caller/flow: Called by pulse.mjs for status, ready, reservation, session-load, onboard, and workgraph commands.
 * Reads/Writes: Writes help and routing errors to stdout/stderr; delegated modules own runtime reads/writes.
 * CLI args: <command> [options], help [command], --help.
 * Ownership: Command registry and lazy module loading only; command behavior stays in each cli/*.mjs module.
 * Repo root rule: Does not resolve repo roots directly; subcommands use shared resolvers.
 */

import { normalizeIo } from "./io.mjs";

export const COMMANDS = {
  status: {
    usage: "status [--repo-root <repo>] [--json] [--sync]",
    summary: "Inspect Pulse runtime posture, state mirrors, handoffs, reservations, and optional sync freshness.",
    subcommands: [],
    details: [
      "Use at session start or after compaction to orient safely before opening artifacts.",
      "--sync refreshes runtime mirrors before reporting; --json emits machine-readable output.",
    ],
    load: async () => import("./status.mjs"),
  },
  ready: {
    usage: "ready [--repo-root <repo>] [--json]",
    summary: "Show work items that are unblocked and ready for execution from the runtime/workgraph view.",
    subcommands: [],
    details: [
      "Use before reserving work or choosing a single-worker/swarm execution slice.",
      "This is the lightweight operator read; deeper workgraph filtering lives under `workgraph list`.",
    ],
    load: async () => import("./ready.mjs"),
  },
  reservation: {
    usage: "reservation <reserve|release|list|sweep> [options]",
    summary: "Coordinate worker claims on work items and clean up stale reservations.",
    subcommands: [
      ["reserve", "Claim a work item for an owner with optional TTL and note."],
      ["release", "Release an owner's claim on a work item."],
      ["list", "List reservations, optionally only active claims."],
      ["sweep", "Expire stale reservations using current time or --now."],
    ],
    details: [
      "reserve --item <id> --owner <owner> [--ttl-ms <ms>] [--note <text>]",
      "release --item <id> --owner <owner>",
      "list [--active-only]",
      "sweep [--now <iso>]",
      "Common options include --repo-root <repo> and --json.",
    ],
    load: async () => import("./reservation.mjs"),
  },
  "session-load": {
    usage: "session-load [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    summary: "Load the safe resume packet: runtime posture, handoff pointers, reservations, and next actions.",
    subcommands: [],
    details: [
      "Use when resuming an interrupted Pulse session before touching work artifacts.",
      "--resume-owner scopes resume hints to a specific worker/owner when reservations exist.",
    ],
    load: async () => import("./session-load.mjs"),
  },
  onboard: {
    usage: "onboard <check|apply> [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    summary: "Check or create required Pulse runtime/workgraph scaffolding for a repository.",
    subcommands: [
      ["check", "Validate required .pulse runtime/workgraph files without creating them."],
      ["apply", "Create or repair missing scaffolding, then report readiness."],
    ],
    details: [
      "check",
      "apply",
      "Use --resume-owner to preserve/inspect owner-specific resume context during onboarding.",
    ],
    load: async () => import("./onboard.mjs"),
  },
  workgraph: {
    usage: "workgraph <command> [options]",
    summary: "Maintain canonical work items and relationships under .pulse/workgraph and works/.",
    subcommands: [
      ["create", "Create an epic/story/task/bug item with optional parent, owner, priority, labels, and risks."],
      ["show", "Show one work item with metadata, hierarchy, dependencies, labels, risks, and content paths."],
      ["list", "Filter work items for planning, review, or operational triage."],
      ["ready", "List unblocked open items whose dependencies are complete."],
      ["update", "Mutate item metadata without editing workgraph files by hand."],
      ["close", "Mark an item complete/closed after verification artifacts are in place."],
      ["reopen", "Return a closed item to active status when follow-up work is required."],
      ["dep", "Manage blocking dependency edges that control readiness."],
      ["link", "Manage non-blocking related-item links."],
      ["children", "List direct child items under an epic/story/task parent."],
      ["graph", "Summarize graph nodes plus hierarchy, dependency, and link edges."],
      ["doctor", "Validate workgraph integrity and optionally repair supported issues."],
    ],
    details: [
      "create --kind <kind> --title <title> [--parent <id>] [--owner <owner>] [--priority <n>] [--label <label>] [--risk <flag>]",
      "show <id>",
      "list [--kind <kind>] [--status <status>] [--epic <id>] [--parent <id>] [--owner <owner>] [--label <label>]",
      "ready",
      "update <id> [--title <title>] [--slug <slug>] [--status <status>] [--priority <n>] [--owner <owner>] [--clear-owner]",
      "            [--blocked-reason <text>] [--clear-blocked-reason] [--add-label <label>] [--rm-label <label>] [--add-risk <flag>] [--rm-risk <flag>]",
      "close <id>",
      "reopen <id>",
      "dep add <id> <depends-on> | dep rm <id> <depends-on>",
      "link add <id> <linked-item> | link rm <id> <linked-item>",
      "children <id>",
      "graph",
      "doctor [--fix]",
      "Common options include --repo-root <repo> and --json.",
    ],
    load: async () => import("./workgraph.mjs"),
  },
};

function renderCommandLine(name, entry) {
  return `  ${name.padEnd(13)} ${entry.summary}`;
}

function renderSubcommandLine([name, summary]) {
  return `    ${name.padEnd(11)} ${summary}`;
}

function renderCommandHelp(name, entry) {
  const lines = [
    "Pulse runtime workflow CLI",
    "",
    `Usage: pulse.mjs ${entry.usage}`,
    "",
    "Description:",
    `  ${entry.summary}`,
  ];

  if (entry.subcommands.length > 0) {
    lines.push("", "Subcommands:", ...entry.subcommands.map(renderSubcommandLine));
  }

  lines.push(
    "",
    "Command usage:",
    ...entry.details.map((line) => (line ? `  ${line}` : "")),
    "",
    "Options:",
    "      --repo-root <repo>      Repository root; defaults to current working directory when supported",
    "      --json                  Emit machine-readable JSON when supported",
    "  -h, --help                  Print help",
  );

  return lines.join("\n");
}

export function renderHelp(commandName = "") {
  const command = COMMANDS[commandName];
  if (command) {
    return renderCommandHelp(commandName, command);
  }

  return [
    "Pulse runtime workflow CLI",
    "",
    "Usage: pulse.mjs [OPTIONS] <COMMAND>",
    "",
    "Commands:",
    ...Object.entries(COMMANDS).map(([name, entry]) => renderCommandLine(name, entry)),
    "  help          Print this message or detailed help for a command",
    "",
    "Subcommands:",
    "  reservation",
    ...COMMANDS.reservation.subcommands.map(renderSubcommandLine),
    "  onboard",
    ...COMMANDS.onboard.subcommands.map(renderSubcommandLine),
    "  workgraph",
    ...COMMANDS.workgraph.subcommands.map(renderSubcommandLine),
    "",
    "Options:",
    "      --repo-root <repo>      Repository root; defaults to current working directory when supported",
    "      --json                  Emit machine-readable JSON when supported",
    "      --sync                  Refresh runtime mirrors before status output (`status` only)",
    "      --resume-owner <id>     Scope resume/onboarding context to one owner (`session-load`, `onboard`)",
    "  -h, --help                  Print help",
    "",
    "Examples:",
    "  pulse.mjs status --repo-root . --json",
    "  pulse.mjs reservation reserve --repo-root . --item task-123 --owner worker-a",
    "  pulse.mjs reservation list --repo-root . --active-only --json",
    "  pulse.mjs workgraph create --repo-root . --kind task --title \"Add validation\" --parent story-1",
    "  pulse.mjs workgraph dep add task-2 task-1 --repo-root .",
    "  pulse.mjs help workgraph",
  ].join("\n");
}

export async function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);
  const [command, ...rest] = argv;

  if (!command || command === "--help" || command === "-h") {
    io.stdout.write(`${renderHelp()}\n`);
    return 0;
  }

  if (command === "help") {
    io.stdout.write(`${renderHelp(rest[0])}\n`);
    return 0;
  }

  const entry = COMMANDS[command];
  if (!entry) {
    io.stderr.write(`Unknown command: ${command}\n${renderHelp()}\n`);
    return 1;
  }

  const { main: runCommand } = await entry.load();
  return runCommand(rest, { ...context, io });
}
