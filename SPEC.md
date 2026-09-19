# Pulse — SPEC

Status: rewritten from the code on branch `features/harness-experimental`
after plan [`0025`](docs/plans/0025-parallel-verified-learning.md) shipped
phases A–F and G1–G2 (G3 and E5 deferred). This file describes **what runs
today**. Plans and decisions in `docs/` explain **why**; when they disagree
with the code, the code wins and this file gets fixed.

## 1. What Pulse is

Pulse is a local CLI truth layer for a developer working with coding agents
in one repository: a JSONL store of Epic/Story/Ticket/Decision records,
gates that bracket the work (`claim`/`handoff` for a worker, `lane input`/
`lane seal` for a review or qa lane), an evidence gate built on receipts,
and an append-only event log. One binary, `pulse`.

What Pulse is **not**:

- It dispatches nothing. The host spawns agents; Pulse decides what counts.
- It has no daemon and no background process. One command = one process.
- It runs no agents. The only processes it ever spawns are argv that a
  record declares and that only a `human:` actor can author (decision 0026):
  a Ticket's `verify[]`, an `active` learning's `check_argv`, a finding's
  `check.argv`, a doc's `generated_by.check_argv`. Pulse records what it
  observed; it never chooses what to run.
- It never talks to a network except `pulse serve`, a read-only HTTP server
  bound to `127.0.0.1` (Decision 0023).

Pulse is embedded in a target repo by `pulse init`; it owns `.pulse/`, the
`PULSE.md` profile file and the marked Pulse block in `AGENTS.md`, and
nothing else. The repo's source stays the repo's.

## 2. Data model

### 2.1 The store

`.pulse/issues.jsonl` is the one store: schema `3`, one JSON object per
line, validated against an embedded JSON-schema (every object rejects
unknown fields) on every read and write. Hand edits are legal — it is a
single human-editable file; that is why `dep` has no remove command (undo a
dep edge by editing). Every mutation takes the repository write lock,
re-reads, validates every record, writes atomically (records sorted by id),
and emits exactly one event.

Common fields: `schema: 3`, `id` (`EP-`/`ST-`/`TK-`/`DEC-` + 4 hex chars),
`kind`, `title`, `status`, `revision`, `created_at`, `updated_at`;
optional `tags[]`, `deps[]` (`{type: blocked_by|supersedes, id}`), `notes[]`
(capped at 50 on the record — older entries are drained; the durable copy of
every note is its `note.recorded` event; `kind: note|friction`).

### 2.2 Story

`outcome`, `rules[]` (`BR-*` id + text), `exceptions[]` (id + text),
`qa_cases[]` (free-form case objects; gates read `id`, `surface`,
`priority` — `high` matters at close-story), `open_questions[]` (`q` +
`disposition: resolved|rejected|delegated|deferred|blocking`),
`docs_written[]` (docs where rules/exceptions live, read by close-story),
`approach`, `success_signals[]`, `out_of_scope[]`, `epic`.

### 2.3 Ticket

`story`, `role` (`implementation` | `decision_work`), `risk`
(`low|medium|high|null`), `surface` (`cli|api|ui|lib|docs|null`),
`objective`, `description` (free-form markdown: the *how*, for a worker
with no other context), `touches[]` (files it will edit or create, as
repo-relative globs — the parallel-claim key, decision 0025),
`context.anchors[]` (`path[:note]` — places to *read*; the ready gate
checks they exist), `context.docs[]`, `context.decisions[]`,
`change.required[]`/`invariants[]`/`docs_to_update[]`, `non_scope[]`,
`acceptance[]` (`id`, `when`, `then`), `verify[]` (`{name, argv[], cwd?}` —
what `pulse verify` runs), `qa_cases[]` (`QA-<n>` ids from the Story).
`decision_work` tickets instead carry `question` and `deliverable`.

Runtime-owned fields (written by gates, refused by `pulse work update`):
`lease` (`{role, actor, run_id, expires_at}`), `verdicts` (per-lane
`{receipt, verdict, commit}`), `checkpoints[]` (last 10 kept; older ones
archive to `.pulse/evidence/<id>/checkpoint-<n>.json`).

### 2.4 Epic and Decision

Epic: `outcome`, `out_of_scope[]`. Decision: `question`, `options[]`,
`decision`, `consequences`, `accepted_by`, `accepted_at`; `context` as a
markdown string.

### 2.5 `.pulse/` layout

| Path | Durable? | Contents |
|---|---|---|
| `.pulse/issues.jsonl` | tracked | the store |
| `.pulse/receipts/YYYY-MM.jsonl` | tracked | receipt ledger, one canonical JSON line each, append + fsync |
| `.pulse/events/YYYY-MM-DD.jsonl` | tracked | event log, append + fsync |
| `.pulse/evidence/<id>/` | tracked | run logs, verify logs, lane outputs, screenshots, checkpoint archives, reconcile check logs |
| `.pulse/learnings/LRN-*.md` | tracked | learning files (frontmatter + markdown body) |
| `.pulse/prompts/*.md` | tracked | worker/review/reconcile prompts `pulse init` seeds |
| `.pulse/base/` | tracked (never gitignored) | as-shipped copies of template-written files, the merge base for `--refresh` |
| `.pulse/runtime/` | gitignored | lane inputs + pre-run snapshots, reconcile prepare files, refresh conflict/kept artifacts, verify scratch |
| `.pulse/cache/` | gitignored | nothing writes it in v3; the ignore entry is kept |

Plus, at the repo root: `PULSE.md` (profile, human-edited) and the
`<!-- PULSE:BEGIN/END -->` block inside `AGENTS.md`.

## 3. Lifecycle

```
              work ready            claim                handoff            close
  draft ────────────────► ready ──────────► active ──────────► verifying ──────► done
    │                       ▲                │  ▲                  │               story:
    │ work transition       │                │  │ lane/reconcile   │ re-claim      close-story
    ▼ cancelled      release┘                │  └──────────────────┘               ──────►
    ▲                                        │
    │ work transition        work transition │ (blocked)
    blocked ◄────────────────────────────────┘
```

- `draft → ready` only through the ready gate; `ready → active` through
  `pulse claim` (which takes the lease — and also resumes an `active`
  ticket, extending the lease, or re-claims a `verifying` one: fresh
  lease, back to `active`, next handoff refreshes the snapshot).
- `active → verifying` through `pulse handoff` (lease dropped, handoff
  receipt sealed); `verifying → done` only through `pulse close`; a Story
  only through `pulse close-story`.
- `verifying → active` (rework): a sealed non-seat lane receipt with
  `verdict: fail`, or a reconciled panel verdict of `fail`.
- `active → ready`: `pulse release` — by the holder, by any `human:`
  actor, or by anyone once the lease expired.
- Manual `pulse work transition`: `draft|ready|blocked → cancelled` and
  `ready|active → blocked`; everything else is refused
  (`transition_not_allowed`).

## 4. Gates

Every gate collects **all** violations into one report — never just the
first — surfaced as a single `gate_failed` kernel error whose message lists
`code: message` per violation. The per-condition codes below are the
contract; each has a test aimed at it.

### 4.1 Ready gate (`pulse work ready`)

For an `implementation` Ticket, all eight conditions are checked:

| # | Condition | Violation code |
|---|---|---|
| 1 | `acceptance[]` non-empty, unique ids, non-empty when/then | `ready_acceptance_missing` |
| 2 | every `context.anchors` path exists on disk | `ready_anchor_missing` |
| 3 | no open question is `blocking` or missing a disposition | `ready_question_blocking` |
| 4 | every `blocked_by` dep is `done` or `cancelled` (and exists) | `ready_blocked_by_open` |
| 5 | every `qa_cases[]` id is defined on the ticket's Story | `ready_qa_case_unresolved` |
| 6 | `risk` and `surface` are both set (not null) | `ready_classification_missing` |
| 7 | `description` non-empty (medium/high risk only) | `ready_description_missing` |
| 8 | `touches[]` non-empty and every entry a safe repo-relative glob (medium/high risk only; existence is never checked — tickets create files) | `ready_touches_missing` |

A `decision_work` ticket checks only conditions 3–4 plus a non-empty
`question` (reported as `ready_acceptance_missing`). A Story checks
condition 3, a non-empty `outcome` (`ready_outcome_missing`), and at least
one rule or qa_case (`ready_rules_or_qa_missing`). A Story's own
`blocked_by` deps are **not** checked by the gate — only a Ticket's are.

### 4.2 Claim / release / reserve (`kernel::reservation`)

A ticket's files are **held** while it is `verifying`, or `active` with a
live lease. A ticket with no `touches` overlaps everything: it is
exclusive.

| Command | Refusal | Code |
|---|---|---|
| `claim` | ticket not `ready`/`active`/`verifying` | `run_not_ready_or_active` |
| `claim` | another actor's live lease on this ticket | `run_lease_held` |
| `claim` | the calling actor holds a live lease on a *different* ticket — one actor, one live claim | `claim_actor_busy` |
| `claim` | this ticket's `touches` overlap files held by any other ticket | `claim_files_reserved` |
| `reserve` | caller does not hold the ticket's live lease on an `active` ticket | `reserve_lease_mismatch` |
| `reserve` | empty path list, or any path absolute/escaping/empty | `reserve_paths_invalid` |
| `reserve` | a new path overlaps files another ticket holds | `claim_files_reserved` |
| `release` | live lease held by another actor and caller is not `human:` | `release_not_holder` |

`pulse reserve <id> <paths…>` appends to `touches` (deduplicated) mid-run
and emits `lease.reserved`; reserving on a touches-less (exclusive) ticket
*narrows* its claim to exactly the named files. Refusals name the blocking
ticket, its status and the pattern.

### 4.3 Handoff (`pulse handoff --from handoff.json`)

Input shape (`deny_unknown_fields`): `run_id` (required), `summary`,
`changed_files[]`, `acceptance[]` (`{id, status, how}`), `verify_results[]`
(compatibility only — no longer read), `docs_updated[]`,
`learnings_used[]` (`{id, usage: helpful|not_needed|misleading}`),
`friction[]` (become friction notes), `open_risks[]`.

| Condition | Violation code |
|---|---|
| ticket is `active` | `handoff_not_active` |
| lease actor is the calling actor **and** `run_id` matches the lease | `handoff_lease_mismatch` |
| every ticket acceptance id appears in the handoff | `handoff_acceptance_missing` |
| every handed acceptance has `status: "done"` | `handoff_acceptance_not_done` |
| ticket declares `verify[]` (or a learning check applies): a `verify` receipt exists | `handoff_verify_missing` |
| …its fence matches the current fence (`profile::same_fence`) | `handoff_verify_stale` |
| …every declared name is present with `exit == 0` | `handoff_verify_failed` |
| every `change.docs_to_update` path is in `docs_updated` | `handoff_documentation_missing` |
| …and git actually sees a change to it | `handoff_documentation_not_diffed` |
| every `learnings_used` id exists | `learning_unknown` |
| every usage is `helpful|not_needed|misleading` | `handoff_learning_usage_invalid` |
| when the ticket has `touches`: every dirty path (after `fence_ignore`) is covered by the `touches` of *some* `active`/`verifying`/`done` ticket — nothing edited outside every reservation | `handoff_unreserved_changes` |

Clean: a `handoff` receipt (source = the ticket's fence), status
`verifying`, lease dropped, friction lines become notes, each
`learnings_used` bumps the learning's usage counter.

### 4.4 Lane input / seal (`pulse lane input`, `pulse lane seal`)

`lane input` writes `.pulse/runtime/lane/<id>/<role>[-<seat>]-input.json` —
the bounded claim a lane may see (identity, objective, description,
change, acceptance, `verify[]`, non_scope, changed files since the handoff
commit filtered to the ticket's `touches`; Story rules/exceptions for
`review-adversarial`; resolved qa_cases and viewports for `qa-*`) — plus a
pre-run source snapshot, and emits `run.started`. A seat's input is
byte-identical to any other seat's: round 1 is blind.

| Refusal | Code |
|---|---|
| Ticket subject is not `verifying` (a Story needs `surface`/`risk` set instead) | `lane_not_verifying` / `profile_missing` |
| lane not in the record's profile (and no `--force`) | `lane_not_in_profile` |
| panel role run without `--seat <n>` | `lane_seat_required` |
| `--seat` without a panel, outside `1..=count`, or combined with `--force` | `lane_seat_invalid` |

`lane seal` (lane role or human) reads the snapshot
(`lane_not_prepared`), then under one lock:

| Condition | Code |
|---|---|
| the latest handoff was sealed by this actor — no self-review | `lane_actor_not_independent` |
| the tree's fence `dirty_hash` changed since the pre-run snapshot | `lane_mutated_workspace` |
| `.pulse/evidence/<id>/<role>[.<seat>].json` exists and parses as `LaneOutput` (`verdict`, `acceptance[]`, `cases[]`, `findings[]` with optional `check {argv, exit}`, `commands_run[]`, `environment.commit`) | `lane_output_invalid` |
| `environment.commit` equals HEAD — the lane names the commit it ran on | `lane_commit_mismatch` |
| a seat's actor has not sealed another seat of the same panel round | `lane_seat_actor_reused` |

Seal-time corrections (`apply_seal_corrections`), applied before the
receipt is written: a `qa-ui` case `pass` with no existing
`.png`/`.jpg`/`.jpeg` artifact in evidence → `inconclusive` (`qa-api`
likewise without any artifact); `pass` with any `inconclusive` case →
`inconclusive`; a `review-*` `pass` on a ticket declaring `verify[]`,
without a passing `verify` receipt sealed by *this same actor* on the
current fence → `inconclusive`; `pass` with a failed acceptance or an open
`high` finding → `fail`; `fail` with no finding carrying a `check` →
`inconclusive` (an unchecked fail cannot force rework alone).

The receipt is `kind: "lane"` (or `kind: "lane_seat"` for a seat; seats
never write `verdicts` and never bounce the ticket). A plain lane receipt
writes `verdicts[role]` on the record; `fail` bounces the ticket to
`active`. A corrected verdict emits `receipt.recorded` with
`lane_verdict_corrected: true`.

### 4.5 Review panel: seats + `pulse lane reconcile` (decision 0027)

A profile may declare `panels: {<role>: {count, quorum}}`. The lane then
runs `count` times as independent seats (own `--seat`, own actor —
convention `agent:<role>-<n>`), each sealing a `lane_seat` receipt keyed
to the panel round (the latest handoff receipt id, else `head:<commit>`).
`pulse lane reconcile <id> <role> --prepare` refuses until all seats are
sealed (`reconcile_seats_missing`), then writes a blind round-2 input:
every seat's findings anonymized, deterministically sorted, renamed
`RF-1..n`, plus `acceptance_split`. Each seat files
`.pulse/evidence/<id>/<role>.reconcile.<n>.json` with votes `confirmed`
(needs `how`) / `refuted` / `duplicate` (`of: RF-x`) / `cannot_reproduce`.
`pulse lane reconcile` (no `--prepare`) then, per finding:

| Rule | Outcome |
|---|---|
| finding has `check.argv` → Pulse runs it (`--timeout`, default 900s), log to `.pulse/evidence/<id>/reconcile/<rid>.log` | observed `exit == check.exit` → `resolved`; anything else → stays `open` at original severity. **Votes cannot overturn this.** |
| no check → supporters (the raiser ∪ seats voting `confirmed`) ≥ `quorum` | stays `open` |
| no check and fewer supporters | `status: unconfirmed`, `severity: low` |
| `duplicate` merged into a root when ≥ `quorum` seats say so | votes roll up to the root finding |

Acceptance: `pass` when ≥ `quorum` seats reported pass, `fail` when more
than `count − quorum` reported fail, else `not_checked`. Raw verdict is
`fail` on any failed acceptance or standing `high` finding, else `pass`;
the seal-time corrections then apply (verify-receipt rule satisfied). The
single `lane` receipt carries `reconciled: true`, `seats`, `handoff`,
`votes_summary` (per-finding tallies plus `missing`/`invalid` vote files —
a dead seat weakens the result, it never breaks the command). `fail`
bounces the ticket to `active`. A `check.argv` that cannot run logs
`<check not runnable: …>` and counts as standing.
### 4.6 Close (`pulse close`)

| Condition | Violation code |
|---|---|
| ticket is `verifying` | `close_not_verifying` |
| a handoff receipt exists | `close_handoff_missing` |
| the ticket's current fence equals the handoff receipt's fence (scoped: `dirty_hash` only; whole-tree: commit + hash) | `close_source_stale` |
| every lane in the ticket's `<surface>-<risk>` profile has a receipt: `verdict: pass`, sealed on the handoff's fence, by an actor other than the handoff actor, with no unresolved `high` finding; for a panel role, a *reconciled* receipt matching the current handoff round | `close_lane_not_satisfied` |
| `human: required` profiles are closed by a `human:` actor | `close_human_required` |
| no open question is blocking | `close_question_blocking` |

Clean → `close` receipt, `verifying → done`, `issue.transitioned` event.
The CLI adds `unclassified_friction` to the output as a warning — it never
blocks a Ticket close.

### 4.7 Close-story (`pulse close-story`)

| Condition | Violation code |
|---|---|
| every child ticket is `done` or `cancelled`, and at least one is `done` | `close_story_children_incomplete` |
| no unclassified friction on the Story or any child (classified = cited by a learning's `from` or explained by a `friction.dismissed` event) | `close_story_friction_unclassified` |
| a Story with rules/exceptions lists `docs_written[]`; each path is safe, under `docs/`, exists; every rule/exception id appears verbatim in at least one listed file | `close_story_docs_missing` |
| the **whole tree** is clean (`source::snapshot`, not the scoped fence — a story milestone commits) | `close_story_source_dirty` |
| every `priority: high` qa_case is covered by a `pass` case in some passing story-scope `qa-*` lane receipt sealed on the current HEAD | `close_story_qa_not_satisfied` |

## 5. Source fence

Pulse pins workspace state with a `Source {commit, dirty_hash,
dirty_paths}`. Two snapshots exist:

- **Whole tree** (`source::snapshot`): `commit` = HEAD; `dirty_paths` =
  `git status --porcelain` minus fenced-out paths; `dirty_hash` = sha256
  over each path's `git diff --binary HEAD` output (full bytes for
  untracked files) — `sha256:…`.
- **Scoped** (`source::scoped_snapshot`, tickets with `touches`): hashes
  exactly the files matching `touches` (tracked ∪ untracked, sorted), as
  `path \0 bytes \0` with a `<deleted>` tombstone — `scope:sha256:…`, a
  prefix that can never equal a whole-tree hash. `commit` is recorded for
  reading but **never compared**: one ticket's close survives another
  ticket's commit landing first.

`profile::fence_for` picks per record (`touches` non-empty → scoped,
else whole tree); `profile::same_fence` compares commit + hash for
whole-tree records, hash only for scoped ones. Gates and `lane seal`
compare through these two functions, never by hand.

Always fenced out (never hashed, never a violation): `.pulse/**`, root
`PULSE.md`, root `AGENTS.md`, plus anything matching `fence_ignore` globs
from `PULSE.md`.

The glob grammar (`source::glob_match`) is deliberately small: an exact
path, `dir/` (everything under it), `dir/**`, or one `*` within a single
segment. Not `**` in the middle, no character classes.

## 6. Parallel work

- **Frontier** (`pulse frontier [story]`, read-only, deterministic):
  candidates are `ready` tickets sorted by id. Each candidate is classified
  `waiting` with `reason: blocked_by` (open dep), `reserved` (overlaps a
  currently held ticket), or `frontier` (overlaps an already-accepted
  runnable ticket — the greedy pass serializes overlapping tickets, smaller
  id wins), else `runnable` with `{id, title, surface, risk, touches}`.
  `held[]` lists what is held right now. The host re-asks between spawns.
- **Files are held from claim to done**, review included (decision 0025 §5):
  close compares the handoff fence, so a mid-review edit by another ticket
  would stale it forever without a nameable cause.
- **One worker, one actor**: `claim_actor_busy` forces parallel workers
  onto distinct actors (`agent:worker-1`, `agent:worker-2`, …).
- **Commit after close**: the host convention (AGENTS block) is
  `git add -- <touches> && git commit` immediately after `pulse close`, so
  the next frontier is not fenced in by uncommitted work. `pulse doctor`
  reports dirty paths whose only covering tickets are all `done` as
  `awaiting_commit`.
- **The pre-edit hook** (`pulse hook pre-edit`, plan 0025 G1) is where a
  reservation binds on an edit that never runs a `pulse` command. The
  decision is made from the **path alone** (identity only sharpens two
  outcomes; the shipped snippet passes no actor). Rules, in order:

  | # | Path situation | Decision |
  |---|---|---|
  | 1 | not inside the repo, or repo not enrolled (no `.pulse/issues.jsonl`) | allow |
  | 2 | fenced out (`.pulse/`, `PULSE.md`, `AGENTS.md`, `fence_ignore`) | allow — except a lane actor may only write under `.pulse/evidence/` |
  | 3 | any other path, lane actor | deny: lanes review, they do not edit source |
  | 4 | no ticket `active` (live lease) | `hook.unclaimed` policy: `allow` (default) or `deny` ("claim a ticket first") |
  | 5 | an active ticket without `touches` holds the tree | allow for its holder; deny for anyone else ("holds the whole tree exclusively") |
  | 6 | inside an active held ticket's `touches` | allow for its holder; deny for anyone else, naming the holder |
  | 7 | inside a `verifying` ticket's `touches` | deny: under review |
  | 8 | everything else | deny with the `pulse reserve <id> <path>` hint |

  CLI contract: allow exits `0` silently; deny exits `2` with the reason on
  stderr (one deny wins over any allow); internal failure (torn store,
  broken `PULSE.md`) exits `1` — a torn store must never lock every edit.
  `--path` repeats; `--stdin-json` extracts paths from host payloads
  (Claude Code's `tool_input.file_path`/`path`/`notebook_path`, Codex
  apply-patch `*** Add/Update/Delete File:` lines, `cwd`-relative
  resolution); absolute paths are matched against both the given and the
  canonicalized repo root (macOS `/private` prefix). `pulse hook snippet
  claude` prints the PreToolUse configuration for pasting; Pulse never
  writes a host settings file, and any other host is `hook_host_unknown`.
  `pulse init` prints one hint line pointing at it.

  Known bypass, stated plainly: a file written through a shell
  (`sed -i`, `>`) never crosses the hook. `handoff_unreserved_changes`
  (§4.3) is the second net.

## 7. Evidence

### 7.1 Receipts

`.pulse/receipts/YYYY-MM.jsonl`, one canonical JSON line per receipt,
append + fsync. Envelope: `id` (ULID), `kind`, `subject {id, revision?}`,
`actor`, `source {commit, dirty_hash}`, `recorded_at`, `run_id?`, `payload`,
`artifacts[]` (`{path, sha256}` — hashed at record time; a missing file
refuses the receipt with `artifact_missing`). Sealing the same id with the
same content is idempotent; with different content, `receipt_conflict`.
Unreadable lines are reported by `list_receipts`/`pulse doctor`, never
erased (Decision 0017).

Kinds, and who seals them: `checkpoint` (worker), `handoff` (worker),
`verify` (`pulse verify`), `lane` / `lane_seat` (`pulse lane seal`,
`pulse lane reconcile`), `close`, `close_story`.

### 7.2 Redaction (the tracked-plane boundary)

Before a receipt payload is written, every string goes through
`clean_json_strings`: the five secret patterns (AWS access key, GitHub
token, `sk-` API key, private-key block, Bearer token) are **refused**
(`receipt_privacy_violation` — the receipt is not written), and any string
that *starts like* an absolute path must canonicalize inside the repo or is
refused; in-repo absolute paths are rewritten repo-relative. A verify log
redaction refuses is replaced by `<log withheld: redaction refused it>` —
the **exit code survives**, the log is what is lost (decision 0026 records
the sharp edge: a log beginning with a path is withheld wholesale).

### 7.3 Verify logs

`pulse verify` writes `.pulse/evidence/<id>/verify/<name>.log` per declared
command: the merged stdout+stderr tail, last 32 KiB, cut at a UTF-8
boundary (the same bound as Decision 0024's run logs). Re-running
overwrites the log by design — the previous receipt still carries the old
log's sha256, so it stays self-describing.
### 7.4 Event log

`.pulse/events/YYYY-MM-DD.jsonl` (UTC day files), append + fsync, torn
tails reported not fatal; read back sorted by ULID id (= chronological).
Event types emitted by v3 code, all of them:
`issue.created`, `issue.updated`, `issue.transitioned`,
`run.started`, `run.completed`, `note.recorded`, `receipt.recorded`,
`verify.recorded`, `lease.reserved`, `checkpoint.recorded`,
`learning.added`, `learning.retired`, `friction.dismissed`,
`docs.maybe_stale`. `pulse events tail [--since <cursor>] [--id <id>]
[--follow]` reads it; the events surface is read-only. Legacy v2
one-file-per-event layouts are still read.

## 8. Identity and authority

An actor is the string `kind:id` with kind `human|agent|system`. It is
**self-declared**: `--actor`/`--from` wins, then `PULSE_ACTOR`, then
`git config user.name` as `human:<name>` (the hook never falls back to git
config — a hook that guessed "human" would make the actor rules
meaningless). What the system *does* enforce despite self-declaration:
`run_id` must match the lease at `checkpoint`/`handoff`; one actor cannot
hold two live claims (`claim_actor_busy`); a lane cannot be sealed by the
actor that handed off (`lane_actor_not_independent`); panel seats in one
round cannot share an actor (`lane_seat_actor_reused`); every receipt and
event records who claimed to do it.

What is *not* enforced: that two `--actor` strings name two real sessions.
Deliberate spoofing is a host-hook problem (decision 0026 "Không giải
quyết"), not a repo rule.

The authorization matrix (`kernel::roles`, fixed `match`, no config):

| Action | human | agent worker-ish | agent `review-*`/`qa-*`/`check-*` | system |
|---|---|---|---|---|
| MutateGraph (new/update/ready/dep/transition, learn activate/retire) | ✔ | ✘ | ✘ | ✘ |
| CheckpointOrHandoff (checkpoint, handoff, reserve) | ✔ | ✔ (non-lane ids) | ✘ | ✘ |
| LaneReceipt (lane seal, reconcile prepare+seal) | ✔ | ✘ | ✔ | ✘ |
| Close (close, close-story) | ✔ | ✘ | ✘ | ✘ |
| NoteOrLearnAdd (note, learn add, learn dismiss) | ✔ | ✔ | ✔ | ✘ |
| Verify | ✔ | ✔ | ✔ | ✘ |

An "agent worker-ish" id is any agent id that does not start with
`review-`/`qa-`/`check-`.

## 9. The learning loop

1. **Friction.** `pulse note <id> "…" --friction` appends a friction note
   (capped at 50 on the record) and a `note.recorded` event. The friction's
   stable key is the **event id** (`<subject>#evt_<ulid>`) — notes drain
   off the record, events never do (plan 0025 E1 as executed). A friction
   is `unclassified` until a learning's `from` cites
   `<subject>#<evt-id>` or a `friction.dismissed` event explains it.
   `pulse learn friction [id] [--all]` lists states;
   `pulse learn dismiss <subject> [keys…] --all --reason` records
   dismissals (any actor may dismiss — a dismissal is a classification,
   not an erasure; idempotent).
2. **Learn.** `pulse learn add` (any actor) creates a `candidate`
   `.pulse/learnings/LRN-*.md` — kinds `failure|constraint|technique|
   routing`; `--friction <subject>#<evt>` cites what it classifies;
   `--cite <path>:<from>-<to>` pins the code it is about (Pulse hashes the
   lines itself); `--check-argv '["…"]'` (+ `--check-cwd`) declares a
   mechanical check.
3. **Activate.** `pulse learn activate` is human-only (`MutateGraph`) and
   requires `usage.helpful ≥ 1` (`learning_not_yet_helpful`). It flips the
   frontmatter status and writes the file; it emits no event
   (`learning.added`/`learning.retired` are the logged transitions).
4. **Enforce.** `pulse verify` runs, after the ticket's own `verify[]`,
   every `active` learning matching the ticket's anchors/tags (glob on
   anchor paths, exact on tags; suspects excluded) that has `check_argv`,
   named `learning.<LRN-id>`, cwd = `check_cwd`. Its result is an ordinary
   verify result: a non-zero exit blocks handoff via
   `handoff_verify_failed`.
5. **Usage.** `handoff.learnings_used` bumps `helpful|not_needed|
   misleading`.
6. **Retire.** A learning with `misleading > helpful` is *suspect*: excluded
   from enforcement and recall, listed by `pulse doctor`
   (`learning_suspects`) for a human to `pulse learn retire --reason`.
7. **Stale cites.** A `cites` entry whose recorded sha256 no longer matches
   the file is reported (`pulse doctor` `stale_cites`, `stale: true` in the
   packet) — drift is surfaced, never auto-retired.

Recall for the packet: `applicable` = active (+ candidates for display)
sorted by `helpful` then id, capped at 5 (`RECALL_LIMIT` — display only;
enforcement is not capped).

`pulse metrics` is read-only and deterministic; `--since` (RFC3339 or date)
narrows event- and receipt-derived counts: `tickets_done`,
`friction_per_ticket_done`, `friction_unclassified`, `rework_rate` (tickets
with a `issue.transitioned {reason: rework}` / done tickets),
`lane_verdicts` (pass/fail/inconclusive from `lane` receipts),
`lane_verdict_corrected`, `verify_runs`, `panel` (reconciles, findings by
open/unconfirmed/resolved), `median_claim_to_done_minutes` (`run.started
{run_id}` → `issue.transitioned {to: done}`), `learnings` (status counts,
suspect, stale_cites, enforced, usage totals). Seven metrics plan 0022
hand-measured are reported in `not_derivable` with the reason (§12).

## 10. Docs

Doc frontmatter (optional): `applies_to[]` — **"which code this doc
describes"** (plan 0025 F3's meaning, not "who should read this"),
`tags[]`, `generated_by {argv, check_argv}` for generated docs.

- **`docs_to_update` gate** (§4.3): declaring a doc in
  `change.docs_to_update` requires listing it in `docs_updated` at handoff
  *and* git seeing the diff.
- **`pulse docs check`**: broken relative links inside `docs/`, paths
  referenced by `docs/README.md` that don't exist, and
  `generated_by.check_argv` runs whose real exit differs from the declared
  one. Findings are `medium`; `verdict: fail` iff any non-low finding
  exists; exits `1` on fail — unless `--write <path>`, which writes the
  lane-shape report and prints `{"status":"done"}`: **that is the whole
  `check-docs` lane**, no agent needed
  (`pulse lane seal <id> check-docs` after).
- **`docs check --ticket <id>`**: adds one **low**-severity finding per doc
  that may have been staled by the ticket's edits (a doc whose
  `applies_to` matches changed files, itself unchanged, and no
  `generated_by`). Low findings never flip the verdict or the exit code.
  The same advisory is computed at `pulse handoff`: `docs_maybe_stale` in
  the JSON output, a `docs.maybe_stale` event, and a human-readable hint.
- **`pulse docs applicable <id>`** is a *hint*: anchor paths matched
  against `applies_to` globs, tags matched exactly, capped at 8. The
  worker greps/globs `docs/` first; this command is never the search
  engine (plan 0025 F1 demoted it).

## 11. Enrollment, profile, health

### `pulse init` (and `--refresh`)

Writes, never overwriting an existing file: `.pulse/issues.jsonl` (empty),
`PULSE.md`, `docs/README.md`, `docs/operations/run.md`, the Pulse block
inside `AGENTS.md` (between `<!-- PULSE:BEGIN/END -->` markers), and
`.pulse/prompts/{worker,review-correctness,review-adversarial,reconcile}.md`.
Appends `**/.pulse/runtime/` and `**/.pulse/cache/` to `.gitignore`.
Registers the repo in `~/.pulse/projects.json` (`PULSE_REGISTRY` overrides
the path; `--no-register` opts out — Decision 0023). `--with-qa-templates`
copies `scripts/qa/{ui,api}.mjs` + `README.md`, skipping files that exist.

Every template-written file keeps its as-shipped copy in `.pulse/base/`.
`--refresh` re-renders the refreshable units (the four prompts and the
AGENTS block region — never the rest of `AGENTS.md`, never `PULSE.md`,
never QA scripts) through a three-way merge (`git merge-file`):

| Situation | Outcome |
|---|---|
| file absent | `created` |
| local == new template | `unchanged` |
| local == base (user never edited) | `updated` — template wins |
| local ≠ base, clean merge | `merged` — both sides land |
| local ≠ base, conflicts | `conflict` — the user's file is untouched; the marker version is filed under `.pulse/runtime/refresh/`; exit still 0 |
| no base (enrolled by an older Pulse) | `kept` — the user's file stays, the new template is filed as `.new` beside it |

A `kept` file is resolved later with `--refresh --take-new <file>`
(template wins, base advances) or `--keep-mine <file>` (user keeps, base
advances so the next refresh merges for real); an unknown file is
`init_refresh_unknown_file`.

### `PULSE.md`

YAML, human-edited: `fence_ignore[]` globs; `profiles` keyed
`<surface>-<risk>` (resolved from a record's own fields; `decision_work`
for decision-work tickets) each with `lanes[]` and optional
`human: required` and `panels: {<role>: {count, quorum}}` (validated at
load: `count ≥ 2`, `1 ≤ quorum ≤ count`, role in the same profile's
`lanes` — else `pulse_md_invalid`); `hook: {unclaimed: allow|deny}`. The
seed ships no panel and no non-default hook policy — only a comment showing
the shapes.

### `pulse doctor`

One read-only pass, eight checks, exits non-zero (`doctor_findings`) on any
finding: torn/unparsable store lines (the doctor is the lenient reader —
the strict store refuses to read until fixed), unreadable receipt lines,
expired leases on `active` tickets, orphan evidence directories no receipt
names, lanes prepared but never sealed (`stale_lane_preparations`),
`awaiting_commit` (dirty paths whose covering tickets are all `done`),
`learning_suspects`, `stale_cites`.

### `pulse serve`

Read-only board server (tiny_http), binds `127.0.0.1` (default port 7777),
`--open` launches a browser, `--workspace <dir>` scans a tree (depth ≤ 4)
in addition to the registry; dead registry entries self-hide. API:
`GET /api/projects`, `GET /api/p/<pid>/board`,
`GET /api/p/<pid>/issue/<id>` (record + receipts + evidence manifest +
event trace), `GET /p/<pid>/evidence/<rel>` (canonicalize + prefix check
against traversal). Lenient reads: a torn line is skipped and counted,
never a 500. No write endpoint, no lock, no actor.

## 12. Commands and error codes

### 12.1 Command surface (1:1 with `pulse --help`)

| Command | Purpose |
|---|---|
| `pulse init [--refresh] [--take-new F] [--keep-mine F] [--no-register] [--with-qa-templates]` | enroll a repo; three-way-merge template updates |
| `pulse work new <kind> <title> [--story S] [--epic E] [--risk R] [--surface S] [--from F]` | create a draft record |
| `pulse work show <id>` / `list [--kind --status --story --tag --ready]` / `tree [id]` | read records |
| `pulse work ready <id>` | run the ready gate (`draft → ready`) |
| `pulse work update <id> (--set k=v \| --from F \| --stdin)` | merge fields; `lease`/`verdicts`/`checkpoints` refused |
| `pulse work dep add` | add a dep edge (undo is a hand edit) |
| `pulse work transition <id> --to --reason` | `draft\|ready\|blocked → cancelled`, `ready\|active → blocked` |
| `pulse claim <id> [--ttl 3600]` | take the lease (`ready → active`), the file reservation |
| `pulse frontier [story]` | what can run right now (runnable / waiting / held) |
| `pulse reserve <id> <paths…>` | widen a held claim's `touches` mid-run |
| `pulse release <id>` | drop a stuck/expired lease (`active → ready`) |
| `pulse packet <id>` | the one bounded JSON a worker reads — it **is** the output (default and `--json` alike; the flag is an accepted no-op, dogfood 0025 F8) |
| `pulse checkpoint <id> --from F` | append a checkpoint + receipt; no status change |
| `pulse verify <id> [--timeout 900]` | run declared `verify[]` + learning checks; seal what was observed |
| `pulse handoff <id> --from F` | the handoff gate (`active → verifying`) |
| `pulse lane input <id> <role> [--seat N] [--force]` | write the lane's bounded input + pre-run snapshot |
| `pulse lane seal <id> <role> [--seat N]` | validate the lane output, seal the receipt |
| `pulse lane reconcile <id> <role> [--prepare] [--timeout 900]` | merge a panel's seats into the one receipt close reads |
| `pulse close <id>` / `close-story <id>` | the only ways to `done` |
| `pulse note <id> <text> [--friction] [--from actor]` | append a note |
| `pulse events tail [--since cursor] [--id] [--follow]` | read the event log |
| `pulse learn add [--from F \| --title/--kind …] [--friction] [--check-argv] [--check-cwd] [--cite]` | create a candidate learning |
| `pulse learn show [id] [--status]` / `applicable <id> [--all]` | read learnings |
| `pulse learn friction [id] [--all]` / `dismiss <subject> [keys…] --reason` / `activate <id>` / `retire <id> --reason` | the loop's classifications and human gates |
| `pulse doctor` | read-only health report; non-zero exit on findings |
| `pulse metrics [--since]` | the loop's numbers from the log |
| `pulse docs applicable <id>` / `check [--ticket <id>] [--write P]` | doc hint / doc rot + the `check-docs` lane |
| `pulse hook pre-edit [--path P]… [--stdin-json]` / `snippet <host>` | the pre-edit gate / print host config |
| `pulse serve [--workspace D] [--port 7777] [--open]` | read-only board |

Every command accepts `--repo-root` (default: cwd); most accept `--actor`
and `--json`. Errors print a JSON object `{code, message, hint?}` on stderr
and exit 1.

### 12.2 Kernel error codes (all carry a mandatory hint)

| Code | Raised by | Meaning |
|---|---|---|
| `actor_invalid` | identity | actor string malformed, or none given and `git config user.name` unset |
| `artifact_missing` | evidence | a declared artifact does not exist at receipt time |
| `checkpoint_lease_mismatch` | checkpoint | checkpoint actor or `run_id` does not match the live lease |
| `claim_actor_busy` | reservation | the actor holds a live lease on another ticket |
| `claim_files_reserved` | reservation | `touches` overlap files another ticket holds (claim and reserve) |
| `dep_cycle` | issues | the dep edge would close a cycle |
| `dep_type_invalid` | issues | dep type must be `blocked_by` or `supersedes` |
| `doctor_findings` | cli | doctor found ≥ 1 finding (drives the non-zero exit) |
| `field_owned_by_runtime` | issues | `work update` tried to write `lease`/`verdicts`/`checkpoints` |
| `friction_not_found` | learn | the cited friction key does not exist on the subject |
| `friction_reason_missing` | cli | `learn dismiss` without `--reason` |
| `friction_selection_missing` | cli | `learn dismiss` selected no friction (no keys, no `--all`) |
| `from_file_invalid` | cli | `--from` is not a valid handoff/checkpoint JSON shape |
| `gate_failed` | completion | a gate report was not clean (violations listed in the message) |
| `git_invocation_failed` | source | git missing, failed, or produced non-UTF-8 |
| `hook_host_unknown` | cli | `hook snippet` for a host other than `claude` |
| `init_refresh_unknown_file` | init | `--take-new`/`--keep-mine` named a file refresh does not manage |
| `issue_kind_invalid` | issues | `work new` kind is not epic/story/ticket/decision |
| `issue_not_found` | issues | id is not in the store |
| `issues_line_invalid` | store | a store line is not valid JSON/UTF-8 |
| `issues_record_invalid` | store | a record fails the embedded schema |
| `lane_actor_not_independent` | lane | the sealing actor sealed the latest handoff |
| `lane_commit_mismatch` | lane | `environment.commit` ≠ HEAD |
| `lane_mutated_workspace` | lane | the fence's `dirty_hash` changed between input and seal |
| `lane_not_in_profile` | lane | the lane is not in the record's profile (no `--force`) |
| `lane_not_prepared` | lane | no pre-run snapshot for this lane slot |
| `lane_not_verifying` | lane | ticket subject is not `verifying` |
| `lane_output_invalid` | lane | lane output missing or fails the `LaneOutput` shape |
| `lane_seat_actor_reused` | lane | the actor sealed another seat of the same panel round |
| `lane_seat_invalid` | lane | `--seat` without a panel, out of `1..=count`, or with `--force` |
| `lane_seat_required` | lane | a panel role run without `--seat` |
| `learning_cite_invalid` | learn | `--cite` path/line-range malformed or unreadable |
| `learning_invalid` | learn | learning frontmatter/kind/check-argv malformed |
| `learning_not_found` | learn | no learning file with that id |
| `learning_not_yet_helpful` | learn | `activate` with `usage.helpful < 1` |
| `metrics_since_invalid` | cli | `--since` is neither RFC3339 nor a date |
| `profile_missing` | profile, lane | no such profile key in `PULSE.md`; or a story-scope lane without surface/risk |
| `pulse_md_invalid` | profile | `PULSE.md` missing, not YAML, or an invalid panel |
| `receipt_conflict` | evidence | same receipt id, different content |
| `receipt_not_found` | evidence | id not in `.pulse/receipts/*.jsonl` |
| `reconcile_not_prepared` | lane | no `--prepare` input/map/snapshot for this round |
| `reconcile_seats_missing` | lane | not every seat is sealed for the round |
| `registry_home_missing` | serve | no `PULSE_REGISTRY` and no `HOME`/`USERPROFILE` |
| `release_not_holder` | reservation | live lease held by another actor; caller not `human:` |
| `reserve_lease_mismatch` | reservation | reserve without the ticket's own live lease |
| `reserve_paths_invalid` | reservation | empty path list or an unsafe path |
| `role_forbidden` | roles | the actor kind may not perform the action |
| `run_lease_held` | reservation | another actor's live lease on this ticket |
| `run_not_ready_or_active` | reservation | claim on a ticket that is not `ready`/`active`/`verifying` |
| `set_invalid` | cli | `--set` value is not `k=v` |
| `transition_not_allowed` | issues | manual transition outside the allowed pairs |
| `verify_argv_invalid` | verify | bad `name`/`argv`/`cwd` in a declared command |
| `verify_failed` | verify | some observed exit ≠ 0 (receipt is sealed *before* this error) |
| `verify_not_runnable` | verify | ticket is not `active` or `verifying` |
| `verify_nothing_declared` | verify | no `verify[]` and no matching learning check |

Validation-layer codes (outside the kernel family, no hint): `io_error`,
`json_error`, `json_serialize_error`, `non_canonical_number`,
`unsafe_path` (absolute/escaping/traversal), `lock_timeout` (10 s write
lock), `receipt_privacy_violation` (redaction refused).

Gate violation codes (surface inside `gate_failed`, one test each) are
listed with their gates in §4: `ready_*` (9), `handoff_*` (8) plus
`learning_unknown`, `close_*` (6), `close_story_*` (5).

## 13. Known limits (honest)

- **Identity is self-declared.** §8's rules guard against accidents, not
  forgery; a session that declares itself `human:` walks past the hook's
  actor rules.
- **Cross-build breakage in one checkout.** Worker A can break worker B's
  build through a file B only *reads*; mitigations are `blocked_by` edges
  at plan time and `pulse verify` at handoff. Reconsider per-ticket
  worktrees above one such friction per story (decision 0025 §8).
- **`lane_commit_mismatch` mid-review.** If another ticket's commit lands
  while a lane runs, the seal refuses and the lane re-runs. Accepted;
  the 0025 dogfood (2026-09-19) did not hit it once — the scoped fence
  filtered cross-ticket dirt as designed — so it stays measured-too-little
  to call resolved.
- **Verify grandchildren survive.** A timed-out verify kills its child, not
  the process group (decision 0026 risk 1); reaping is a host concern.
- **The hook sees only the host's edit tools.** Shell writes bypass it;
  `handoff_unreserved_changes` is the second net. The snippet covers
  Claude Code only; Codex's deny contract was never verified, so no
  snippet is invented for it.
- **A verify receipt pins the tree it *created*.** The fence is captured
  after the commands run; `handoff_unreserved_changes` catches a
  self-editing verify outside `touches` only.
- **Panel qa lanes don't merge `cases`.** A reconciled receipt carries an
  empty `cases` list, so a story-scope qa panel fails
  `close_story_qa_not_satisfied` loudly, never silently (decision 0027).
- **Not dogfooded — was the last known limit; now closed.** Phases
  B/D/C/E/F/G ran on a real target repo (`~/Workspace/Personal/todolist`,
  story ST-33d3, 2026-09-19); the friction table, the three measured
  numbers (cross-build breakage 0 — single checkout kept;
  `docs_maybe_stale` 0/3 correct — stays advisory; verify max 11.2s vs
  the 900s timeout) and the decisions still pending live in
  [`docs/plans/0025-dogfood.md`](docs/plans/0025-dogfood.md).
  Two practical lessons already shipped as fixes: scratch files are
  per-ticket under the fenced-out `.pulse/runtime/` (the prompt teaches
  it; a bare shared `handoff.json` once cost a re-claim, a re-verify and
  two lanes), and `pulse packet` prints the packet itself.
