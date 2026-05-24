# Golden Path: One Pulse Feature Run

This is the shortest concrete example of a standard Pulse v2 run.

## Example Request

> Add inbound email support for the agent inbox.

## The Flow

1. `pulse:workflow use`
   - is the normal session-entry command
   - bootstraps `.pulse/runtime` and `.pulse/workgraph` when needed, then restores current Pulse context
   - confirms whether the repo can run swarm, single-worker, planning-only, or blocked

2. `pulse:workflow intake`
   - classifies the request
   - confirms or creates the owning epic/story boundary when structural mutation is approved

3. `pulse:workflow brainstorm` when direction is open
   - compares viable directions
   - produces approved `work-brief.md`

4. `pulse:workflow explore`
   - gathers repo/domain/external evidence
   - writes `discovery.md`

5. `pulse:workflow design`
   - turns discovery evidence into approved solution decisions
   - writes `solution-design.md`
   - stops for Gate 1 approval

6. `pulse:workflow plan`
   - decomposes approved `solution-design.md` without changing it
   - writes lowercase `plan.md`
   - records docs impact for `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, `docs/decisions/`, and `docs/product/`
   - prepares current-work and workgraph materialization posture
   - stops for Gate 2 approval

7. `pulse:workflow validate`
   - verifies feasibility/readiness for selected current work
   - runs spikes for risky items
   - stops until execution is explicitly approved (Gate 3)

8. `pulse:workflow swarm` or `pulse:workflow execute`
   - starts only after Gate 3 approval
   - implements approved work with reservations and explicit verification

9. `pulse:workflow review`
   - runs specialist review
   - records P1/P2/P3 findings
   - blocks merge while P1 findings remain

10. `pulse:workflow compound`
    - records durable learnings for future runs

## Quick Scout

Before resuming or planning deeper work on an onboarded repo, run the rendered runtime scout command.

```bash
{{pulse_command}} status --repo-root <repo> --json
```

Use the scout output to decide which artifacts to open next.

## Runtime Commands

Use `{{pulse_command}}` for readiness, reservation, and workgraph operations:

```bash
{{pulse_command}} ready --repo-root <repo> --json
{{pulse_command}} reservation list --repo-root <repo> --active-only --json
{{pulse_command}} workgraph list --repo-root <repo> --json
{{pulse_command}} workgraph doctor --repo-root <repo> --json
```

Approved current-slice workgraph items are created or updated through `{{pulse_command}} workgraph`; do not hand-edit `.pulse/workgraph/items.jsonl`.

## Core Promise

Pulse adds deliberate structure so decisions, approvals, implementation, documentation, workgraph state, and verification remain consistent from request to merge.
