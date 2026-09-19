# Error-code audit (P3.4, 2026-09-17)

The plan §2 target is "distinct error codes < 40". The 93 in the metrics
table was a *grep-pattern* count: every string literal that is the first
argument to `PulseError::kernel(...)` / `PulseError::validation(...)` /
the `violation(...)` gate helpers, plus `error.rs`'s fixed variant codes.
This audit is the count correction and the deletion pass in one.

## Count correction: gate labels are not codes

25 of the 93 were `violation(...)` labels (`ready_*`, `close_*`,
`handoff_*`) — row labels inside a gate report that the CLI already folds
into one surfaced code, `gate_failed`:

```
{"code": "gate_failed", "message": "TK-307d is not ready: ready_blocked_by_open: ..."}
```

A user never sees `ready_blocked_by_open` as a code. The plan's original
metric ("distinct error codes") counts what surfaces in the `code` field;
the P1.12 session's grep-script definition ("wide") over-counted by design
and said so. Both counts are kept below; the surfaced count is the metric.

## Deleted: dead groups (nothing constructed them)

- `error.rs` variants whose machinery was already deleted: transaction
  (`ambiguous_transaction`, `event_mismatch`, `invalid_transaction` —
  `storage/transaction.rs` went at `d363734`), CAS (`cas_conflict`),
  content roots (`content_root_violation` — the `works/` path family,
  deleted here with its dead `storage/paths.rs` functions), plus
  `durability_unsupported`, `failpoint`, `not_found`, `already_exists`
  (no constructors anywhere; `serve`'s `not found` is a route enum, not
  this error). Their `code()` arms and the `print_error` CAS arm went too.

## Merged: near-duplicates (same condition class, one recovery surface)

| Kept | Deleted | Why |
|---|---|---|
| `actor_invalid` | `actor_required` | missing and malformed are the same refusal |
| `runner_argv_invalid` | `runner_argv_empty` | empty argv is the degenerate invalid argv |
| `runner_output_malformed` | `runner_output_invalid` | internal capture panic vs contract miss — same "output unusable" surface |
| `runner_spec_invalid` | `runner_role_missing` | an absent role is an invalid runners.json |
| `learning_invalid` | `learning_kind_invalid`, `learning_usage_invalid`, `learn_add_invalid` | one malformed-learning surface |
| `issues_record_invalid` | `issues_schema_invalid` | common-field miss vs schema miss — one validated-record surface |

## The list — codes that actually surface (51)

**Store (6):** `issue_not_found`, `issue_kind_invalid`, `issues_line_invalid`,
`issues_record_invalid`, `invalid_id`, `invalid_path`
**Kernel gates & lifecycle (8):** `gate_failed`, `role_forbidden`,
`transition_not_allowed`, `set_invalid`, `field_owned_by_runtime`,
`from_file_invalid`, `checkpoint_lease_mismatch`, `profile_missing`
**Run & runner (11):** `run_not_ready_or_active`, `run_lease_held`,
`run_another_active`, `run_inconclusive`, `run_continue_without_checkpoint`,
`runner_spec_invalid`, `runner_argv_invalid`, `runner_placeholder_unknown`,
`runner_spawn_failed`, `runner_output_malformed`, `doctor_findings`
**Lanes (5):** `lane_not_verifying`, `lane_not_in_profile`,
`lane_output_invalid`, `lane_commit_mismatch`, `lane_mutated_workspace`
**Learnings (3):** `learning_invalid`, `learning_not_found`,
`learning_not_yet_helpful`
**Receipts (3):** `receipt_conflict`, `receipt_not_found`,
`artifact_missing`
**Env & identity (4):** `actor_invalid`, `host_unsupported`,
`registry_home_missing`, `pulse_md_invalid`
**Fixed variant codes (8):** `io_error`, `json_error`, `non_canonical_number`,
`unsafe_path`, `lock_timeout`, `dep_cycle`, `dep_type_invalid`,
`git_invocation_failed`
**Validation plumbing (3):** `append_line_embedded_newline`,
`json_serialize_error`, `receipt_privacy_violation`

(dep_cycle/dep_type_invalid/git_invocation_failed are `kernel(...)` codes —
listed with the fixed group for being single-purpose. The three validation
plumbing codes guard storage/serialization boundaries.)

## Result

- Surfaced codes: 68 before the audit → **51** after.
- Grep-pattern count (the old "wide" basis, incl. gate labels): 93 → 76.
- Distance to < 40: 11. The remaining codes are one-per-condition with
  mandatory hints (`run` refusals, lane seal failures, receipt conflicts);
  collapsing them is exactly the trade P1.11 rejected ("losing which
  condition failed for marginal budget gain"). Recorded as the honest
  distance; closing it further is an owner decision, not a sweep.
