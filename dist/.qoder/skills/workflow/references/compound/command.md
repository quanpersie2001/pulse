# `pulse:workflow compound`

Post-cycle learning pass that turns completed Pulse work into reusable memory for future planning and execution.

Compound answers:

> What durable, reusable knowledge emerged from this completed work slice, and where should it live so future planning and execution benefit?

Compound is the canonical post-cycle learning extraction. It produces machine-readable learnings, corrections, and ratchets. It does not consolidate daemon posture, repair artifacts, or re-execute work.

## Mission

Extract, classify, and propagate durable knowledge from a completed work cycle so that future planning and execution can reuse proven patterns, avoid repeated mistakes, and honor earned non-regression rules.

## Entry criteria

Run `pulse:workflow compound` when:

- `pulse:workflow review` has completed and the outcome is known
- the completed work produced non-obvious reusable lessons, or the user explicitly requests a learning pass
- work artifacts, verification evidence, and review findings are available for analysis

Do not run when:

- execution is still in flight
- review is incomplete or ambiguous
- no durable or reusable learning emerged (skip with explicit note)

If entry criteria fail, route precisely:

- review incomplete → `pulse:workflow review`
- execution still in flight → `pulse:workflow execute` or `pulse:workflow swarm`
- runtime/session posture unclear → `pulse:workflow use`

## Command-local references

- [runtime-appendix.md](runtime-appendix.md) — context sources, analysis stream prompts, synthesis quality bar, propagation taxonomy, promotion rules, memory destinations, and state update contract
- [learnings-template.md](learnings-template.md) — learnings file structure and examples
- [analysis-prompts.md](analysis-prompts.md) — prompts for pattern, decision, and failure analysis streams
- [corrections-and-ratchets.md](corrections-and-ratchets.md) — correction and ratchet file structures

## Phase flow

```text
Gather Context -> Analyze (3 streams) -> Synthesize Learnings -> Classify & Route -> Promote Critical -> Update State
```

### Phase 1 — Gather context

Read work artifacts, verification evidence, daemon posture, and workgraph metadata for the completed slice.

Minimum reads:

- story `README.md`, `work-brief.md`, `discovery.md`, `solution-design.md`
- Ticket `README.md` and `verification.md`
- daemon posture from `pulse daemon status`
- `.pulse/workgraph/nodes/` or a Rust graph projection when structure clarifies the completed slice
- review findings and relevant verification artifacts

Fallback if work artifacts are partial: session summary plus recent diff.

Context source details are in [runtime-appendix.md](runtime-appendix.md#gather-context).

### Phase 2 — Analyze (3 streams)

Run three analysis streams in parallel and collect outputs:

1. **Pattern extractor** — reusable code, architecture, process, and integration patterns
2. **Decision analyst** — good calls, bad calls, surprises, and tradeoffs
3. **Failure analyst** — bugs, blockers, wasted effort, and missing prerequisites

Use [analysis-prompts.md](analysis-prompts.md) for stream prompts.

### Phase 3 — Synthesize learnings

Write one learnings file per completed feature or work slice:

```text
the owning learning artifact directory/YYYYMMDD-<slug>.md
```

Use [learnings-template.md](learnings-template.md) for file structure.

Synthesis quality bar:

- each learning must have `domain`, `severity`, `category`, and `applicable-when`
- `applicable-when` must name a concrete technical trigger, not a lifecycle phase
- reject vague or non-actionable guidance

Quality details are in [runtime-appendix.md](runtime-appendix.md#synthesis-quality-bar).

### Phase 4 — Classify and route

Classify each learning into exactly one propagation route:

| Route | Meaning | Destination |
|-------|---------|-------------|
| `global-critical` | cross-feature planner-visible rule | owning learning artifact (candidate) |
| `correction` | tactical guardrail for repeated/expensive mistake | owning learning artifact/corrections/ |
| `ratchet` | non-regression must-check from repeated/costly miss | owning learning artifact/ratchet/ |
| `work-item-local` | attach to future work via item `memory_hooks.learnings` | learnings file only |
| `planner-only` | planning/decomposition heuristic | learnings file only |

Propagation behavior:

- planners read `global-critical` directly
- planners attach `work-item-local`, `correction`, and `ratchet` refs into future item `memory_hooks` when triggers match
- workers consume only routed memory hooks plus approved handoff context, not the whole memory corpus

Use [corrections-and-ratchets.md](corrections-and-ratchets.md) for correction and ratchet file structures.

### Phase 5 — Promote critical learnings

Promote to the owning learning artifact only when all are true:

- cross-feature value
- meaningful waste prevented if known earlier
- generalizable beyond a narrow implementation detail
- concise enough for a planner-read file
- not just work-item-local implementation guidance

Append promoted entries with a link back to the full learnings file.

### Phase 6 — Update state

Record the corresponding work-artifact note with:

- feature or completed slice
- date
- learnings file path
- count of critical promotions
- count of work-item-local learnings

Output a handoff summary:

- learnings path
- critical promotion count
- work-item-local count
- statement that future planning now has expanded memory

State update details are in [runtime-appendix.md](runtime-appendix.md#state-update).

## Gate posture

Compound is post-cycle. It does not approve or block execution, review, or merge.

Compound produces memory artifacts that inform future `plan` and `validate` cycles. It does not change the completed work, reopen closed items, or alter workgraph metadata.

## Recommended next

After compound completes, the normal next command is `pulse:workflow plan` for the next work cycle, or `pulse:workflow use` to end the session.

## Red flags

- skipping compounding without artifact review
- promoting too many narrow items to `global-critical`
- writing generic non-actionable learnings
- fabricating learnings instead of reporting none when nothing durable emerged
- assuming workers should read the whole memory corpus directly
- not flagging propagation failures when relevant item `memory_hooks` were never attached
