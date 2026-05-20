# Golden Path: One Pulse Feature Run

This is the shortest concrete example of a standard Pulse v2 run.

## Example Request

> Add inbound email support for the agent inbox.

## The Flow

1. `/pulse onboard`
   - checks runtime readiness and bootstraps `.pulse/runtime` and `.pulse/workgraph`
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

Before resuming or planning deeper work on an onboarded repo:

```bash
node {{scripts_path}}/pulse_status.mjs --json
```

Use the scout output to decide which artifacts to open next.

## Runtime Workgraph Commands

Use `pulse-work` for runtime metadata operations:

```bash
pulse-work ready --json
pulse-work show <ID> --json
pulse-work create --kind TASK --title "..." --parent <ID>
```

## Core Promise

Pulse adds deliberate structure so decisions, approvals, implementation, and verification remain consistent from request to merge.
