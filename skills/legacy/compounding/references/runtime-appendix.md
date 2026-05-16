# Compounding Runtime Appendix (Canonical)

This appendix defines the operational contract for compounding.

## 1) Gather Context

Read:

- `history/<feature>/CONTEXT.md`
- `history/<feature>/discovery.md`
- `history/<feature>/approach.md`
- `history/<feature>/verification/`
- `.pulse/STATE.md`
- `.pulse/handoffs/manifest.json` (+ relevant owner files)
- bead graph (`.beads/` and/or `br show`)
- review findings/beads
- `history/<feature>/lifecycle-summary.md` when present

Fallback if history is partial: session summary + recent diff.

## 2) Analysis Streams (Pattern / Decision / Failure)

Run three analysis streams and write temporary outputs:

- pattern extractor -> `/tmp/compounding-patterns.md`
- decision analyst -> `/tmp/compounding-decisions.md`
- failure analyst -> `/tmp/compounding-failures.md`

Use `references/analysis-prompts.md` for prompt contracts.

## 3) Synthesis Quality Bar

For each learning, include:

- `domain`
- `severity` (`critical` or `standard`)
- `category` (`pattern` | `decision` | `failure`)
- `applicable-when` (specific technical trigger)

Reject vague guidance. `applicable-when` must identify a concrete trigger state, not a lifecycle phase.

Write one file per feature:

- `.pulse/memory/learnings/YYYYMMDD-<slug>.md`

Use `references/learnings-template.md`.

## 4) Propagation Taxonomy (must preserve)

Classify each learning into exactly one route:

- `global-critical`
  - planner-visible global rule
  - candidate for `.pulse/memory/critical-patterns.md`
- `correction`
  - tactical guardrail for repeated/expensive mistake
  - write under `.pulse/memory/corrections/`
- `ratchet`
  - non-regression must-check from repeated/costly misses
  - write under `.pulse/memory/ratchet/`
- `bead-local`
  - attach via future bead `learning_refs`
- `planner-only`
  - planning/decomposition heuristic; not worker default context

## 5) Promotion Rules for Global-Critical

Promote only when all are true:

- cross-feature value
- meaningful waste prevented if known earlier
- generalizable beyond narrow implementation detail
- concise enough for planner-read file
- not just bead-local implementation guidance

Append promoted entries to `.pulse/memory/critical-patterns.md` with link back to full learning file.

## 6) Durable Memory Destinations and Meanings (must preserve)

- `.pulse/memory/learnings/` -> per-feature durable learning bundle
- `.pulse/memory/critical-patterns.md` -> compact planner-read global-critical index
- `.pulse/memory/corrections/` -> tactical corrective rules for repeated mistakes
- `.pulse/memory/ratchet/` -> trigger-bound must-check non-regression rules

Propagation behavior:

- planners read global-critical directly
- planners add bead-local/correction/ratchet refs into `learning_refs` when triggers match
- workers consume only bead-linked learning refs, not whole corpus

## 7) State Update + Handoff

Update `.pulse/STATE.md` and `.pulse/state.json` with:

- feature
- date
- learnings file path
- count of critical promotions
- count of bead-local learnings

Output handoff summary:

- learnings path
- critical promotion count
- bead-local count
- statement that future planning now has expanded memory

## 8) Red Flags

- skipping compounding without artifact review
- promoting too many narrow items to global-critical
- writing generic non-actionable learnings
- fabricating learnings instead of reporting none
- assuming workers should read `critical-patterns.md` directly
- not flagging propagation failures from missing `learning_refs`
