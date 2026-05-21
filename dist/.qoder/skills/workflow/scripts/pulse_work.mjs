#!/usr/bin/env node

/**
 * Purpose: Compatibility wrapper for Pulse workgraph commands and service imports.
 * Caller/flow: Prefer pulse.mjs workgraph|ready; this module preserves legacy imports.
 * Reads/Writes: Delegates to split workgraph CLI and service modules.
 * CLI args: create|show|list|ready|update|close|reopen|dep|children|graph|doctor plus --repo-root/--json.
 * Ownership: Compatibility layer only; workgraph behavior lives under ./workgraph/ and ./cli/workgraph.mjs.
 * Repo root rule: Uses shared resolver from pulse_paths.mjs via cli/workgraph.mjs.
 */

import { main as runWorkgraphCommand } from "./cli/workgraph.mjs";

export {
  WORKGRAPH_COMMANDS,
  buildDisplayItem,
  childrenOf,
  closeItem,
  createItem,
  doctor,
  getDecoratedItems,
  graph,
  listItems,
  mutateDependencies,
  readyItems,
  reopenItem,
  showItem,
  updateItem,
} from "./workgraph/service.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runWorkgraphCommand(argv, context);
}
