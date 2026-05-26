# Workflow Product Contract

Pulse exposes one public workflow router, `pulse:workflow`, with command references that guide a gated delivery chain.

## Current Chain

```text
pulse:workflow use
→ pulse:workflow intake
→ pulse:workflow brainstorm (optional when direction is open)
→ pulse:workflow explore
→ pulse:workflow design
→ pulse:workflow plan
→ pulse:workflow validate
→ pulse:workflow swarm or pulse:workflow execute
→ pulse:workflow review
→ pulse:workflow compound
```

## Planning Contract

`pulse:workflow plan` consumes approved `solution-design.md` and produces lowercase `plan.md` under the owning story directory.

`plan.md` must include:

- design decision traceability
- affected surfaces
- mandatory docs impact for `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, `docs/decisions/`, and `docs/product/`
- directory/artifact structure changes
- task breakdown
- sequencing and parallelization notes
- one bounded current-work contract
- validation mapping
- workgraph materialization posture
- Gate 2 approval request

Planning must not change approved solution decisions. If decomposition requires a new decision, route back to `pulse:workflow design`. If evidence is missing, route back to `pulse:workflow explore`.

## Workgraph Contract

Intake creates or matches the owning EPIC/STORY boundary and writes `intake.md`. Planning queries that boundary through `{{pulse_command}} workgraph ... --json`, enriches the existing epic/story `README.md` content, then materializes approved current-slice TASK/BUG metadata through `{{pulse_command}} workgraph` commands. Validation then proves the approved `plan.md`, materialized TASK/BUG contracts, runtime mirrors, and workgraph metadata are coherent before Gate 3 execution approval. Agents and humans must treat `.pulse/workgraph/items.jsonl` as database-like storage behind the CLI: do not read or hand-edit it during planning or validation, and do not create duplicate EPIC/STORY items when intake already established the boundary.
