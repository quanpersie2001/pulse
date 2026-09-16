# Plan 0022 metrics

Measured per §2 of `docs/plans/0022-thin-harness.md`. Re-measure at the end
of every phase; add a column, don't overwrite the previous one.

## How each row is measured

```bash
find src -name '*.rs' | xargs cat | wc -l
# leaf CLI commands: recurse `pulse <path> --help` until a subcommand's
# --help has no "Commands:" section; count those leaves (see history for the
# exact recursive script — bash only, zsh does not word-split unquoted args).
```

Distinct error codes (from P1.12 onward): no exact prior script survives
("Python-counted" was never checked in), so this session defines one and
sticks to it — every string literal that is the first argument to
`PulseError::kernel(...)`, `PulseError::validation(...)` or the
`violation(...)` gate-report helpers in `kernel::ready`/`kernel::completion`
(a "narrow" count that excludes the latter — codes that only ever surface
folded into one `gate_failed` message — is 63; the wide count including
them, used in this table, is 85), plus `error.rs`'s fixed non-`Kernel`/
`Validation` variant codes. A future session should keep using this same
definition rather than re-deriving one, so the trend line stays comparable.

The last three rows (hand-typed commands to close a Ticket, required flags
on that path, friction-per-Ticket that is a Pulse bug) are not
mechanically measured; they are carried from the plan's own baseline
estimate and re-judged qualitatively once the dogfood target exists in
Phase 2.

## Table

| Metric | Baseline 2026-09-16 | After Phase 0 | After P1.3+P1.4+P1.5 | After Phase 1 (P1.11, 2026-09-16) | After P1.12 (2026-09-16) | After Phase 2A (2026-09-16) | Target v3.0 |
|---|---|---|---|---|---|---|---|
| Rust lines in `src/` | 43106 | 43106 | 6517 | 10422 | 10599 | 13089 | < 10000 |
| Distinct error codes (wide count, see above) | 271 (grep-pattern count, not directly comparable) | 271 | 31 | 64 (methodology undocumented) | 85 | 93 | < 40 |
| CLI leaf commands | 67 (measured; plan estimated ~60) | 67 | 13 | 20 | 19 | 27 | ≤ 22 |
| Hand-typed commands to close one Ticket | ~11 (plan estimate) | ~11 | n/a | 5 (`new`, `ready`, `run worker`, `run review`, `close`) | 5 (unchanged; now proven end-to-end through the real `pulse` binary by `tests/golden_path.rs`, not just fixture-fake-agent unit tests) | 5 (unchanged — `learn`/`docs` are enrichments a worker/reviewer can use, not new required steps on this path) | ≤ 6 |
| Required flags on that path | ~25 (plan estimate) | ~25 | n/a | ~2 (`--risk`, `--surface` on `new`; actor defaults from git config) | ~2 (unchanged) | ~2 (unchanged) | ≤ 4 |
| Friction/Ticket that is a Pulse bug | Track B: majority | Track B: majority (no new dogfood yet) | (same) | (same — no new dogfood yet; Phase 2) | (same — no Phase 2 dogfood yet; but `tests/golden_path.rs`, the first true end-to-end CLI run, found exactly one real Pulse bug on its first pass — `run_lane` blocking every story-scope qa lane with `lane_not_verifying` — fixed the same session, F2 prereq of P1.12) | (same — still no Phase 2 dogfood; this session's own smoke tests of `docs check`, `learn`, `init --with-qa-templates` found no new Pulse bugs, only the two P1.12 review carryovers A1 was scoped to fix) | < 1 |
| Repos running Pulse for real | 0 | 0 | 0 | 0 | 0 (the golden path repo is a throwaway temp dir per test run, not a persistent dogfood target) | 0 (dogfood target still doesn't exist; that's Phase 2B, P2.2 onward) | 1 (UI + API) |

Baseline test suite (`cargo test --all-targets`) at the Phase 0 commit: 12
test binaries, 624 tests, 0 failures.

After P1.3+P1.4+P1.5 (commit `a2d164a`): 8 test binaries, 130 tests, 0
failures — deletion landed in one commit instead of spread across P1.3-P1.5
(see that commit's message), so `src/` was transiently below both the
Phase 1 and v3.0 targets before P1.6-P1.10 added back `evidence::receipt`,
`kernel::{completion,packet,checkpoint,profile,lane,reservation,run}` and
`kernel::init`'s full asset set.

**After Phase 1 (P1.11, commits through the error-code consolidation just
before this one):** 9 test binaries, 193 tests, 0 failures
(`cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` /
`cargo test --all-targets` all green). `src/` = 10422 lines, under the
Phase 1 gate (< 12000) but over the v3.0 target (< 10000) by 422 lines —
expected: P2.1 (`learn/*`, `docs::{applicable,check}`) and P3 (`board`,
`doctor`) still land. 64 error codes is over both the Phase 1 gate (< 60,
by 4) and the v3.0 target (< 40); of the 64, 21 are pre-existing
`storage`/`error.rs`/`runner::` codes this plan's file-fate table marks
"giữ nguyên" (`io_error`, `json_error`, `cas_conflict`, `runner_spec_invalid`,
etc.) and were never in scope to remove — the ~43 codes v3 actually
introduced are already under the final target on their own. A P1.11 pass
merged 4 near-duplicate new codes (`checkpoint_input_invalid` +
`handoff_input_invalid` -> `from_file_invalid`; `lane_output_missing` ->
`lane_output_invalid`; `ready_gate_failed` -> `gate_failed`) before this
count; further squeezing the new codes down risked losing which condition
failed for marginal budget gain, so this was not pursued further this
session. 20 CLI leaves and a 5-command/~2-flag golden-path-so-far (`new` ->
`ready` -> `run worker` -> `run review` -> `close`; a human never touches
`checkpoint`/`handoff`, the worker calls those itself) both already meet
the v3.0 target, though only against the fixture-fake-agent tests in this
repo — no dogfood target exists yet to confirm it end-to-end (Phase 2).

**After P1.12 (this session's review fix-up pass):** 10 test binaries, 196
tests, 0 failures (`cargo fmt --check` / `cargo clippy --all-targets -- -D
warnings` / `cargo test --all-targets` all green). `src/` = 10599 lines,
+177 from P1.11's 10422: F1 added `kernel::lane::lane_input` plus three
tests (+255), the story-scope `run_lane` fix added a few more (+5), F3's
deletion of the v2 `pulse events compact` machinery and stale assets cut
it back down (-112), and F5/F6's hint/env-parameter work added it back
(+27, +2) — still 599 over the v3.0 target, expected until P2.1/P3 land.
The error-code row jumped from
64 to 85 not because this session added ~21 codes (it added exactly one,
`lane_not_verifying`, F1) but because the "Python-counted" script behind
64 was never checked in — this session defines and documents a concrete
replacement (see "How each row is measured" above) that, unlike the
plan's own header-comment grep (suffix-matched on `_missing`/`_stale`/
etc.), also counts codes like `gate_failed`, `role_forbidden` and
`dep_cycle` that don't end in one of those suffixes. Treat 85 as the new
baseline for this definition, not as a 21-code regression. 19 CLI leaves
(-1: `pulse events compact`, F3) is under target. The golden-path/
required-flags rows are unchanged in shape but, for the first time,
backed by a real end-to-end run of the actual `pulse` binary
(`tests/golden_path.rs`, F2) rather than only fixture-fake-agent unit
tests — that run surfaced one genuine Pulse bug (`run_lane` refusing
every story-scope qa lane with `lane_not_verifying`, since a Story never
reaches `verifying`), fixed in the commit immediately before the test.

**After Phase 2A (this session — A1-A6, commits `dd445ed`..`297a893`):**
10 test binaries, 250 tests, 0 failures (`cargo fmt --check` /
`cargo clippy --all-targets -- -D warnings` / `cargo test --all-targets` all
green throughout — every commit landed with a clean validation run before
it, per plan §14's rule). `src/` grew from 10599 to 13089 (+2490): A2
(`learn/*` + wiring, +1355), A3 (`docs/*` + wiring, +804), A4 (prompt
assets are markdown, not `.rs`, but `runners_json_seed`/`ensure_prompts`
and their tests in `kernel::init`, +154), A5 (QA template copying +
`kernel::init` wiring/tests, +148), A6 (seed content only, +17); A1 was a
net +12 (a profile-check refactor plus one new test). This is exactly what
Phase 1's own "After Phase 1" note predicted: "P2.1 (`learn/*`,
`docs::{applicable,check}`) and P3 (`board`, `doctor`) still land" — both
the `src/` (13089, over the < 10000 target by 3089) and error-code (93,
over the < 40 target by 53) rows are now *further* from the v3.0 target
than after P1.12, not closer, because Phase 2A's job was to add the
remaining plan-mandated surface, not shrink it; Phase 3 (`board`,
`doctor`, then the actual cut-or-keep decisions) is where those two rows
turn around. CLI leaves grew from 19 to 27 (+8: `learn`'s 6 leaves —
`add`/`list`/`show`/`applicable`/`activate`/`retire` — plus `docs`'s 2 —
`applicable`/`check`), already 5 over the ≤ 22 target for the same reason.
The golden-path/required-flags rows are unchanged in shape and value — A1
removed a `--force` a Story-scope lane run needed, but that was never on
the Ticket-closing path golden_path.rs measures, so this row didn't move.
No new dogfood target exists yet (Phase 2B, P2.2 onward), so the last two
rows are unchanged from P1.12 other than confirming no Pulse bug turned up
in this session's own manual smoke tests of `docs check`, `learn add`,
and `init --with-qa-templates` (see each commit's message for the exact
commands run).
