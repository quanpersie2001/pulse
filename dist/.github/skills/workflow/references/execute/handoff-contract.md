# Execute handoff contract

Use this contract when `pulse:workflow execute` pauses due to context pressure or forced stop.

## Owner-scoped files

- worker mode: `.pulse/runtime/handoffs/worker-<runtime_identity>.json`
- single-worker mode: `.pulse/runtime/handoffs/single-worker.json`

A worker must not consume another worker’s handoff unless coordinator reassignment is explicitly recorded in manifest + handoff file.

## Required payload

Every handoff must include:

- `owner_identity`
- `mode` (`worker` | `single-worker`)
- `paused_reason` (`context_critical` or explicit blocker)
- `last_closed_item` (or `null`)
- `verification_evidence_paths` for last closed item
- `transfer.completed` (recent completions)
- `transfer.in_flight` (active item + current step)
- `transfer.blockers` (blocking condition + who can unblock)
- `transfer.resume_notes` (minimal notes for seamless resume)
- `next_action` that starts with coordination/state check then item selection

## Manifest registration

After writing owner handoff file, register/update `.pulse/runtime/handoffs/manifest.json` with:

- owner identity
- handoff file path
- summary
- next action
- timestamp

Worker mode requires notifying coordinator after handoff write+manifest update.
