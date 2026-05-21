# Swarming Appendix

## Coordinator event protocol

All worker events must include:

- `event_type`
- `runtime_identity`
- `item_id` (when applicable)
- `timestamp`
- `summary`

Optional event-specific payload fields are required when relevant (conflict holder, verify result, commit hash, evidence path, blocker reason).

If required fields are missing, coordinator requests corrected event and does not infer.

## Required worker events

- `[ONLINE]` startup acknowledgment
- `[BLOCKED]` execution blocker report
- `[FILE CONFLICT]` reservation collision
- `[READY_TO_COMMIT]` commit slot request
- `[COMMIT_DONE]` commit confirmation
- `[HANDOFF]` pause handoff posted
- `[DONE]` item completion report

## Coordinator responses

- `ACK_ONLINE`
- `ACK_BLOCKED` + next decision
- `CONFLICT_RESOLUTION`
- `COMMIT_SLOT_GRANTED` or `COMMIT_SLOT_WAIT`
- `ACK_DONE`
- `ACK_HANDOFF`

## Commit-slot serialization

Shared branch allows one active commit slot at a time.

Protocol:

1. worker sends `[READY_TO_COMMIT]` with exact file list
2. coordinator grants `COMMIT_SLOT_GRANTED` to one worker
3. worker commits and reports `[COMMIT_DONE]`
4. coordinator grants next slot

## Silence ladder

1. reminder ping
2. direct check with expected response field list
3. request handoff/status dump
4. reservation safety check + possible reassignment prep
5. escalate to user when recovery fails or product judgment required

## Pause summary contract

Coordinator pause report must include:

- active workers and statuses
- in-flight blockers/conflicts
- reservation/commit-slot posture
- first resume step
- whether swarm is paused or complete
