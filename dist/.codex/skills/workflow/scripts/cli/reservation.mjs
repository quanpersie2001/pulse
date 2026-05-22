#!/usr/bin/env node

/**
 * Purpose: CLI facade for runtime reservation operations.
 * Caller/flow: Invoked by operators/workers to reserve, release, list, and sweep paths.
 * Reads/Writes: Reads/writes .pulse/runtime/reservations.json via reservation store.
 * CLI args: reserve|release|list|sweep with --repo-root, --agent, --item, --path, --ttl, --json.
 * Ownership: Command surface only; lock/overlap rules are owned by reservation/store.mjs.
 * Repo root rule: Uses shared resolver from core/paths.mjs.
 */

import { resolveRepoRoot } from "../core/paths.mjs";
import { assertBareBooleanOptions, assertKnownOptions, parseCliArgs as parseSharedCliArgs } from "./args.mjs";
import { normalizeIo, writePayload } from "./io.mjs";
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
} from "../reservation/store.mjs";

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

function parseArgs(argv, io = normalizeIo()) {
  if (argv.includes("--help") || argv.includes("-h")) {
    io.stdout.write(
      `${[
        "Usage:",
        "  pulse.mjs reservation reserve --repo-root <repo> --agent <name> [--item <id>] --path <glob> [--ttl <seconds>] [--note <text>] [--json]",
        "  pulse.mjs reservation release --repo-root <repo> --agent <name> [--item <id>] [--path <glob>] [--id <reservation-id>] [--json]",
        "  pulse.mjs reservation list --repo-root <repo> [--active-only] [--agent <name>] [--path <glob>] [--status active|released|expired] [--json]",
        "  pulse.mjs reservation sweep --repo-root <repo> [--json]",
      ].join("\n")}\n`,
    );
    return null;
  }

  const parsed = parseSharedCliArgs(argv);
  assertKnownOptions(parsed, [
    "repo-root",
    "agent",
    "item",
    "ttl",
    "note",
    "path",
    "paths",
    "id",
    "active-only",
    "json",
    "status",
  ]);
  assertBareBooleanOptions(parsed, ["active-only", "json"]);
  if (parsed.positionals.length > 1) {
    throw new Error(`Unknown argument: ${parsed.positionals[1]}`);
  }

  return {
    command: parsed.positionals[0] || "",
    repoRoot: parsed.string("repo-root", undefined),
    agent: parsed.string("agent"),
    itemId: parsed.string("item"),
    ttlSeconds: parsed.has("ttl") ? Number.parseInt(parsed.string("ttl"), 10) : null,
    note: parsed.string("note"),
    paths: [...parsed.list("path"), ...parsed.list("paths")],
    ids: parsed.list("id"),
    activeOnly: parsed.has("active-only"),
    json: parsed.has("json"),
    status: parsed.string("status"),
  };
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

export function main(argv = process.argv.slice(2), context = {}) {
  const io = normalizeIo(context.io);
  const args = parseArgs(argv, io);
  if (!args) {
    return 0;
  }
  const repoRoot = resolveRepoRoot({ explicitRoot: args.repoRoot, env: context.env, cwd: context.cwd });
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

  writePayload(result, { json: args.json, render: renderText, output: io.stdout });
  return 0;
}

