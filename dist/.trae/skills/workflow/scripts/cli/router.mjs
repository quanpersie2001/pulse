import { normalizeIo } from "./io.mjs";

export const COMMANDS = {
  status: {
    usage: "status [--repo-root <repo>] [--json] [--sync]",
    load: async () => import("./status.mjs"),
  },
  ready: {
    usage: "ready [--repo-root <repo>] [--json]",
    load: async () => import("./ready.mjs"),
  },
  reservation: {
    usage: "reservation <reserve|release|list|sweep> [options]",
    load: async () => import("./reservation.mjs"),
  },
  "session-load": {
    usage: "session-load [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    load: async () => import("./session-load.mjs"),
  },
  onboard: {
    usage: "onboard <check|apply> [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    load: async () => import("./onboard.mjs"),
  },
  workgraph: {
    usage: "workgraph <create|show|list|ready|update|close|reopen|dep|link|children|graph|doctor> [options]",
    load: async () => import("./workgraph.mjs"),
  },
};

export function renderHelp(commandName = "") {
  const command = COMMANDS[commandName];
  if (command) {
    return ["Usage: pulse.mjs <command> [options]", "", "Command:", `  ${command.usage}`].join("\n");
  }

  return [
    "Usage: pulse.mjs <command> [options]",
    "",
    "Commands:",
    ...Object.values(COMMANDS).map((entry) => `  ${entry.usage}`),
    "  help [command]",
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
