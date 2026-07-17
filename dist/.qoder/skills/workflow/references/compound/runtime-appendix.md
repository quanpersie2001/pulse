# `pulse:workflow compound` Runtime Appendix

Use this appendix for operational detail referenced by `command.md`. The command file owns the phase flow; this file should stay compact and avoid repeating the procedure.

## Gather context

Read from these sources for the completed slice:

- story artifacts: `README.md`, `work-brief.md`, `discovery.md`, `solution-design.md`
- task/bug artifacts: `README.md`, `verification.md`
- optional when present: `approach.md`, `execplan.md`, `validation.md`, `lifecycle-summary.md`, `references/`
- `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
- `.pulse/runtime/handoffs/manifest.json` and relevant owner handoff files
- `.pulse/workgraph/items.jsonl` or `.pulse/workgraph/views/graph.json`
- review findings, note outputs, and relevant verification artifacts

Fallback if work artifacts are partial: session summary plus recent diff.

## Analysis streams

Three streams run in parallel. Use [analysis-prompts.md](analysis-prompts.md) for prompt contracts.

| Stream | Output file | Focus |
|--------|-------------|-------|
| Pattern extractor | `/tmp/compounding-patterns.md` | reusable code, architecture, process, integration patterns |
| Decision analyst | `/tmp/compounding-decisions.md` | good calls, bad calls, surprises, tradeoffs |
| Failure analyst | `/tmp/compounding-failures.md` | bugs, blockers, wasted effort, missing prerequisites |

## Synthesis quality bar

Each learning entry must include:

- `domain`
- `severity` (`critical` or `standard`)
- `category` (`pattern` | `decision` | `failure`)
- `applicable-when` — a concrete technical trigger, not a lifecycle phase

Reject vague guidance. If `applicable-when` cannot name a specific trigger state, the learning is not reusable.

## Propagation taxonomy

Classify each learning into exactly one route:

- `global-critical` — cross-feature planner-visible rule; candidate for `.pulse/memory/critical-patterns.md`
- `correction` — tactical guardrail for a repeated or expensive mistake; write under `.pulse/memory/corrections/`
- `ratchet` — non-regression must-check from repeated or costly miss; write under `.pulse/memory/ratchet/`
- `work-item-local` — attach to future work through item `memory_hooks.learnings`
- `planner-only` — planning/decomposition heuristic; not worker default context

## Promotion rules

Promote to `.pulse/memory/critical-patterns.md` only when all are true:

- cross-feature value
- meaningful waste prevented if known earlier
- generalizable beyond a narrow implementation detail
- concise enough for a planner-read file
- not just work-item-local implementation guidance

Append promoted entries with a link back to the full learnings file.

## Memory destinations

| Destination | Meaning |
|-------------|---------|
| `.pulse/memory/learnings/` | per-feature durable learning bundle |
| `.pulse/memory/critical-patterns.md` | compact planner-read global-critical index |
| `.pulse/memory/corrections/` | tactical corrective rules for repeated mistakes |
| `.pulse/memory/ratchet/` | trigger-bound must-check non-regression rules |

Propagation behavior:

- planners read `global-critical` directly
- planners attach `work-item-local` / `correction` / `ratchet` refs into future item `memory_hooks` when triggers match
- workers consume only routed memory hooks plus approved handoff context, not the whole memory corpus

## State update

Update `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json` with:

- feature or completed slice
- date
- learnings file path
- count of critical promotions
- count of work-item-local learnings

Output handoff summary:

- learnings path
- critical promotion count
- work-item-local count
- statement that future planning now has expanded memory
