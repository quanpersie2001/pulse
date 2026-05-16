# Swarming Appendix

Operational appendix for `pulse:swarming`. Keep `SKILL.md` as the hot path and load this file only when you need concrete runtime protocol bodies.

---

## A) Worker Spawn Payload

Provide each worker:
- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `epic_id`
- `feature_name`
- optional `startup_hint`
- scoped task context by default (inherit full parent context only when explicitly needed)

Do not invent worker identities. Use the identity from the runtime spawn primitive.

---

## B) Worker Prompt Template

```text
You are a bounded worker in the Pulse swarm.

## Your Identity
- Runtime identity: <RUNTIME_IDENTITY>
- Coordinator identity: <COORDINATOR_IDENTITY>
- Runtime adapter: <ADAPTER_NAME>
- Epic ID: <EPIC_ID>
- Feature: <FEATURE_NAME>

## Startup Contract
1. Read `AGENTS.md`.
2. If present, run `node .pulse/scripts/pulse_status.mjs --json`.
3. Load `pulse:executing`.
4. Post `[ONLINE]` on the active coordination surface with:
   - runtime identity
   - `AGENTS.md: read`
   - `pulse:executing: loading`
   - next step: `bv --robot-priority`
5. Only after `[ONLINE]` may you claim a bead.

## Coordination Surface
- Claude Code: `SendMessage`
- Codex: parent-thread worker follow-ups

## Reservation Contract
Reserve before editing:
node .pulse/scripts/pulse_reservations.mjs reserve --agent <RUNTIME_IDENTITY> --bead <BEAD_ID> --path <glob> --json

If reserve fails, post `[FILE CONFLICT]` immediately and wait for coordinator resolution.

Release before `[DONE]`:
node .pulse/scripts/pulse_reservations.mjs release --agent <RUNTIME_IDENTITY> --json

## Work Selection Contract
- Use `bv --robot-priority` plus bead graph state.
- Treat `<STARTUP_HINT>` as a hint, not fixed assignment unless explicitly assigned.
- Do not freelance outside the current bead.

## Reporting Contract
Report immediately when any of these happen:
- `[ONLINE]`
- `[BLOCKED]`
- `[FILE CONFLICT]`
- `[READY_TO_COMMIT]`
- `[COMMIT_SLOT_GRANTED]`
- `[COMMIT_DONE]`
- `[COMMIT_BLOCKED]`
- `[DONE]`
- `[HANDOFF]`

## What You Must NOT Do
- Do not edit without reservations.
- Do not skip `[ONLINE]`.
- Do not close beads without verification evidence.
- Do not wait silently when blocked or conflicted.
- Do not escalate directly to the user; route through coordinator first.
```

---

## C) Coordination Message Protocol

### Canonical event schema (all worker/coordinator events)

Use this canonical shape on the active coordination surface:

```json
{
  "event": "<EVENT_NAME>",
  "runtime_identity": "<RUNTIME_IDENTITY>",
  "timestamp": "<ISO-8601>",
  "payload": {}
}
```

Required top-level fields for every event:
- `event`
- `runtime_identity`
- `timestamp`
- `payload`

Missing-field rule:
- if any required field is missing, the coordinator must request a corrected event and must not infer or backfill missing values

### Required worker events

#### `[ONLINE]`
`payload` must include:
- `agents_read` (`true`)
- `executing_loaded` (`true`)
- `next_step` (`bv --robot-priority`)

#### `[BLOCKED]`
`payload` must include:
- `bead_id`
- `blocker_type` (`MISSING_CONTEXT | DEPENDENCY_NOT_MET | TECHNICAL_FAILURE | AMBIGUITY`)
- `description`
- `ask`
- `paused` (`true`)

#### `[FILE CONFLICT]`
`payload` must include:
- `bead_id`
- `requested_scope`
- `holder_identity` (nullable when unknown)
- `reason`
- `decision_request` (`true`)

#### `[READY_TO_COMMIT]`
`payload` must include:
- `bead_id`
- `commit_summary`
- `files_modified`
- `verification_paths`
- `queue_request` (`true`)

#### `[COMMIT_SLOT_GRANTED]`
`payload` must include:
- `bead_id`
- `slot_token`
- `granted_by`

#### `[COMMIT_DONE]`
`payload` must include:
- `bead_id`
- `commit_hash`
- `slot_token`
- `files_modified`

#### `[COMMIT_BLOCKED]`
`payload` must include:
- `bead_id`
- `slot_token` (nullable when no slot issued)
- `reason`
- `ask`

#### `[DONE]`
`payload` must include:
- `bead_id`
- `bead_title`
- `commit_hash`
- `implementation_summary`
- `files_modified`
- `verification_paths`
- `context_budget`

#### `[HANDOFF]`
`payload` must include:
- `bead_id`
- `handoff_path`
- `resume_briefing`
- `open_blockers`

### Coordinator control messages

- `[SWARM_START]` — announce execution model and spawned workers.
- `[CONFLICT_DECISION]` — resolve requester/holder action (`WAIT | RELEASE_AT_SAFE_POINT | DEFER`).
- `[OVERSEER]` — global reminder/correction.
- `[CONTEXT_WARNING]` — pause boundary with handoff summary.
- `[SWARM_COMPLETE]` — completion summary + next skill.

---

## D) Silence Ladder (Coordinator)

- 2 quiet cycles -> reminder
- 3 quiet cycles -> direct status check + stalled marker when appropriate
- 5 quiet cycles with active work remaining -> escalate to user

Active swarm never idles: continue tend loop while any worker is active or any executable bead remains.

---

## E) Coordinator Handoff Contract

When coordinator context exceeds ~65%:
1. write `.pulse/handoffs/coordinator.json` using `../using-pulse/references/handoff-contract.md`
2. register it in `.pulse/handoffs/manifest.json`
3. broadcast pause notice with handoff summary and resume briefing
4. refresh feature checkpoint if `.pulse/checkpoints/<feature>/...` is in use
5. report safe pause and resume path

Use these fields:
- `summary`
- `next_action`
- `read_first`
- `payload.transfer` (status, completed, in_flight, blockers, resume_notes)
