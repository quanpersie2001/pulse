# `pulse:workflow swarm`

Coordinator manual for running Gate 3-approved current-slice execution with parallel workers.

This command is orchestration only. It does not implement product code.

## Role boundary (non-negotiable)

Coordinator owns:

- spawning workers and preserving startup contract
- tending coordination traffic and worker state
- resolving reservation conflicts and commit-slot serialization
- pause/resume continuity through owner-scoped handoffs

Coordinator must not:

- edit product files
- silently re-scope approved work
- treat graph emptiness as automatic feature completion

## Entry criteria

Run `pulse:workflow swarm` only when all are true:

- Gate 3 is explicitly approved
- validate recommends `swarm`
- current-slice work is approved and execution-ready
- active coordination surface is available

If `single-worker` is recommended, route to `pulse:workflow execute`.

## Required context reads

1. `AGENTS.md`
2. `.pulse/runtime/tooling-status.json`
3. `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
4. active current-slice artifacts under `works/` for the approved shape
5. coordinator handoff, if resuming: `.pulse/runtime/handoffs/coordinator.json`

## Phase flow

```text
Readiness -> Surface Init -> Spawn -> Tend Loop -> Pause or Complete
```

### Phase 1 — Confirm swarm readiness

1. Reconfirm Gate 3 approval and slice boundaries.
2. Inspect executable readiness:

```bash
pulse-work ready --json
```

3. Inspect full graph posture for active epic/story:

```bash
pulse-work graph --json
```

4. Update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` with active coordinator intent.

Hard stop if approval/slice boundaries conflict across artifacts/state.

### Phase 2 — Initialize coordination surface

Establish one authoritative surface for:

- worker startup acknowledgments (`[ONLINE]`)
- blocker/conflict reports (`[BLOCKED]`, `[FILE CONFLICT]`)
- completion reports (`[DONE]`)
- coordinator broadcasts and commit-slot grants

No hidden side channel for authoritative decisions.

Use `swarming-appendix.md` as the event protocol and message-body contract.

### Phase 3 — Spawn workers with startup context

Spawn bounded workers and require each to run `pulse:workflow execute`.

Provide each worker:

- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `active_epic_id`
- `active_story_id` or slice scope
- optional `startup_hint`

Do not pre-assign permanent tracks. Workers self-route from live `pulse-work ready` output after startup checks.

Immediately register each worker in `.pulse/runtime/STATE.md` under `## Active Workers`.

### Phase 4 — Tend loop (continuous while actionable work exists)

Stay in tending mode while any worker is active, blocked, expected to report, or while `pulse-work ready` still returns executable work.

Each cycle must:

1. Process new worker events and validate required fields.
2. Update worker status in `.pulse/runtime/STATE.md`.
3. Respond immediately to blockers/conflicts.
4. Re-check `pulse-work ready --json` and `pulse-work graph --json` after significant transitions.
5. Refresh reservation posture when conflicts or stalled workers appear:

```bash
node {{scripts_path}}/pulse_reservations.mjs --repo-root <repo> list --active-only --json
```

6. Enforce one active commit slot on shared branch at a time.

If runtime cannot wake/poll and no actionable signal exists, run one full tend cycle, persist pause state, and report paused-awaiting-signal (not complete).

### Worker event obligations

If a required field is missing, request corrected event payload and do not infer.

Coordinator must verify, per event:

- item transition is valid in `pulse-work`
- reservation ownership is safe before overlapping edits
- commit slot is granted before any worker commits

### Silence ladder

Use reminder/escalation ladder from `swarming-appendix.md` before escalating to user. Escalate only for product decisions, persistent silence, or unresolved collisions after recovery attempts.

### Handoff reassignment rule

Normal rule is same-owner resume only.

Coordinator may reassign orphaned worker handoff only after confirming:

1. prior worker inactivity
2. reservation transfer safety
3. commit queue transfer safety
4. manifest + owner handoff metadata updated in `.pulse/runtime/handoffs/manifest.json`

### Phase 5 — Pause/resume integrity

If context is critical or coordinator must stop:

1. write `.pulse/runtime/handoffs/coordinator.json`
2. register in `.pulse/runtime/handoffs/manifest.json`
3. broadcast paused-not-complete summary with resume instructions
4. preserve worker roster, blockers, reservations, and commit-slot posture

Pause is not completion.

### Phase 6 — Completion determination

Treat empty ready-list as a signal, not proof.

Completion requires all:

- no executable current-slice work remains
- no unresolved blockers/conflicts for active slice
- approved shape artifacts and `.pulse/runtime/STATE.md` agree on whether later slices remain

Then route:

- final slice complete -> recommend `pulse:workflow review`
- more slices remain -> recommend `pulse:workflow plan`

## Red flags

- coordinator edits product files
- workers idle while ready work exists
- repeated unresolved path conflicts
- parallel commits without slot control
- claiming completion while paused awaiting signals
- promoting graph emptiness to feature completion without artifact/state confirmation

## References

- `swarming-appendix.md`
- `runtime-adapter-spec.md`
