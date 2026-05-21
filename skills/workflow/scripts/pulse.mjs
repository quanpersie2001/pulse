#!/usr/bin/env node

import fs from "node:fs";
import { fileURLToPath } from "node:url";

import { normalizeIo } from "./cli/io.mjs";

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

const COMMANDS = {
  status: {
    usage: "status [--repo-root <repo>] [--json] [--sync]",
    load: async () => import("./cli/status.mjs"),
  },
  ready: {
    usage: "ready [--repo-root <repo>] [--json]",
    load: async () => import("./cli/ready.mjs"),
  },
  reservation: {
    usage: "reservation <reserve|release|list|sweep> [options]",
    load: async () => import("./cli/reservation.mjs"),
  },
  "session-load": {
    usage: "session-load [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    load: async () => import("./cli/session-load.mjs"),
  },
  onboard: {
    usage: "onboard <check|apply> [--repo-root <repo>] [--resume-owner <owner_id>] [--json]",
    load: async () => import("./cli/onboard.mjs"),
  },
  workgraph: {
    usage: "workgraph <create|show|list|ready|update|close|reopen|dep|children|graph|doctor> [options]",
    load: async () => import("./cli/workgraph.mjs"),
  },
};

function renderHelp(commandName = "") {
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

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
