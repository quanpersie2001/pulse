#!/usr/bin/env node

/**
 * Purpose: CLI facade for runtime reservation operations.
 * Caller/flow: Invoked by operators/workers to reserve, release, list, and sweep paths.
 * Reads/Writes: Reads/writes .pulse/runtime/reservations.json via reservation store.
 * CLI args: reserve|release|list|sweep with --repo-root, --agent, --item, --path, --ttl, --json.
 * Ownership: Command surface only; lock/overlap rules are owned by pulse_reservation_store.mjs.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs.
 */

import { resolveRepoRoot } from "./pulse_paths.mjs";
import { isDirectExecution } from "./cli_execution.mjs";
import {
  RESERVATION_SCHEMA_VERSION,
  ensureReservationStore,
  findReservationConflicts,
  listReservations,
  normalizeReservationPattern,
  readReservationStore,
  releaseReservations,
  reservationPatternsOverlap,
  reservePaths,
  summarizeReservationStatus,
  sweepExpiredReservations,
} from "./pulse_reservation_store.mjs";

export {
  RESERVATION_SCHEMA_VERSION,
  ensureReservationStore,
  findReservationConflicts,
  listReservations,
  normalizeReservationPattern,
  readReservationStore,
  releaseReservations,
  reservationPatternsOverlap,
  reservePaths,
  summarizeReservationStatus,
  sweepExpiredReservations,
};

function parseArgs(argv) {
  const args = {
    command: "",
    repoRoot: undefined,
    agent: "",
    itemId: "",
    ttlSeconds: null,
    note: "",
    paths: [],
    ids: [],
    activeOnly: false,
    json: false,
    status: "",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (!args.command && !arg.startsWith("--")) {
      args.command = arg;
      continue;
    }
    if (arg === "--repo-root") {
      args.repoRoot = argv[index + 1];
      index += 1;
      continue;
    }
    if (arg.startsWith("--repo-root=")) {
      args.repoRoot = arg.slice("--repo-root=".length);
      continue;
    }
    if (arg === "--agent") {
      args.agent = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--agent=")) {
      args.agent = arg.slice("--agent=".length);
      continue;
    }
    if (arg === "--item") {
      args.itemId = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--item=")) {
      args.itemId = arg.slice("--item=".length);
      continue;
    }
    if (arg === "--ttl") {
      args.ttlSeconds = Number.parseInt(argv[index + 1] || "", 10);
      index += 1;
      continue;
    }
    if (arg.startsWith("--ttl=")) {
      args.ttlSeconds = Number.parseInt(arg.slice("--ttl=".length), 10);
      continue;
    }
    if (arg === "--note") {
      args.note = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--note=")) {
      args.note = arg.slice("--note=".length);
      continue;
    }
    if (arg === "--path" || arg === "--paths") {
      args.paths.push(argv[index + 1] || "");
      index += 1;
      continue;
    }
    if (arg.startsWith("--path=") || arg.startsWith("--paths=")) {
      args.paths.push(arg.split("=")[1] || "");
      continue;
    }
    if (arg === "--id") {
      args.ids.push(argv[index + 1] || "");
      index += 1;
      continue;
    }
    if (arg.startsWith("--id=")) {
      args.ids.push(arg.slice("--id=".length));
      continue;
    }
    if (arg === "--active-only") {
      args.activeOnly = true;
      continue;
    }
    if (arg === "--json") {
      args.json = true;
      continue;
    }
    if (arg === "--status") {
      args.status = argv[index + 1] || "";
      index += 1;
      continue;
    }
    if (arg.startsWith("--status=")) {
      args.status = arg.slice("--status=".length);
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      process.stdout.write(
        [
          "Usage:",
          "  node pulse_reservations.mjs --repo-root <repo> reserve --agent <name> [--item <id>] --path <glob> [--ttl <seconds>] [--note <text>] [--json]",
          "  node pulse_reservations.mjs --repo-root <repo> release --agent <name> [--item <id>] [--path <glob>] [--id <reservation-id>] [--json]",
          "  node pulse_reservations.mjs --repo-root <repo> list [--active-only] [--agent <name>] [--path <glob>] [--status active|released|expired] [--json]",
          "  node pulse_reservations.mjs --repo-root <repo> sweep [--json]",
        ].join("\n"),
      );
      process.exit(0);
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  return args;
}

function renderText(result) {
  if (result.reservation) {
    return [
      `Reservation created: ${result.reservation.id}`,
      `Agent: ${result.reservation.agent}`,
      `Item: ${result.reservation.item_id || "(none)"}`,
      `Paths: ${result.reservation.paths.join(", ")}`,
      `TTL seconds: ${result.reservation.ttl_seconds ?? "none"}`,
      `Status: ${result.reservation.status}`,
    ].join("\n");
  }

  if (Array.isArray(result.conflicts)) {
    if (result.conflicts.length === 0) {
      return "No reservation conflicts.";
    }
    return [
      "Reservation conflicts:",
      ...result.conflicts.map(
        (conflict) =>
          `- ${conflict.agent} holds ${conflict.paths.join(", ")} (${conflict.id})`,
      ),
    ].join("\n");
  }

  if (Array.isArray(result.reservations)) {
    if (result.reservations.length === 0) {
      return "No reservations found.";
    }
    return [
      "Reservations:",
      ...result.reservations.map(
        (reservation) =>
          `- ${reservation.id} | ${reservation.status} | ${reservation.agent} | ${reservation.paths.join(", ")}`,
      ),
    ].join("\n");
  }

  if (typeof result.released_count === "number") {
    return `Released ${result.released_count} reservation(s).`;
  }

  if (typeof result.swept_count === "number") {
    return `Swept ${result.swept_count} expired reservation(s).`;
  }

  return JSON.stringify(result, null, 2);
}

export function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const repoRoot = resolveRepoRoot({ explicitRoot: args.repoRoot });
  let result;

  switch (args.command) {
    case "reserve":
      result = reservePaths(repoRoot, {
        agent: args.agent,
        itemId: args.itemId,
        paths: args.paths,
        ttlSeconds: args.ttlSeconds,
        note: args.note,
      });
      break;
    case "release":
      result = releaseReservations(repoRoot, {
        agent: args.agent,
        itemId: args.itemId,
        paths: args.paths.map((item) => normalizeReservationPattern(repoRoot, item)),
        ids: args.ids,
      });
      break;
    case "list":
      result = listReservations(repoRoot, {
        activeOnly: args.activeOnly,
        agent: args.agent || undefined,
        itemId: args.itemId || undefined,
        paths: args.paths.map((item) => normalizeReservationPattern(repoRoot, item)),
        status: args.status || undefined,
      });
      break;
    case "sweep":
      result = sweepExpiredReservations(repoRoot);
      break;
    default:
      throw new Error(
        `Unknown command: ${args.command || "(missing)"}. Use reserve, release, list, or sweep.`,
      );
  }

  process.stdout.write(args.json ? `${JSON.stringify(result, null, 2)}\n` : `${renderText(result)}\n`);
  return 0;
}

if (isDirectExecution(import.meta.url)) {
  process.exitCode = main();
}
