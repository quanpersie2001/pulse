/**
 * Purpose: Compatibility command that lists ready workgraph items.
 * Caller/flow: Invoked as `pulse.mjs ready` by operators or older automation expecting a top-level ready command.
 * Reads/Writes: Delegates to workgraph ready; reads workgraph items and writes stdout only.
 * CLI args: --repo-root, --json, --help through the delegated workgraph command.
 * Ownership: Thin alias only; readiness filtering is owned by cli/workgraph.mjs and workgraph/service.mjs.
 * Repo root rule: Delegates repo root resolution to cli/workgraph.mjs.
 */

import { main as runWorkgraphCommand } from "./workgraph.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runWorkgraphCommand(["ready", ...argv], context);
}
