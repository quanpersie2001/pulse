---
name: pulse-review
description: Perform a lane review interactively — read a `verifying` Ticket's handoff claim, re-check it yourself, and write the lane's `<role>.json` in the closed §8.4 shape into the Ticket's evidence dir. Use it when the review must happen in a session a human is watching (risk-high adversarial review, a human gate) instead of an unattended `pulse run` lane agent. Do not use it to fix what the review finds (findings become rework through `verifying -> active`), to review your own handoff (the lane actor must differ from the handoff actor), or when an unattended lane would do — then just `pulse run <lane> <id>`.
---

# Pulse Review

One lane review, performed in a supervised session, leaving exactly the
artifact the sealed-lane machinery would have left. You are acting as the
lane agent `review-correctness`, `review-adversarial`, `qa-api` or `qa-ui`
for one Ticket — decide which from the Ticket's profile, and write only
that role's file.

## 0. Preconditions

```text
pulse work show <ticket-id> --json
```

The Ticket must be `verifying` — review checks a handed-off claim. Read the
handoff first: its receipt under `.pulse/receipts/` names the commit and
the evidence dir `.pulse/evidence/<ticket-id>/`; the handoff JSON there
carries `summary`, `changed_files`, `acceptance[]`, `verify_results[]`,
`open_risks[]`. If `pulse run <lane> <ticket-id>` would do — an unattended
lane agent with a configured command — do that instead and stop. This skill
earns its session when a human needs to watch: `*-high` profiles,
adversarial review, a human gate.

You must not be the actor who handed off. A reviewer grading its own claim
is not a review; the close gate rejects the receipt if the actors match.

## 1. Verify the claim yourself

Never grade the handoff's prose — grade the tree and the commands:

- Run every `verify[]` argv from the Ticket yourself, in this repo, now.
  A result you did not produce is a claim, not a check.
- Get the diff the claim covers:
  `git diff <handoff.commit>..HEAD --stat` (and read it for
  `review-correctness`).
- For `review-adversarial`, also read the parent Story's `rules[]`,
  `exceptions[]`, the Ticket's `non_scope[]` and any `invariants` — hunt
  for what the rules imply and the diff forgot: expiry, concurrency,
  empty/oversized input, repeated submission, revocation.
- For `qa-api`/`qa-ui`, drive the Story's `qa_cases[]` for your surface
  against the running app (`docs/operations/run.md` says how it starts)
  and keep the evidence: transcripts, screenshots, console logs — write
  them under `.pulse/evidence/<ticket-id>/` and reference them from
  `cases[].artifacts`.

Edit nothing outside `.pulse/evidence/<ticket-id>/`. A review that "fixes
one small thing" invalidates the handoff's source snapshot; findings are
reports, and rework is a lifecycle decision.

## 2. Write `<role>.json`

`.pulse/evidence/<ticket-id>/<role>.json` — the shape is closed (unknown
fields fail the seal). Required: `verdict`, `environment`; the arrays
default to empty but a review with findings should carry them:

```json
{"verdict":"pass|fail|inconclusive",
 "acceptance":[{"id":"AC-1","status":"pass|fail|not_checked","how":"ran pnpm test auth"}],
 "cases":[{"id":"QA-001","status":"pass|fail|inconclusive","observation":"…",
           "artifacts":["logs/QA-001.http.txt"]}],
 "findings":[{"id":"F-1","ref":"AC-2|QA-004|-","summary":"…",
              "owner":"src/auth/errors.ts",
              "check":{"argv":["pnpm","test","auth","--","-t","revoked"],"exit":1},
              "severity":"high|medium|low","status":"open"}],
 "commands_run":[{"argv":["…"],"exit":0}],
 "environment":{"commit":"<HEAD right now>","server":null,"tool":"manual"}}
```

Seal rules you must satisfy (the sealer enforces them anyway — a file that
ignores them is a wasted review):

- `environment.commit` is the HEAD you actually checked — stale commit, no
  receipt.
- A `verdict: "fail"` whose findings carry no `check` seals as
  `inconclusive`: an unchecked finding cannot alone force rework. If you
  cannot phrase the failing behavior as one argv plus its observed exit,
  you have a suspicion, not a finding — say so in `cases[].observation`
  and stay `inconclusive`.
- `verdict: "pass"` with a failed acceptance or an open `high` finding is
  corrected to `fail` at the seal. Do not round up: `pass` means every
  acceptance you checked passed and no finding is open.
- A qa case may only be `pass` with its artifact on disk (`qa-ui`: an
  image; `qa-api`: a log/response transcript).
- Never restate the handoff's summary as your own conclusion — the receipt
  stores your file verbatim.

## 3. Hand the artifact to the sealer

The record system reads that file only through
`pulse run <role> <ticket-id>`: it snapshots the tree, executes the role's
configured command, validates `<role>.json` and seals the receipt. When the
review already happened in your session, the target repo's `runners.json`
sets that role's command to a trivial exit
(`{"command":"echo '{\"status\":\"done\"}'","timeout_seconds":60}`) — the
review is the expensive part, the seal just needs the file. A non-trivial
command means the review runs again unattended; either is fine, but decide
deliberately. If the seal rejects the file, fix the file, not the gate.

A `fail` verdict that sealed reworks the Ticket (`verifying -> active`):
the next `pulse run worker <ticket-id>` resumes from the findings.

## Report

```markdown
## Reviewed
- <ticket-id> as <role> — verdict <pass|fail|inconclusive>

## Checked
- <verify argv or QA case> — <result>

## Findings
- F-1 (high, AC-2) — <summary> — check: <argv> -> <exit> | none (inconclusive)

## Artifact
- .pulse/evidence/<ticket-id>/<role>.json — sealed via `pulse run` (yes/no)
```
