# `pulse:workflow swarm` Runtime Appendix

Use this appendix for event protocol, tend-loop details, and operational contracts referenced by `command.md`.

## Event protocol

All worker events must include:

- `event_type`
- `runtime_identity`
- `item_id` (when applicable)
- `timestamp`
- `summary`

Optional event-specific payload fields are required when relevant (conflict holder, verify result, commit hash, evidence path, blocker reason).

If required fields are missing, coordinator requests corrected event and does not infer.

### Required worker events

| Event | When | Key payload |
|-------|------|-------------|
| `[ONLINE]` | worker startup | `runtime_identity`, readiness confirmations |
| `[BLOCKED]` | execution blocker | `item_id`, `phase`, `blocking_reason`, `needs`, `recommended_reroute` |
| `[FILE CONFLICT]` | reservation collision | `item_id`, requested paths, current holder |
| `[READY_TO_COMMIT]` | commit slot request | `item_id`, exact file list |
| `[COMMIT_DONE]` | commit confirmation | `item_id`, commit hash, files changed |
| `[HANDOFF]` | pause handoff posted | `item_id`, handoff path |
| `[DONE]` | item completion | `item_id`, commit hash, files changed, verification result, evidence paths, implementation gap summary |

Worker event payloads follow the contracts in [`../execute/runtime-appendix.md`](../execute/runtime-appendix.md#completion-report-contract).

### Coordinator responses

| Response | When |
|----------|------|
| `ACK_ONLINE` | worker startup validated |
| `ACK_BLOCKED` + next decision | blocker acknowledged |
| `CONFLICT_RESOLUTION` | reservation conflict resolved |
| `COMMIT_SLOT_GRANTED` | commit slot assigned to worker |
| `COMMIT_SLOT_WAIT` | commit slot busy, worker must wait |
| `ACK_DONE` | completion report validated |
| `ACK_HANDOFF` | handoff acknowledged |

## Commit-queue serialization

Shared branch allows one active commit slot at a time.

Protocol:

1. Worker sends `[READY_TO_COMMIT]` with exact file list.
2. Coordinator grants `COMMIT_SLOT_GRANTED` to one worker.
3. Worker commits declared scoped files and reports `[COMMIT_DONE]`.
4. Coordinator grants next slot.

Coordinator must verify the worker's file list matches declared scope before granting the slot.

## Tend-loop checklist

Each tend cycle:

1. Read new events from coordination surface.
2. Validate required fields on each event.
3. Verify item transition is valid in workgraph metadata.
4. Update worker status in `.pulse/runtime/STATE.md`.
5. For `[BLOCKED]`: acknowledge, decide next action (reroute, unblock, escalate).
6. For `[FILE CONFLICT]`: check reservation ownership, resolve or escalate.
7. For `[READY_TO_COMMIT]`: verify file list, grant or hold slot.
8. For `[DONE]`: validate completion report, release worker registration.
9. Re-check `node .gemini/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` after significant transitions.
10. Refresh reservation posture when conflicts or stalled workers appear.
11. Check for newly unblocked work to spawn additional workers if safe.

## Silence ladder

When a worker goes silent:

1. Reminder ping on coordination surface.
2. Direct check with expected response field list.
3. Request handoff/status dump.
4. Reservation safety check and possible reassignment prep.
5. Escalate to user when recovery fails or product judgment required.

Escalate only for product decisions, persistent silence, or unresolved collisions after recovery attempts.

## Pause summary contract

Coordinator pause report must include:

- active workers and their statuses
- in-flight blockers/conflicts
- reservation and commit-slot posture
- first resume step
- whether swarm is paused or complete
