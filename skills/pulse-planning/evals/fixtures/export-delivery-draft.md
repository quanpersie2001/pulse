# Draft: `works/_drafts/export-delivery/`

Grill and spec are finished; no graph node exists for this work yet. The three
files below are the draft's `story.md`, `approach.md` and `qa.md`.

Source product contract: Scheduled Exports (`BR-*` and `E-*` cited below come
from it).

## story.md

### Outcome

A due schedule produces an export covering the right period, delivers it to the
customer's bucket, and records the run so an admin can see what happened.

### Acceptance behavior

- A schedule that is due runs, and covers exactly the period since the previous
  successful run's cutoff (BR-03).
- A run that starts while the previous run for the same schedule is still in
  progress is skipped and recorded as skipped (E-03).
- Rejected destination credentials fail the run, pause the schedule, and show
  the admin that the destination rejected the credentials (E-01).
- An export above the per-run size ceiling fails with the observed size and the
  ceiling (E-02).
- A failed run is retried twice with backoff before it is recorded failed
  (BR-06).
- Run history shows status, byte count and row count (BR-05).

### Open questions

- (blocking; owner: human) If delivery uploads part of the file and the
  connection drops, does that run count as failed and retried, or as succeeded
  with a partial object? This changes what run history shows.

### Out of scope

- Creating, editing or deleting schedules. That is a separate Story.

## approach.md

### Solution

A `run_exports` job polls due schedules, claims one with a row-level lock, and
walks three stages: resolve the period, stream CSV to a temporary object, then
promote the object to its final key. Each stage writes its outcome to an
`export_runs` row, so history is a projection of that table rather than a
separate log.

### Seams

- The existing `ObjectStore` trait already wraps the S3-compatible client and is
  where credential rejection surfaces; reuse it rather than calling the SDK.
- The existing `AuditLogReader` already supports a time-bounded cursor read, so
  the period resolution in BR-03 is a caller of it, not new query code.
- Overlap detection for E-03 is the row-level claim itself: a claim that finds a
  live claim records a skipped run.

### Implementation decisions

- Retry for BR-06 lives in the job's own loop, not in `ObjectStore`, so a
  credential rejection can pause the schedule without being retried as a
  transport fault.
- The temporary-then-promote shape exists so a dropped connection never leaves a
  readable partial object at the final key.

### Testing decisions

- Job-level tests for period resolution, overlap skip and retry budget.
- An `ObjectStore` fake that rejects credentials and one that refuses oversized
  objects.

### Out of scope

- Multipart upload. The size ceiling is enforced, not worked around.

## qa.md

Baseline owner: the Story this draft becomes. In the real `qa.md` every heading
below sits one level higher (`## Scope`, `### QA-021`); they are shifted here
only because three files share this one document.

### Scope

A due schedule produces a correct export, delivers it, and records what
happened.

### Posture

automated

### Risks

- RISK-DOUBLE: an audit event appears in two exports.
- RISK-OVERLAP: two runs for one schedule execute at once.

### Exit criteria

- Every critical case passes on the candidate source.

### Cases

#### QA-021 A due run covers the period since the last cutoff

- Intent: No audit event appears in two exports (BR-03).
- Surface: job
- Priority: critical
- Risks: RISK-DOUBLE
- Preconditions:
  - one schedule with a previous successful run
- Steps:
  1. run the job for that schedule
- Expected:
  - the export covers events after the previous cutoff and no earlier

#### QA-022 An overlapping run is skipped and recorded

- Intent: A second concurrent run is skipped (E-03).
- Surface: job
- Priority: critical
- Risks: RISK-OVERLAP
- Preconditions:
  - one run for the schedule is in progress
- Steps:
  1. start a second run for the same schedule
- Expected:
  - the second run is recorded skipped with `export.run_overlapping`

#### QA-023 Rejected credentials fail the run and pause the schedule

- Intent: Credential rejection is visible and stops the schedule (E-01).
- Surface: job
- Priority: critical
- Steps:
  1. run a schedule whose destination rejects the credentials
- Expected:
  - the run is failed with `export.destination_unauthorized`
  - the schedule is paused

#### QA-024 An oversized export fails with the observed size

- Intent: The size ceiling is enforced (E-02).
- Surface: job
- Priority: high
- Steps:
  1. run a schedule whose export exceeds the ceiling
- Expected:
  - the run fails with `export.run_too_large`, the observed size and the ceiling

#### QA-025 A failed run is retried twice before it is recorded failed

- Intent: The retry budget holds (BR-06).
- Surface: job
- Priority: high
- Steps:
  1. run a schedule whose delivery fails transiently three times
- Expected:
  - two retries occur with backoff, then the run is recorded failed

#### QA-026 Run history shows status, bytes and rows

- Intent: History is readable by an admin (BR-05).
- Surface: api
- Priority: normal
- Steps:
  1. read run history for a schedule with one successful and one failed run
- Expected:
  - both runs appear with status, byte count and row count

## Documentation impact

Posture: required. `DOC-API-WORKSPACES` gains the run-history read path, and the
three new error codes join the failure taxonomy.
