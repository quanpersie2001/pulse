# Pulse evals (plan 0026 — G3 of plan 0025)

Paired `claude -p` runs that measure whether a skill/prompt text changes
agent behavior in the direction the harness wants. Grading is **mechanical**
— assertions over `.pulse/issues.jsonl`, receipts and the session
transcript — never an LLM judge. Chosen straight from the 0025 friction
table (see `docs/plans/0026-eval-skills.md`):

| Eval | Surface | Ground truth |
|---|---|---|
| E1 `plan-cut` | `skills/pulse-plan` | cut satisfies: ready gate, `verify[]` present, single `docs_to_update` owner holding the doc in `touches`, prerequisite edge wired |
| E2 `worker-handoff` | `templates/prompts/worker.md` | handoff receipt sealed, no scratch payload at the repo root, no refusal codes in the transcript |
| E3 `seat-budget` | `templates/prompts/review-correctness.md` | seat terminates within budget (turns ≤ 30, ≤ 3 seal attempts) and its outcome is recorded (sealed receipt or a reported refusal) |

## Verified runner contract

`claude 2.1.276`, verified 2026-09-19 by smoke run (this is the
"verify-when-implementing, never invent" rule from G1/plan 0025):

```
claude -p "<prompt>" --output-format json --no-session-persistence \
  --permission-mode bypassPermissions
```

- stdout is one JSON object: `result` (final text), `num_turns` (the E3
  budget metric — one tool action per turn), `usage`, `total_cost_usd`,
  `is_error`, `subtype`.
- No `--max-turns` flag has been verified; the runner instead enforces a
  20-minute wall-clock timeout (SIGKILL) and a killed run grades as failed.
- Re-verify after any claude CLI upgrade; the flags come from
  `references/repo-harness/evals/benchmark.md` plus the local smoke.

## Usage

```
node evals/run.mjs E3 without 1     # cheapest first: runner shakedown
node evals/run.mjs E3 with 3
node evals/run.mjs E1 with 3
node evals/run.mjs grade evals/.run/E3-with-01
```

Workspaces are copies of `tests/fixtures/target-repos/minimal-service`
under `evals/.run/` (gitignored); the fixture itself is never used in
place. Setup drives all state through the real `pulse` CLI — the E3 lane
input is gate-produced, never hand-written. The fixture's old free-form
`PULSE.md` gets a `profiles:` block appended in the workspace copy only.

## Bar (from plan 0026)

Skill "works": arm `with` passes ≥ 2/3 runs and arm `without` < 2/3, same
fixture, same prompt. Results are rates, never single-run absolutes; a
blurry 2/3-vs-1/3 outcome means run more n, not conclude. Full results go
to `evals/results/<date>.md` — an eval without recorded numbers is not
done.

## Budget

~30–60k tokens per run; a full sweep (3 evals × 2 arms × 3 runs) is
~0.5–1M tokens. Run per-eval; E3 is the cheapest shakedown.
