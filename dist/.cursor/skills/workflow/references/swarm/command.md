# `pulse:workflow swarm`

Coordinator manual for running Gate 3-approved current-slice execution with parallel workers.

This command is orchestration only. It does not implement product code.

## Role boundary

Coordinator owns:

- spawning workers and providing startup context
- tending coordination traffic and worker state
- resolving reservation conflicts and commit-slot serialization
- pause/resume continuity through owner-scoped handoffs

Coordinator must not:

- edit product files
- silently re-scope approved work
- treat graph emptiness as automatic feature completion

Workers follow `pulse:workflow execute` in worker mode. Execute owns item selection, implementation, verification, commit discipline, and completion reporting. Swarm does not redefine worker procedure.

## Entry criteria

Run `pulse:workflow swarm` only when all are true:

- Gate 3 is explicitly approved
- validate recommends `swarm`
- current-slice work is approved and execution-ready
- active coordination surface is available

If `single-worker` is recommended, route to `pulse:workflow execute`.

## Command-local references

- [runtime-appendix.md](runtime-appendix.md) — event protocol, tend-loop checklist, commit-queue rules, silence ladder, and pause contract

## Phase flow

```text
Readiness -> Surface Init -> Spawn -> Tend Loop -> Pause or Complete
```

### Phase 1 — Confirm swarm readiness

1. Reconfirm Gate 3 approval and slice boundaries.
2. Inspect executable readiness:

```bash
node .cursor/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json
```

3. Inspect full graph posture for active epic/story from `.pulse/workgraph/views/graph.json`.
4. Update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` with active coordinator intent.

Hard stop if approval/slice boundaries conflict across artifacts/state.

### Phase 2 — Initialize coordination surface

Establish one authoritative surface for:

- worker startup acknowledgments (`[ONLINE]`)
- blocker/conflict reports (`[BLOCKED]`, `[FILE CONFLICT]`)
- completion reports (`[DONE]`)
- coordinator broadcasts and commit-slot grants

No hidden side channel for authoritative decisions.

Event protocol and message bodies are defined in [runtime-appendix.md](runtime-appendix.md#event-protocol).

### Phase 3 — Spawn workers with startup context

Spawn bounded workers. Each worker runs `pulse:workflow execute` in worker mode.

Provide each worker:

- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `active_epic_id`
- `active_story_id` or slice scope
- optional `startup_hint`

Adapter mapping:

- Claude Code: spawn with Agent, coordinate through SendMessage, optional Task metadata only
- Codex: spawn native subagents, parent thread is coordination surface
- Other: use the active coordination surface defined by the runtime

Do not pre-assign permanent tracks. Workers self-route from live `node .cursor/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` output after startup checks.

Immediately register each worker in `.pulse/runtime/STATE.md` under `## Active Workers`.

### Phase 4 — Tend loop (continuous while actionable work exists)

Stay in tending mode while any worker is active, blocked, expected to report, or while `node .cursor/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` still returns executable work.

Each cycle must:

1. Process new worker events and validate required fields.
2. Update worker status in `.pulse/runtime/STATE.md`.
3. Respond immediately to blockers/conflicts.
4. Re-check `node .cursor/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` and active workgraph posture after significant transitions.
5. Refresh reservation posture when conflicts or stalled workers appear:

```bash
node .cursor/skills/workflow/scripts/pulse.mjs reservation list --repo-root <repo> --active-only --json
```

6. Enforce one active commit slot on shared branch at a time.

If a required event field is missing, request corrected payload and do not infer.

Coordinator must verify, per event:

- item transition is valid in the active workgraph metadata
- reservation ownership is safe before overlapping edits
- commit slot is granted before any worker commits

Use the silence ladder from [runtime-appendix.md](runtime-appendix.md#silence-ladder) before escalating to user. Escalate only for product decisions, persistent silence, or unresolved collisions after recovery attempts.

If runtime cannot wake/poll and no actionable signal exists, run one full tend cycle, persist pause state, and report paused-awaiting-signal (not complete).

### Phase 5 — Pause/resume integrity

If context is critical or coordinator must stop:

1. Write `.pulse/runtime/handoffs/coordinator.json`.
2. Register in `.pulse/runtime/handoffs/manifest.json`.
3. Broadcast paused-not-complete summary with resume instructions.
4. Preserve worker roster, blockers, reservations, and commit-slot posture.

Use the pause summary contract from [runtime-appendix.md](runtime-appendix.md#pause-summary-contract).

Pause is not completion.

### Handoff reassignment rule

Normal rule is same-owner resume only.

Coordinator may reassign orphaned worker handoff only after confirming:

1. prior worker inactivity
2. reservation transfer safety
3. commit queue transfer safety
4. manifest + owner handoff metadata updated in `.pulse/runtime/handoffs/manifest.json`

### Phase 6 — Completion determination

Treat empty ready-list as a signal, not proof.

Completion requires all:

- no executable current-slice work remains
- no unresolved blockers/conflicts for active slice
- approved shape artifacts and `.pulse/runtime/STATE.md` agree on whether later slices remain

Then route:

- final slice complete -> recommend `pulse:workflow review`
- more slices remain -> recommend `pulse:workflow plan`

## Gate posture

Swarm consumes Gate 3 approval. It does not approve Gate 3, Gate 4, merge, release, future slices, or unplanned fixes.

After swarm completes for the current slice, the normal next command is `pulse:workflow review`.

## Red flags

- coordinator edits product files
- workers idle while ready work exists
- repeated unresolved path conflicts
- parallel commits without slot control
- claiming completion while paused awaiting signals
- promoting graph emptiness to feature completion without artifact/state confirmation
