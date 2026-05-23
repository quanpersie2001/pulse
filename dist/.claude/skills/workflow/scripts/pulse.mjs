#!/usr/bin/env node

/**
 * Purpose: Public Pulse runtime CLI entrypoint shipped with the workflow skill. It exposes the rendered
 * `node .claude/skills/workflow/scripts/pulse.mjs` operator surface used by agents and humans to inspect runtime posture, find ready work,
 * coordinate worker reservations, resume interrupted sessions, onboard repositories, and maintain the canonical
 * workgraph without hand-editing runtime files.
 *
 * Help contract: `pulse.mjs help` / `pulse.mjs --help` must present a br-style operator overview:
 * a short product description, `Usage: pulse.mjs [OPTIONS] <COMMAND>`, command summaries, nested subcommand
 * summaries for command groups, global options, and practical examples. `pulse.mjs help <command>` must drill
 * into command-specific usage and subcommands. Keep this docstring aligned with `cli/router.mjs` help metadata.
 *
 * Commands routed by this entrypoint:
 * - `status [--repo-root <repo>] [--json] [--sync]`: inspect runtime posture, state mirrors, handoffs,
 *   reservations, and optional mirror sync freshness.
 * - `ready [--repo-root <repo>] [--json]`: list execution-ready work items from the runtime/workgraph view.
 * - `reservation <reserve|release|list|sweep> [options]`: coordinate worker claims.
 *   - `reserve`: claim a work item for an owner with optional TTL and note.
 *   - `release`: release an owner's claim on a work item.
 *   - `list`: list reservations, optionally only active claims.
 *   - `sweep`: expire stale reservations using current time or `--now`.
 * - `session-load [--repo-root <repo>] [--resume-owner <owner_id>] [--json]`: load safe resume context,
 *   including runtime posture, handoff pointers, reservations, and next-action hints.
 * - `onboard <check|apply> [--repo-root <repo>] [--resume-owner <owner_id>] [--json]`: verify or create
 *   required Pulse runtime/workgraph scaffolding.
 *   - `check`: validate required `.pulse` runtime/workgraph files without creating them.
 *   - `apply`: create or repair missing scaffolding, then report readiness.
 * - `workgraph <command> [options]`: maintain canonical work items and relationships under `.pulse/workgraph`
 *   and `works/`.
 *   - `create`: create an epic/story/task/bug item with optional parent, owner, priority, labels, and risks.
 *   - `show`: show one work item with metadata, hierarchy, dependencies, labels, risks, and content paths.
 *   - `list`: filter work items for planning, review, or operational triage.
 *   - `ready`: list unblocked open items whose dependencies are complete.
 *   - `update`: mutate item metadata without editing workgraph files by hand.
 *   - `close` / `reopen`: transition item completion state.
 *   - `dep`: manage blocking dependency edges that control readiness.
 *   - `link`: manage non-blocking related-item links.
 *   - `children`: list direct child items under an epic/story/task parent.
 *   - `graph`: summarize graph nodes plus hierarchy, dependency, and link edges.
 *   - `doctor`: validate workgraph integrity and optionally repair supported issues.
 * - `help [command]` or `--help`: print global or command-focused usage.
 *
 * Common options: `--repo-root <repo>` selects the repository root when supported; `--json` emits machine-readable
 * output when supported; `--sync` is status-only; `--resume-owner <id>` is for `session-load` and `onboard`.
 *
 * Caller/flow: Invoked by rendered `node .claude/skills/workflow/scripts/pulse.mjs ...` calls from `pulse:workflow` instructions,
 * operator shells, and tests. The wrapper only detects direct execution and then hands `process.argv` to
 * `cli/router.mjs`; imported usage must stay side-effect free.
 *
 * Reads/Writes: Delegates all command-specific reads/writes to `cli/router.mjs` and loaded command modules.
 * This file reads only filesystem metadata needed to compare realpaths for direct-execution detection.
 *
 * Ownership: Thin executable wrapper only; command registry, help text, option parsing, repo-root resolution,
 * runtime reads/writes, and workgraph mutation behavior are owned by `cli/router.mjs` and `cli/*.mjs` modules.
 *
 * Repo root rule: Does not resolve repo roots directly; subcommands use shared resolvers so default cwd,
 * explicit `--repo-root`, and test-provided contexts behave consistently.
 */

import fs from "node:fs";
import { fileURLToPath } from "node:url";

import { main } from "./cli/router.mjs";

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

if (isDirectExecution(import.meta.url)) {
  process.exitCode = await main();
}
