# TK-006 Decide the state-file schema evolution strategy

## Objective
Answer, with evidence from the current code: how should `.todolist.json`
evolve when optional fields like `due` are introduced — tolerant per-field
defaulting on load, or an explicit format version with a migration step?
Produce a recommended decision that TK-005 can rely on.

## Current behavior
`loadTodos` reads the file, maps `ENOENT` to `[]`, and `JSON.parse`s with
no shape check and no version key. Every consumer assumes the historical
array-of-todos shape. ST-002 relies on tolerant loading (missing `due` =
undated) without a recorded rationale.

## Target behavior
A written decision, not a code change:
- `works/TK-006/research/schema-evolution.md` compares at least two
  strategies (tolerant defaulting, explicit version + migration, and any
  other the research surfaces) with trade-offs anchored in the actual
  `src/cli.mjs` load/save paths.
- `works/TK-006/decision.md` records the recommended strategy, why, its
  consequences, and what would change its mind (reversal triggers).

## Code anchors
- src/cli.mjs

## Required changes
- Research note and decision document under `works/TK-006/` only; no
  source, test, or doc changes.

## Invariants
- The recommendation must keep pre-due files loading unchanged (ST-002
  compatibility is a hard constraint).
- No user data loss in any scenario the strategy describes.

## Implementation freedom
open: research structure and depth are the worker's; the two required
artifacts and the hard constraint are fixed.

## Scope
- `works/TK-006/research/schema-evolution.md`, `works/TK-006/decision.md`.

## Non-scope
- Implementing any migration, changing the state format, product docs.

## Acceptance
- AC-1: The research note compares at least two concrete strategies with
  trade-offs citing the current load/save code paths.
- AC-2: `decision.md` names one recommended strategy with rationale,
  consequences, and reversal triggers.
- AC-3: The recommendation explicitly states how pre-due files load under
  it, and is directly usable by TK-005.

## Verify
- node scripts/verify.mjs

## Open questions
- (resolved) Output is a recommendation in decision.md; accepting it as a graph Decision node is the developer's act, not this ticket's.

## Documentation impact
- Posture: none
- Rationale: the strategy lands in a Decision record on acceptance; the
  product doc changes only when behavior changes (TK-005).

## QA impact
- Posture: none
- Rationale: research-only ticket; no behavior surface exists to check.

## Expected handoff
- Research note, decision.md, verify result confirming no code drifted.
