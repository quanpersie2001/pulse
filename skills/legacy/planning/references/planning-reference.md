# Planning Reference

Use when `pulse:planning` needs quality rules or artifact schemas.

## Quality Rules

- Choose one mode first: `direct_task`, `spike`, `small_change`, `standard_feature`, or `high_risk_feature`.
- Use the least workflow that honestly protects the work.
- Use phases only for observable milestones. Use epics when capability/risk areas explain tough work better.
- `high_risk_feature` defaults to an epic map unless a phase plan is plainly clearer.
- Stories are end-to-end outcomes, not architecture layers.
- Beads are worker-sized tasks for validated current work, not speculative future planning.
- MEDIUM/HIGH unknowns need validating proof or a spike before execution beads.

Trace:

```text
mode -> shape -> epic? -> current work -> bead?
```

## Mode Gate

| Mode | Use When | Shape |
|---|---|---|
| `direct_task` | obvious local change | short approach + direct handoff or one bead |
| `spike` | one assumption decides path | yes/no question + proof |
| `small_change` | <=3 files, LOW risk, no API/data model change | one work shape |
| `standard_feature` | ordered user/system capability | phase plan or epic map |
| `high_risk_feature` | hard-to-reverse, external/security/data, broad blast | epic map + feasibility proof |

Above `small_change`, record why smaller modes are insufficient.

## Discovery And Approach

`discovery.md` should capture only facts needed for the plan:

- architecture snapshot: areas, entry points, key files
- constraints: runtime/framework versions, dependencies, quality gates
- summary: what exists, what is missing, warnings

`approach.md` should capture:

- recommended approach and rejected alternatives
- risk map: component, LOW/MEDIUM/HIGH, reason, proof needed
- likely file/order boundaries
- relevant learnings and validating questions

## Shape Artifacts

For direct, spike, or small work:

```markdown
# Work Shape: <Feature>
Mode: `<mode>`
Why this mode: <why smaller/bigger workflow is unnecessary>
Current work: <outcome or yes/no spike question>
Proof: `<command>` or observable check
Out of scope: <not solved>
Approval (Gate 2): set `PENDING` and stop for explicit approval; default approved path syncs state and stops (`next_action: manual_invoke`), and current-work prep proceeds only when the user explicitly chooses an equivalent of `Approve and continue now` (`next_action: continue_now`).
```

For milestone-shaped work:

```markdown
# Phase Plan: <Feature>
Mode: `standard_feature` | `high_risk_feature`
Feature summary: <2-4 sentences>
Phase overview: Phase | What Changes | Why Now | Demo | Unlocks
Order check: first phase is obvious; later phases build on it; no technical buckets.
Approval summary: current phase, picture after it, deferred work.
```

For tough capability/risk-shaped work:

```markdown
# Epic Map: <Feature>
Mode: `standard_feature` | `high_risk_feature`
Feature outcome: <what is true when all epics finish>
Architecture / reality basis: <repo facts, stack constraints, external limits>
Epics: Epic | Capability/Risk Area | Why It Exists | Stories | Proof Needed
Story queue: Story | Epic | Outcome | Depends On | Feasibility Status
Current story to prepare: <story, why now, testable exit>
```

## Current Work Prep

For epic-map work, prepare only the approved current story:

```markdown
# Current Story Pack: <Story>
Epic: <name>
Entry state: <current repo truth>
Exit state: <what must be true after execution>
Files likely touched: <bounded list>
Feasibility assumptions: <assumption | risk | proof needed>
Verification: <commands/checks>
Out of scope: <not solved>
Bead mapping: <created after validation accepts feasibility>
```

For phase-shaped work, keep the existing contract/story-map pattern:

```markdown
# Phase Contract: Phase <N> - <Name>
Entry state: <observable truth>
Exit state: <testable truth>
Demo: <walkthrough/checks>
Stories: Story | What Happens | Unlocks | Done
Out/success/pivot: <scope boundary, proof, revise signal>

# Story Map: Phase <N> - <Name>
Dependency diagram: Entry -> Story 1 -> Story 2 -> Exit
Story table: Story | Outcome | Contributes To | Creates | Done
Story-to-bead mapping: <br-id or pending validation>
```

## Pressure Scenarios

- Small fix stays `direct_task` or `small_change`; no epic/phase ceremony.
- Tough feature uses an epic map when capability/risk areas are clearer than 2-4 phases.
- MEDIUM/HIGH unknown appears as proof needed before execution beads; standard/high-risk work defers bead creation until validating confirms feasibility and routes back.
- Current story is small enough for feasibility validation and one bounded execution pass.
