---
name: pulse-review
description: Perform a lane review — take the lane's bounded input from `pulse lane input`, re-check the claim yourself, write the lane's `<role>.json` in the closed §8.4 shape, and seal it with `pulse lane seal`. Use it whether you are a dispatched lane agent or reviewing in a session a human is watching (risk-high adversarial review, a human gate). Do not use it to fix what the review finds (findings become rework through `verifying -> active`), or to review your own handoff (the lane actor must differ from the handoff actor).
---

# Pulse Review

One lane review, leaving exactly the artifact the close gate reads. You are
acting as the lane `review-correctness`, `review-adversarial`, `qa-api` or
`qa-ui` for one Ticket — decide which from the Ticket's profile, and write
only that role's file.

## 0. Take the lane's input

```text
pulse lane input <ticket-id> <role>
```

That command is the precondition and the input in one: it refuses unless
the Ticket is `verifying` (review checks a handed-off claim) and the lane
belongs to the Ticket's profile, then prints the path of the only file you
should read — `objective`, `change`, `acceptance[]`, `verify[]`,
`changed_files`, `handoff_commit`, your evidence dir, plus `qa_cases[]` for
a qa lane and the Story's `rules[]`/`exceptions[]` for the adversarial one.

Read that file and the repository tree. Do **not** go looking for the
worker's handoff receipt: the input deliberately withholds its `summary`,
its `verify_results` and its checkpoints, because a reviewer that reads the
claim's prose starts grading the prose. If the input leaves you unable to
judge something, that gap is the finding.

You must not be the actor who handed off. A reviewer grading its own claim
is not a review: `pulse lane seal` refuses it, and so does the close gate.

## 1. Verify the claim yourself

Never grade the handoff's prose — grade the tree and the commands:

- Run the declared commands through Pulse, as your own actor:
  `pulse verify <ticket-id> --actor agent:<role>`. Pulse runs exactly the
  `verify[].argv` (from each entry's `cwd`) — plus the `check_argv` of every
  active learning matching the Ticket, under the name `learning.<id>` (plan
  0025 E2) — and seals a `verify` receipt with the exits and logs it
  observed. On a Ticket whose required name set (`verify[]` + applicable
  `learning.*`) is non-empty, a `pass` with no `verify` receipt of your own
  seals as `inconclusive` — a result you did not produce is a claim, not a
  check.
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
- A `pass` on a Ticket with a declared `verify[]` needs a `verify` receipt
  sealed by *you* on the fence you sealed against (`pulse verify
  <ticket-id> --actor agent:<role>`, run at step 1); without it the seal
  downgrades the verdict to `inconclusive`.
- Never restate the handoff's summary as your own conclusion — the receipt
  stores your file verbatim.

## 3. Seal it

```text
pulse lane seal <ticket-id> <role> --actor agent:<role>
```

The seal is what turns your file into evidence: it checks the tree is
unchanged since `pulse lane input` (a review that edited source is not a
review), that `environment.commit` is the commit you actually ran against,
and applies the §8.4 corrections. If the seal rejects the file, fix the
file, not the gate — the pre-run snapshot survives a rejection, so you can
seal again.

A `fail` verdict that sealed reworks the Ticket (`verifying -> active`):
the next worker session picks the findings up from its packet.

## Panel seat

A profile may declare a panel on this lane (`panels: {<lane>: {count: N,
quorum: M}}`, decision 0027). Then the lane is N independent reviewers, and
the orchestrator hands you `--seat <n>` (1-based):

```text
pulse lane input <ticket-id> <role> --seat <n>
pulse lane seal  <ticket-id> <role> --seat <n> --actor agent:<role>-<n>
```

Round 1 is blind. Write `.pulse/evidence/<ticket-id>/<role>.<n>.json`, never
the plain `<role>.json`, and do not read another seat's input or output —
if you can see it, the panel has already lost its point. Your `fail` does
not rework the Ticket by itself: the panel's verdict comes from
`pulse lane reconcile`, which the orchestrator runs once every seat has
filed. Because a finding with a `check` is arbitrated by Pulse running that
`argv`, phrase every checkable finding as a command + expected exit; that is
the one thing the other seats' votes cannot overturn.

## Round 2 (reconcile)

`pulse lane reconcile <id> <role> --prepare` writes the blind findings list
(`RF-1`, `RF-2`, …) plus `acceptance_split`; the orchestrator spawns each
seat again with `.pulse/prompts/reconcile.md`. You read **only** that file
and the tree, then write
`.pulse/evidence/<ticket-id>/<role>.reconcile.<n>.json`:

```json
{"votes": [{"rid": "RF-1", "vote": "confirmed", "how": "ran `cargo test auth`: fails at src/x.rs:41"}]}
```

Reproduce each finding; `confirmed` needs a `how` naming a command or a
`path:line`; `cannot_reproduce` is the honest answer when you cannot make it
happen (never `refuted` for that); `refuted` needs positive evidence the
finding is wrong; `duplicate` needs `of` naming the other `rid`. Vote on
what you can show, not on how confident the wording is or on a majority you
cannot see. Findings carrying a `check` are decided by the machine — your
vote is recorded, not decisive. Do **not** run `pulse lane reconcile`
yourself: the orchestrator calls it after all seats have filed.

## Report

```markdown
## Reviewed
- <ticket-id> as <role> — verdict <pass|fail|inconclusive>

## Checked
- <verify argv or QA case> — <result>

## Findings
- F-1 (high, AC-2) — <summary> — check: <argv> -> <exit> | none (inconclusive)

## Artifact
- .pulse/evidence/<ticket-id>/<role>.json — sealed (yes/no)
```
