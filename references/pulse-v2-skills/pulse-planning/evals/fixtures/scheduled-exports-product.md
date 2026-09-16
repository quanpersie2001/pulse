# Scheduled Exports

## Requirement Overview

### Destination

Workspace admins can schedule a recurring CSV export of their audit log to an
object store they own, and can see whether the last run succeeded.

### Context

Regulated customers currently ask support to pull audit logs by hand. Support
does roughly forty of these a month and each one takes an hour.

### In scope

- Daily and weekly schedules, per workspace.
- Delivery to a customer-owned S3-compatible bucket.
- A run history an admin can read.

### Out of scope

- Real-time streaming export.
- Formats other than CSV.
- Exporting anything other than the audit log.

### Glossary

- `schedule` — a workspace-scoped recurrence rule plus one destination.
- `run` — one attempt to produce and deliver an export for a schedule.

## Business Rules

- BR-01: Only a workspace admin can create, edit or delete a schedule.
- BR-02: A workspace may hold at most three active schedules.
- BR-03: A run covers exactly the period since the previous successful run's
  cutoff, so no audit event appears in two exports.
- BR-04: A schedule stores destination credentials write-only; they are never
  returned by any read path.
- BR-05: Run history retains the last ninety days of runs, with status, byte
  count and row count.
- BR-06: A failed run is retried twice with backoff before the run is recorded
  as failed.

## Exception Scenarios

- E-01
  - Trigger: destination credentials are rejected by the object store.
  - User-visible outcome: run marked failed, admin sees "Destination rejected
    the credentials" and the schedule is paused.
  - Error code: `export.destination_unauthorized`
- E-02
  - Trigger: the export exceeds the per-run size ceiling.
  - User-visible outcome: run marked failed with the observed size and the
    ceiling.
  - Error code: `export.run_too_large`
- E-03
  - Trigger: a run starts while the previous run for the same schedule is still
    in progress.
  - User-visible outcome: the new run is skipped and recorded as skipped.
  - Error code: `export.run_overlapping`

## Open Questions

### Open

- (deferred; owner: human:dana; type: grilling) Should an admin be able to
  trigger a one-off run outside the schedule? Not needed for the first release.

### Decisions so far

- DEC-014 Customer-owned buckets only for the first release — no Pulse-hosted
  storage, so no retention liability.
- Confirmed frontier, all resolved: credential storage shape (DEC-014), retry
  budget (BR-06), overlap behavior (E-03).

### Ruled out

- Per-user schedules — the audit log is workspace-scoped, so a per-user
  schedule has no meaning here.
