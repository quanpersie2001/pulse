# Draft: `works/_drafts/schedule-crud/`

Grill and spec are finished; no graph node exists for this work yet. The three
files below are the draft's `story.md`, `approach.md` and `qa.md`.

Source product contract: `DOC-PRODUCT-SCHEDULED-EXPORTS`.

## story.md

### Outcome

A workspace admin can create, edit, pause and delete an export schedule, and a
non-admin cannot.

### Acceptance behavior

- An admin creates a schedule with a recurrence and a destination, and sees it
  listed (BR-01).
- A fourth active schedule is refused with the limit stated (BR-02).
- Destination credentials never appear in any read response (BR-04).
- A non-admin receives a permission error on every write path (BR-01).

### Out of scope

- Running the export. Delivery and run history are separate Stories.

## approach.md

### Solution

Schedules live in a new `export_schedules` table, workspace-scoped, with the
destination stored as an opaque encrypted blob written through the existing
`SecretStore` seam. The HTTP surface is four endpoints under
`/v1/workspaces/{id}/export-schedules`, authorized by the existing
`require_admin` middleware.

### Seams

- `SecretStore` already exists and already hides read-back; reuse it rather than
  adding a second secret path.
- `require_admin` middleware already enforces workspace admin; BR-01 is a
  configuration of it, not new authorization code.
- The active-schedule count for BR-02 is enforced in the repository layer inside
  the same transaction as the insert, so two concurrent creates cannot both
  pass.

### Implementation decisions

- `SecretStore` currently returns the plaintext to its caller on read. It needs a
  write-only handle so no route can read a destination secret back. This is a
  shared symbol used by fourteen call sites across three packages.
- Serialization of the recurrence rule uses the existing cron-subset parser.

### Testing decisions

- Contract tests at the HTTP layer for each acceptance behavior.
- A repository-level concurrency test for BR-02.
- A test asserting no read path serializes the destination secret.

### Out of scope

- Run execution, retries, run history.

## qa.md

Baseline owner: the Story this draft becomes. In the real `qa.md` every heading
below sits one level higher (`## Scope`, `### QA-011`); they are shifted here
only because three files share this one document.

### Scope

Schedule management stays observable to a workspace admin, and closed to
everyone else.

### Posture

automated

### Risks

- RISK-LIMIT: a fourth active schedule slips past the limit.
- RISK-LEAK: a read path serializes destination credentials.

### Exit criteria

- Every critical case passes on the candidate source.

### Cases

#### QA-011 Admin creates a schedule and sees it listed

- Intent: A created schedule appears in the admin's list (BR-01).
- Surface: api
- Priority: critical
- Steps:
  1. create a schedule as a workspace admin
  2. list schedules for that workspace
- Expected:
  - the created schedule appears with its recurrence and destination host

#### QA-012 Fourth active schedule is refused with the limit named

- Intent: The active-schedule limit holds (BR-02).
- Surface: api
- Priority: critical
- Risks: RISK-LIMIT
- Preconditions:
  - three active schedules already exist
- Steps:
  1. create a fourth schedule
- Expected:
  - the request is refused and the response states the limit

#### QA-013 Non-admin is refused on every write path

- Intent: Only an admin may change a schedule (BR-01).
- Surface: api
- Priority: critical
- Steps:
  1. attempt create, edit, pause and delete as a non-admin
- Expected:
  - each attempt is refused with a permission error

#### QA-014 No read response contains destination credentials

- Intent: Destination credentials stay write-only (BR-04).
- Surface: api
- Priority: critical
- Risks: RISK-LEAK
- Steps:
  1. create a schedule with destination credentials
  2. read it back through every read path
- Expected:
  - no response body contains the credential value

#### QA-015 Concurrent creates at the boundary leave three schedules

- Intent: The limit holds under concurrency (BR-02).
- Surface: api
- Priority: high
- Risks: RISK-LIMIT
- Preconditions:
  - two active schedules already exist
- Steps:
  1. issue two create requests concurrently
- Expected:
  - exactly three active schedules exist afterwards

## Documentation impact

Posture: required. `DOC-API-WORKSPACES` gains the four endpoints.
