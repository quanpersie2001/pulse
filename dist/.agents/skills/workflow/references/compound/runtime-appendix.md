# `pulse:workflow compound` Runtime Appendix

Use this appendix for operational detail referenced by `command.md`. The command file owns the phase flow; this file should stay compact and avoid repeating the procedure.

## Gather context

Read from these sources for the completed slice:

- story artifacts: `README.md`, `work-brief.md`, `discovery.md`, `solution-design.md`
- Ticket artifacts: `README.md`, `verification.md`
- optional when present: `approach.md`, `execplan.md`, `validation.md`, `lifecycle-summary.md`, `references/`
- daemon posture from `pulse daemon status`
- the owning work artifact and relevant handoff note
- `.pulse/workgraph/nodes/` or `derived graph projection`
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

- `global-critical` — cross-feature planner-visible rule; candidate for the owning learning artifact
- `correction` — tactical guardrail for a repeated or expensive mistake; write under the owning learning artifact/corrections/
- `ratchet` — non-regression must-check from repeated or costly miss; write under the owning learning artifact/ratchet/
- `work-item-local` — attach to future work through item `memory_hooks.learnings`
- `planner-only` — planning/decomposition heuristic; not worker default context

## Promotion rules

Promote to the owning learning artifact only when all are true:

- cross-feature value
- meaningful waste prevented if known earlier
- generalizable beyond a narrow implementation detail
- concise enough for a planner-read file
- not just work-item-local implementation guidance

Append promoted entries with a link back to the full learnings file.

## Memory destinations

| Destination | Meaning |
|-------------|---------|
| owning learning artifact/learnings/ | per-feature durable learning bundle |
| owning learning artifact/ | compact planner-read global-critical index |
| owning learning artifact/corrections/ | tactical corrective rules for repeated mistakes |
| owning learning artifact/ratchet/ | trigger-bound must-check non-regression rules |

Propagation behavior:

- planners read `global-critical` directly
- planners attach `work-item-local` / `correction` / `ratchet` refs into future item `memory_hooks` when triggers match
- workers consume only routed memory hooks plus approved handoff context, not the whole memory corpus

## State update

Record the corresponding work-artifact note with:

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
