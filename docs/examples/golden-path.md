# Golden Path: One Pulse Feature Run

This is the shortest concrete example of a standard Pulse v2 run.

## Example Request

> Add inbound email support for the agent inbox.

## The Flow

1. `/pulse use`
   - is the normal session-entry command
   - bootstraps `.pulse/runtime` and `.pulse/workgraph` when needed, then restores current Pulse context
   - confirms whether the repo can run swarm, single-worker, planning-only, or blocked

2. `/pulse explore`
   - asks missing product and boundary questions
   - locks decisions in feature context artifacts

3. `/pulse plan`
   - routes mode to shape (`work-shape.md`, `phase-plan.md`, or `epic-map.md`)
   - prepares current-work artifacts

4. `/pulse validate`
   - verifies feasibility/readiness for selected current work
   - runs spikes for risky items
   - stops until execution is explicitly approved (Gate 3)

5. `/pulse swarm` or `/pulse execute`
   - starts only after Gate 3 approval
   - implements approved work with reservations and explicit verification

6. `/pulse review`
   - runs specialist review
   - records P1/P2/P3 findings
   - blocks merge while P1 findings remain

7. `/pulse compound`
   - records durable learnings for future runs

## Quick Scout

Before resuming or planning deeper work on an onboarded repo, run the rendered runtime scout command.

```bash
{{pulse_command}} status --repo-root <repo> --json
```

Use the scout output to decide which artifacts to open next.

## Runtime Commands

Use `{{pulse_command}}` for readiness and reservation operations:

```bash
{{pulse_command}} ready --repo-root <repo> --json
{{pulse_command}} reservation list --repo-root <repo> --active-only --json
```

## Core Promise

Pulse adds deliberate structure so decisions, approvals, implementation, and verification remain consistent from request to merge.
