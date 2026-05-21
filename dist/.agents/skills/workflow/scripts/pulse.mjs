#!/usr/bin/env node

import { isDirectExecution } from "./cli_execution.mjs";

const COMMAND_HANDLERS = {
  status: async (argv) => {
    const { main } = await import("./cli/status.mjs");
    return main(argv);
  },
  ready: async (argv) => {
    const { main } = await import("./cli/ready.mjs");
    return main(["ready", ...argv]);
  },
  reservation: async (argv) => {
    const { main } = await import("./cli/reservation.mjs");
    return main(argv);
  },
};

function renderHelp() {
  return [
    "Usage: pulse.mjs <command> [options]",
    "",
    "Commands:",
    "  status [--repo-root <repo>] [--json] [--sync]",
    "  ready [--repo-root <repo>] [--json]",
    "  reservation <reserve|release|list|sweep> [options]",
  ].join("\n");
}

export async function main(argv = process.argv.slice(2)) {
  const [command, ...rest] = argv;

  if (!command || command === "--help" || command === "-h" || command === "help") {
    process.stdout.write(`${renderHelp()}\n`);
    return 0;
  }

  const handler = COMMAND_HANDLERS[command];
  if (!handler) {
    process.stderr.write(`Unknown command: ${command}\n${renderHelp()}\n`);
    return 1;
  }

  return handler(rest);
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
