# Pulse Bead Template

Use this template to normalize every bead after `br create`.

The point is not formatting purity. The point is a stable contract that `planning`, `validating`, `executing`, and `reviewing` can all rely on.

## Canonical Shape

```yaml
---
id: br-000
title: Implement ...
type: task
feature: feature-slug
priority: 1
dependencies:
  - br-001
files:
  - src/path/to/file.ts
  - src/path/to/file.test.ts
verify:
  - command: <repo-appropriate verification command>
    expect: exits 0
verification_evidence:
  - kind: artifact
    path: history/<feature>/verification/br-000.md
    note: Captured output from the final verification command
testing_mode: standard
decision_refs:
  - D1
learning_refs:
  - .pulse/memory/learnings/20260327-auth-cookie.md
labels:
  - feature
  - review-safe
---
```

## Epic Beads (whole-feature root)

When `type: epic`, keep the usual contract fields and add a minimal phase snapshot:

```yaml
phase_tracking:
  total_phases: 3
  current_phase: Phase 2 - API wiring
  completed_phases:
    - Phase 1 - foundation
  final_phase_ready: false
```

Use this snapshot only to make the graph easier to read. It must be copied from `history/<feature>/phase-plan.md` and `.pulse/STATE.md`, and it must never override those artifacts.

Do not try to encode every per-phase nuance into the epic bead. Keep the snapshot minimal and refresh it whenever the planned current phase changes.

## Required Fields

- `dependencies`
  - bead IDs this bead depends on
- `files`
  - the files this bead expects to modify or create
- `verify`
  - one or more runnable checks with expected outcomes
- `verification_evidence`
  - one or more artifact paths or explicit verification records
- `testing_mode`
  - `standard` or `tdd-required`
- `decision_refs`
  - locked decisions from `CONTEXT.md`
- `learning_refs`
  - only the learnings relevant to this bead

## Validation Rules

These rules are intentionally strict because downstream skills rely on them as structured data:

1. `dependencies`, `files`, `verify`, `verification_evidence`, `testing_mode`, `decision_refs`, and `learning_refs` must always exist.
2. Empty is allowed. Omission is not.
3. `files` must be explicit paths, not vague phrases like "auth files" or "related tests".
4. `verify` must contain runnable checks with expected outcomes, not generic prose.
5. `verification_evidence` must point to a standard evidence artifact path or an explicit verification record.
6. `testing_mode` must be set to `standard` or `tdd-required`.
7. If `testing_mode` is `tdd-required`, add `tdd_steps` with distinct red and green commands.
8. `decision_refs` must point to actual decision IDs from `CONTEXT.md` when relevant.
9. `learning_refs` should only include learnings this bead genuinely needs.
10. If a correction entry or ratchet rule clearly applies to the bead, include that path in `learning_refs` instead of assuming the worker will rediscover it from global memory.
11. If `type: epic`, include `phase_tracking.total_phases`, `phase_tracking.current_phase`, `phase_tracking.completed_phases`, and `phase_tracking.final_phase_ready` as a convenience snapshot copied from `phase-plan.md` and `.pulse/STATE.md`.

## Body Sections

After the structured block, use this body shape:

```markdown
# Goal
<what this bead delivers>

## Why
<why this work exists>

## Implementation Notes
- <key constraints from approach.md>
- <relevant patterns to follow>

## Planning Context
- Decision refs: D1, D4
- Learning refs: .pulse/memory/learnings/20260327-auth-cookie.md
- Testing mode: standard | tdd-required
- Testing hint from story map: <smoke / focused integration / full integration-regression>
- Verification evidence: history/<feature>/verification/<bead-id>.md or an explicit verification record
- If `testing_mode` is `tdd-required`, include red and green steps in `tdd_steps`
- Translate the story-map testing hint into concrete `verify` depth, and escalate to `tdd-required` when the phase plan or risk map says that is mandatory

## Done When
- <behavioral acceptance criteria>
- <integration expectation>
```

## Spike Beads

Spike beads may use the same structure with these differences:

- `type: spike`
- include a single `spike_question`
- time-box the work
- `verify` should describe the proof path

Example:

```yaml
---
id: br-099
title: Spike: Does JWT cookie auth work for WebSockets on this stack?
type: spike
feature: auth-refresh
priority: 0
dependencies: []
files:
  - .spikes/auth-refresh/br-099/FINDINGS.md
verify:
  - command: document proof in FINDINGS.md with a definitive YES or NO
    expect: file exists and answer is explicit
verification_evidence:
  - kind: artifact
    path: .spikes/auth-refresh/br-099/FINDINGS.md
    note: Definitive YES/NO spike finding
testing_mode: standard
decision_refs:
  - D3
learning_refs: []
spike_question: Does JWT cookie auth work for WebSockets on this stack?
---
```

## Rules

1. Do not rely on prose alone for file scope or verification.
2. Do not invent alternate field names such as `scope`, `checks`, or `depends_on`.
3. If a field is empty, write an empty list. Do not omit the field.
4. If a bead cannot be normalized to this schema, split or rewrite the bead before handing it to validating.
