# Execute completion report contract

Use this report contract immediately after successful close flow.

## Worker mode payload (`[DONE]`)

Include all of:

- `item_id`
- `runtime_identity`
- `close_result` (`pulse-work close` status)
- `commit_hash` (or `COMMIT_BLOCKED` reason)
- `files_changed`
- `verify_result` summary
- `evidence_paths`
- `follow_up_item` (if needed)

Send only after reservations are released.

## Blocking payload (`[BLOCKED]` or `[FILE CONFLICT]`)

Include all of:

- `item_id`
- blocker type (`verify_failure`, `reservation_conflict`, `missing_contract`, etc.)
- requested/affected paths
- current holder (if known)
- attempted retries (if verification blocker)
- required coordinator action

## Standalone mode record

When running single-worker mode, record equivalent completion details in `.pulse/runtime/STATE.md` with clear item ID and evidence path mapping.
