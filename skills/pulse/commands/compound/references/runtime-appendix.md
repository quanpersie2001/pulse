# Compounding Runtime Appendix (Canonical)

This appendix defines the operational contract for compounding.

## 1) Gather Context

Read:

- relevant work artifacts under `works/epics/**`, especially:
  - story `README.md` and `SPEC.md`
  - task / bug `README.md` and `verification.md`
  - `approach.md`, `execplan.md`, `validation.md`, `lifecycle-summary.md`, and `references/` when present
- `.pulse/runtime/STATE.md`
- `.pulse/runtime/state.json`
- `.pulse/runtime/handoffs/manifest.json` (+ relevant owner files)
- `.pulse/workgraph/items.jsonl`
- `pulse-work show <id>`, `pulse-work children <id>`, or `pulse-work graph --json` when workgraph structure clarifies the completed slice
- review findings, note / note-distill outputs, and relevant verification artifacts

Fallback if work artifacts are partial: session summary + recent diff.

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

Write one file per completed feature or work slice:

- `.pulse/memory/learnings/YYYYMMDD-<slug>.md`

Use `references/learnings-template.md`.

## 4) Propagation Taxonomy (must preserve)

Classify each learning into exactly one route:

- `global-critical`
  - planner-visible global rule
  - candidate for `.pulse/memory/critical-patterns.md`
- `correction`
  - tactical guardrail for a repeated or expensive mistake
  - write under `.pulse/memory/corrections/`
- `ratchet`
  - non-regression must-check from repeated or costly misses
  - write under `.pulse/memory/ratchet/`
- `work-item-local`
  - attach to future work through item `memory_hooks.learnings`
- `planner-only`
  - planning/decomposition heuristic; not worker default context

## 5) Promotion Rules for Global-Critical

Promote only when all are true:

- cross-feature value
- meaningful waste prevented if known earlier
- generalizable beyond a narrow implementation detail
- concise enough for a planner-read file
- not just work-item-local implementation guidance

Append promoted entries to `.pulse/memory/critical-patterns.md` with a link back to the full learning file.

## 6) Durable Memory Destinations and Meanings (must preserve)

- `.pulse/memory/learnings/` -> per-feature durable learning bundle
- `.pulse/memory/critical-patterns.md` -> compact planner-read global-critical index
- `.pulse/memory/corrections/` -> tactical corrective rules for repeated mistakes
- `.pulse/memory/ratchet/` -> trigger-bound must-check non-regression rules

Propagation behavior:

- planners read global-critical directly
- planners attach work-item-local / correction / ratchet refs into future item `memory_hooks` when triggers match
- workers consume only routed memory hooks plus approved handoff context, not the whole memory corpus

## 7) State Update + Handoff

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

## 8) Red Flags

- skipping compounding without artifact review
- promoting too many narrow items to global-critical
- writing generic non-actionable learnings
- fabricating learnings instead of reporting none
- assuming workers should read the whole memory corpus directly
- not flagging propagation failures when relevant item `memory_hooks` were never attached
