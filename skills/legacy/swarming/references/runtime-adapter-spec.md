# Runtime Adapter Spec

Pulse defines one swarm contract at the behavior level. Real runtimes may implement it with different primitives.

## Canonical Behaviors

These are the required behaviors every swarm-capable runtime must preserve:

1. `spawn_worker` — create a bounded worker instance
2. `deliver_startup_context` — give that worker the feature, epic, runtime identity, and any startup hint
3. `report_online` — worker confirms it read `AGENTS.md` and loaded `pulse:executing`
4. `report_done` — worker reports bead closure, verification, and evidence paths
5. `report_blocked` — worker reports blocker details immediately
6. `report_conflict` — worker reports reservation conflicts immediately
7. `observe_worker_progress` — coordinator can see worker updates without guessing from git history
8. `reserve_paths` — workers must claim file scope before editing
9. `release_paths` — workers release file scope before advertising completion
10. `write_handoff` — coordinator and workers can pause safely with owner-scoped handoffs
11. `restore_resume_state` — a resumed turn can reopen the active coordination surface and continue safely

## Shared Rules

- Beads plus `bv` remain the source of truth for work selection.
- Runtime coordination must not create a second planning graph beside beads.
- Workers do not edit without local reservations.
- The canonical contract must not require inbox polling, topic tags, or registration semantics that only exist in one runtime.
- Pause/resume state lives in `.pulse/state.json`, `.pulse/STATE.md`, and `.pulse/handoffs/`.

## Shared Reservation Layer

Both adapters use the same repo-local reservation helper:

```bash
node .pulse/scripts/pulse_reservations.mjs reserve --agent <worker-id> --bead <bead-id> --path <glob> --json
node .pulse/scripts/pulse_reservations.mjs list --active-only --json
node .pulse/scripts/pulse_reservations.mjs release --agent <worker-id> --json
node .pulse/scripts/pulse_reservations.mjs sweep --json
```

Behavioral requirements:

- overlapping write scopes must be rejected
- workers must report conflicts instead of editing around them
- coordinators should sweep expired reservations before and during active swarms

## Claude Code Adapter

Use Claude Code's native teammate model.

### Preferred primitives

- `TeamCreate` — create a shared swarm team when the run benefits from explicit teammate coordination
- `Agent` — spawn bounded workers
- `SendMessage` — coordinator ↔ worker follow-up and escalation
- `TaskCreate` / `TaskUpdate` / `TaskList` — optional runtime coordination metadata only

### Mapping

- `spawn_worker` → `Agent`
- `deliver_startup_context` → worker prompt plus `SendMessage` follow-up when needed
- `report_*` → worker messages delivered through `SendMessage`
- `observe_worker_progress` → parent thread receives teammate messages automatically
- `write_handoff` / `restore_resume_state` → `.pulse/handoffs/*` and `.pulse/STATE.md`

### Hard rule

Team tasks are optional coordination metadata. They never replace bead planning, bead priority, or bead closure.

## Codex Adapter

Use Codex-native subagents and the parent runtime thread.

### Preferred primitives

- native subagent spawn
- parent-thread follow-up messages
- runtime-native worker results routed back to the parent turn

### Mapping

- `spawn_worker` → native subagent spawn
- `deliver_startup_context` → worker prompt plus parent follow-up when needed
- `report_*` → worker replies in the parent coordination thread
- `observe_worker_progress` → parent thread / runtime follow-up stream
- `write_handoff` / `restore_resume_state` → `.pulse/handoffs/*` and `.pulse/STATE.md`

### Hard rule

Do not force Codex workers through fake inbox, topic, or registration flows. Parent-thread coordination is the canonical Codex surface.

## Degraded Mode

If the active runtime cannot satisfy the behaviors above well enough for safe swarm execution:

- preflight must recommend `single-worker`
- Pulse skips `pulse:swarming`
- execution continues through standalone `pulse:executing`

Do not fake swarm behavior with missing progress visibility or missing reservation enforcement.
