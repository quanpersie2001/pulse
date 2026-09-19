# Decision 0024: persist bounded run output

## Status

Accepted, 2026-09-17 (owner: quan). Amends plan 0022 §10 (runner contract)
by adding persistence, not by changing it.

## Context

Two ST-2 frictions of the same kind (≥ 2, the plan's own re-add threshold):

- **F25** — a worker broke the final-stdout-JSON contract minutes after a
  clean `pulse handoff`; the run was classified `run_inconclusive` and the
  cause was unprovable, because the runner (`src/runner/mod.rs`)
  captures stdout/stderr bounded and then **discards** it.
- **F28** — a pseudo-JSON qa step crashed `api.mjs` before it wrote any
  artifact, and the crash output vanished the same way.

Post-mortem today means re-running the role and hoping. The evidence
directory already exists per issue (`.pulse/evidence/<id>/`), is tracked
in git, and is where every other run artifact lands.

## Decision

1. **Persist on every run** — worker, worker-continue, and every lane,
   pass or fail (`run_worker` and `run_lane` in `src/kernel/run.rs`, right
   after `runner::execute` returns, before classification). Green runs are
   kept too: post-mortem needs the healthy sample next to the broken one.
2. **One file per run**: `.pulse/evidence/<id>/run-<role>-<n>.log`, `n`
   counting that role's existing run logs for the issue (+1). Append-only
   naming; never overwrite. Tracked in git like all evidence (only
   `.pulse/runtime/` and `.pulse/cache/` are ignored).
3. **Tail, bounded**: the last 32 KiB of each captured stream (stdout,
   stderr), not the whole capture (capped at `max_output_bytes`, default
   8 MiB). The interesting end of a broken run is its last lines — the
   final-JSON contract is judged there. The file opens with a short
   `key: value` header (role, exit code, timed_out/cancelled, duration)
   so a tail without a receipt is still self-describing.
4. **The stdout contract itself is untouched** — the last non-empty stdout
   line is still the one JSON object the runner parses (plan §10.1). This
   decision only adds storage after classification input is captured; it
   changes no parsing, verdict or receipt format.
5. **Runner stays domain-free**: `runner::execute` keeps returning bytes;
   the kernel layer (which knows the issue id and evidence dir) writes the
   file. A write failure surfaces as an error — evidence loss must not be
   silent.

## Consequences

- One file per role run per issue; a noisy loop adds small files (≤ ~64
  KiB each plus header). Revisit only if a target's runs are so frequent
  that the evidence dir becomes unreviewable — the same trigger as the
  receipts-per-file note in `0022-open.md`.
- The pairing cut (one new mechanism = one merge) is `work dep rm`
  deletion, landed with `pulse doctor` (plan §14 P3.2).
