#!/usr/bin/env node

/**
 * Purpose: Resolve repo root and canonical Pulse runtime file paths.
 * Caller/flow: Imported by workflow runtime helpers and CLI entrypoints.
 * Reads/Writes: Reads git top-level via `git rev-parse`; no repo file writes.
 * CLI args: None.
 * Ownership: Path resolver only; does not own state transitions or mutations.
 * Repo root rule: explicitRoot > PULSE_REPO_ROOT > git top-level > cwd.
 */

export {
  getPulsePaths,
  relativePosix,
  resolveRepoRoot,
} from "./core/paths.mjs";
