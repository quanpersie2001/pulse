/**
 * Purpose: CLI alias for manifest-first Pulse session resume context loading.
 * Caller/flow: Invoked as `pulse.mjs session-load` by pulse:workflow use and resume-oriented automation.
 * Reads/Writes: Delegates to runtime/session-load.mjs; reads runtime state, handoffs, reservations, and workgraph pointers.
 * CLI args: --repo-root, --resume-owner, --json, --help through the delegated runtime command.
 * Ownership: Re-export/alias only; session-load behavior is owned by runtime/session-load.mjs.
 * Repo root rule: Delegates repo root resolution to runtime/session-load.mjs.
 */

import { main as runSessionLoadCommand } from "../runtime/session-load.mjs";

export * from "../runtime/session-load.mjs";
export { runSessionLoadCommand, runSessionLoadCommand as main };
