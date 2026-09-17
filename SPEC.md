# Pulse v3 — SPEC

Status: shipped, tag `v0.0.1` (owner call at close-out: a rebuild restarts the version line, it does not inherit v2's). This file describes the v3 that actually runs
and replaces `PRODUCT.md` (kept as v2 history). Plan
[`0022`](docs/plans/0022-thin-harness.md) and its decisions explain **why**;
this file says **what is**. When they disagree with the code, the code wins
and the doc gets fixed.

## 1. What Pulse is

Pulse is a local CLI truth layer for a developer working with coding agents
in one repository: a JSONL store of Epic/Story/Ticket/Decision records, a
runner that dispatches configured agent roles, an evidence gate that refuses
unverified claims, and an append-only event log. Pulse runs no agents and no
tests itself, has no daemon, and never talks to a network except the local
read-only board server (`pulse serve`). One binary, `pulse`.

Pulse is embedded in a target repo by `pulse init`; it owns `.pulse/` and
nothing else. The repo's source stays the repo's.

## 2. What `pulse init` writes into a target repo

- `.pulse/issues.jsonl` — the store (schema 3, one JSON object per line).
- `.pulse/runners.json` — role → command, timeout, output cap (human-edited).
- `PULSE.md` — profiles: `surface-risk` → required lanes, `human: required`
  for high risk, `fence_ignore`; `decision_work` profile.
- `.pulse/prompts/` — worker / worker-continue / review prompts.
- `docs/README.md` seed, `.gitignore` entries (`**/.pulse/runtime/`,
  `**/.pulse/cache/` — evidence, receipts, events, learnings are tracked).
- `scripts/qa/ui.mjs`, `api.mjs` with `--with-qa-templates`; host hook
  scripts (`.pulse/hosts/claude-code/`) with `--host claude-code`.
- Registers the repo in the user-level registry `~/.pulse/projects.json`
  (`PULSE_REGISTRY` overrides; `--no-register` opts out) — Decision 0023.
- `--refresh` re-copies templates a repo has not hand-edited.

## 3. Records (`.pulse/issues.jsonl`)

Four kinds: `epic`, `story`, `ticket`, `decision`. Common fields: `schema:
3`, 4-hex hash id (`EP-`/`ST-`/`TK-`/`DEC-`), `title`, `status`,
`revision`, timestamps, `deps[]`, `notes[]` (kinds: `note`, `friction`).

- **Story**: `outcome`, `rules[]` (BR-\*), `exceptions[]` (E-\*),
  `qa_cases[]` (QA-\* with `intent`/`surface`/`priority`/`steps`/
  `expected`, optional mechanical `check`), `open_questions[]` with
  dispositions (`resolved|rejected|delegated|deferred|blocking`).
- **Ticket**: `objective`, `change{required,invariants,docs_to_update}`,
  `acceptance[]` (when/then), `verify[]` argv, `context.anchors`
  (`path[:note]`), `non_scope`, `surface`, `risk`, `role`.
- **Statuses**: `draft → ready → active → verifying → done`, plus
  `blocked`, `cancelled`. The `ready` gate refuses: missing
  outcome/acceptance/rules/QA, `blocking` open questions, open
  `blocked_by` deps (a blocked Ticket becomes `ready` only after its
  blocker is `done`), missing or non-existent anchors, missing
  surface/risk classification.

Hand edits of `issues.jsonl` are legal — it is one human-editable file;
that is why `dep rm` has no command (undo a dep edge by editing).

## 4. Run model

`pulse run worker <id>`: acquires a lease (`ready|verifying → active`,
TTL), writes a packet (§6) to `.pulse/runtime/run/<id>/`, spawns the
`worker` role from `runners.json` with `PULSE_ACTOR=agent:worker`. The
worker loops: implement → `pulse checkpoint` (appends state + receipt) →
continue in a fresh process (`worker-continue` role) → `pulse handoff`
(seals the claim, `active → verifying`). Classification is by the worker's
**final stdout line**: one JSON object (`handed_off` / `blocked` /
`continue`); anything else is `run_inconclusive` and keeps the lease for
`pulse release` or resume. Every run — pass or fail — leaves
`.pulse/evidence/<id>/run-<role>-<n>.log` (32 KiB tail per stream behind a
key:value header; Decision 0024).

`pulse run <lane> <id>`: profile check (lane must be in the issue's
`surface-risk` profile; `--force` overrides for Tickets), source snapshot,
spawn, then validate + seal: the lane writes `.pulse/evidence/<id>/<role>.json`
(closed §8.4 shape — verdict/acceptance/cases/findings/commands_run/
environment) and Pulse seals it as a receipt, refusing a mutated workspace
(`lane_mutated_workspace`) or stale commit. `verdict: fail` reworks the
Ticket (`verifying → active`). A fail finding without a mechanical `check`
seals `inconclusive` — an unchecked finding cannot force rework alone.
`review-adversarial` and `human: required` profiles gate high-risk work
with a human in the loop.

## 5. Evidence, receipts, close

`.pulse/evidence/<id>/` holds a run's artifacts (qa transcripts,
screenshots, run logs); tracked in git. `.pulse/receipts/` is append-only
JSONL (per month) of receipts — handoff, checkpoint, lane, docs — each
naming its subject, actor, commit + dirty hash, and artifact hashes.
Unreadable receipts are reported, never erased.

`pulse close` refuses (`gate_failed`) without: a handoff receipt, unchanged
source since handoff (`close_source_stale`), every profile lane sealed
`pass` on the handoff's commit by a different actor, no open `high`
finding, and — for `human: required` profiles — explicit human approval.
`pulse close-story` unions story-scope QA coverage across passing receipts
and refuses dirty source. Done Tickets feed `pulse-learn`.

## 6. Guidance surface

- **AGENTS block** (`assets/agents-block.md`): what Pulse is, the four
  questions before mutation, route by shape (read-only / one Ticket /
  shape+plan), completion = receipt, friction → `pulse note --friction`,
  checkpoint+continue, a command table. Guard-tested: every `pulse …` in
  the block and `skills/**` parses against the real CLI.
- **Docs**: `docs/README.md` is the hand-maintained map; frontmatter
  `applies_to`/`tags` routes them. `pulse docs applicable <id>` matches a
  ticket's anchors/tags; `pulse docs check` verifies links and
  `generated_by.check_argv`.
- **Skills** (four, in `skills/`): `pulse-shape` (vague → shaped Story,
  one interview, D-ids), `pulse-plan` (Story → ready Tickets),
  `pulse-review` (supervised lane review writing the §8.4 json),
  `pulse-learn` (done Ticket → ≤1 learning candidate + ≤1 intervention,
  ladder check > template > doc > AGENTS).
- **Learnings**: `.pulse/learnings/LRN-*.md`, `candidate` → `active`
  (a handoff must record `helpful`, then a human activates) → `retired`.
  `pulse learn applicable <id>` is the packet gate; `pulse learn show`
  with no id lists.

## 7. Board

`pulse serve` — read-only local HTTP server (tiny_http, `127.0.0.1`,
default port 7777, `--open`). Lists registered projects (registry primary,
`--workspace` scan merged in, dead entries self-hide). UI: project picker,
kanban by status grouped by Story, epic filter, Ticket drawer with
Detail / Evidence / Events tabs. Absolute read-only: no write endpoint, no
lock, lenient reads (a torn line is skipped and counted, never a 500).

## 8. Health

`pulse doctor` — read-only pass reporting torn store lines, unreadable
receipts, expired leases on active Tickets, orphan evidence directories,
and the context-threshold detector's state (§10.4 marker pending /
exercised / never exercised). Exits non-zero on findings.

## 9. Errors

Every kernel/runner failure carries a code **and a mandatory hint**
(`PulseError::kernel(code, message, hint)`); the `code` field is the
contract, the message carries gate labels (`ready_blocked_by_open`,
`close_source_stale`, …). The audited list lives in
[`docs/plans/0022-error-code-audit.md`](docs/plans/0022-error-code-audit.md)
(51 surfaced codes at `v0.0.1`).

## 10. Event log

`.pulse/events/YYYY-MM.jsonl`, append + fsync, torn-tail tolerated on read
(Decision 0011). `issue.created/updated/transitioned`, `run.started/
completed`, `receipt.recorded`, notes. `pulse events tail [--id] [--follow]`
reads it; `pulse events` is deliberately read-only.

## 11. Source fence

`git` commit + dirty-hash of the working tree (`.pulse/**` and root
`PULSE.md`/`AGENTS.md` excluded; `fence_ignore` for host policy). Handoff
and every lane receipt pin it; close compares. Source changes after handoff
brick close until a re-run re-verifies — that refusal is the product.

## 12. Golden path (the acceptance test of the whole thing)

On a real target repo, through the skills, with real agents:
init → doctor clean → shape Story (ready, 2+ qa_cases) → plan Tickets
(blocked Ticket stays draft until its blocker is done) → run worker
(checkpoint ≥ 1, resume works) → run lanes → close → close-story →
learn (candidate lands in the next packet, activates after one `helpful`)
— with `review-adversarial` + a human gate on `*-high` work.

## 13. Deliberately absent (need ≥ 2 real frictions each to reconsider)

Docs search · parallel worktrees · knowledge relations · authority grants ·
receipt signatures · an MCP server · reviewer ≥ 2 by default ·
materialization. `pulse serve` left this list (Decision 0023).

## 14. Shape of the code

`src/bin/pulse.rs` (parse/run/render) → `src/cli/` (thin transport,
no domain semantics) → `src/kernel/` (composition: issues, ready, run,
lane, completion, checkpoint, packet, profile, reservation, learn, docs,
init, serve, doctor, roles) → `src/store/issues.rs` (the one JSONL store,
atomic read-validate-write) → `src/storage/` (atomic write, lock, append,
path safety — no domain knowledge) with `src/evidence/` (receipt family),
`src/runner/` (process contract: argv split, never a shell; bounded
output; final-JSON contract), `src/identity/`, `src/event.rs`,
`src/source.rs`, `src/serve/` (0023). `templates/` is everything `init`
writes (embedded via `include_str!`); `assets/` is this repo's own media;
`skills/` and `tests/` (flat crates, `tests/common/` shared helpers) sit
beside. Layers never reach up the ladder; guards pin the tree
(`tests/architecture_guards.rs`, `tests/public_api_contract.rs`).

## 15. Validation

`cargo fmt --check && cargo clippy --all-targets --quiet -- -D warnings &&
cargo test --all-targets` before every commit; the suite must pass at
default thread count (a race is fixed, never hidden). Metrics live in
[`docs/plans/0022-metrics.md`](docs/plans/0022-metrics.md).
