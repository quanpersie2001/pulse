# Swarm Execution Rules

Use this contract when `/pulse swarm` is selected after Gate 3 approval.

## Preconditions

- validated current slice with explicit Gate 3 approval
- safe decomposition boundaries
- clear worker ownership and verification boundaries
- reservation mechanism available for shared file coordination

If these are not true, prefer `execute` or return to `plan`.

## Role boundaries

### Coordinator

Coordinator owns orchestration only:

- launch and scope workers
- enforce reservation/ownership boundaries
- resolve conflicts and blockers
- maintain progress visibility
- serialize shared-branch commits through one commit queue

Coordinator should not implement worker code changes.

### Worker

Each worker must:

- follow assigned scope
- claim reservations before editing shared files
- report blockers immediately
- provide verification evidence for completed slice
- release/transfer reservations cleanly

## Coordination loop

While swarm is active, repeat:

1. ingest worker updates
2. update active worker state
3. resolve blockers/conflicts
4. refresh ready-work view
5. issue next coordination actions

If the runtime has no autonomous wakeup and no actionable signals remain, persist pause/handoff state instead of falsely reporting completion.

## Commit queue rule

On a shared branch, only one worker holds commit slot at a time.

- worker reports `READY_TO_COMMIT`
- coordinator grants `COMMIT_SLOT_GRANTED`
- worker commits declared scoped files
- worker reports completion and releases slot

## Completion and handoff

Swarm completion requires both:

- current slice execution complete
- approved shape/state artifacts agree that review is the next step

If later slices remain, hand off to `plan`; if final slice is complete, hand off to `review`.

## Failure posture

When decomposition becomes unsafe:

- stop launching new workers
- collapse to narrower execution or reroute to `rescue`
- keep blockers explicit with concrete next decisions