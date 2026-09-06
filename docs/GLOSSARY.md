# Glossary

Terms used by Pulse. Definitions follow [`PRODUCT.md`](../PRODUCT.md).

## Work graph

**Epic** — a large outcome and investment boundary. Prose in
`works/EP-*/brief.md`, `design.md`.

**Story** — a provable behavioral slice; default owner of the QA baseline.
Prose in `works/ST-*/story.md`, `approach.md`, `qa.md`.

**Ticket** — the smallest unit one agent can lease, execute and verify.
Contract in `works/TK-*/ticket.md`; the node holds only metadata and the file
hash. Roles: `implementation` or `decision_work`.

**Decision** — a hard-to-reverse choice with context, options, decision and
consequences. Prose in `works/DEC-*/decision.md`.

**Edge** — a typed relation stored as its own file: `parent`, `blocked_by`,
`preferred_after`, `superseded_by`, `related`. ID is deterministic from
`(type, from, to)`.

**Revision** — CAS counter for every node mutation. **Contract revision** —
increments only when semantic inputs change (ticket.md, risk, QA/docs
posture, required Decisions).

**Lease** — exclusive right of one actor to execute one Ticket, held in
`.pulse/runtime/` with a TTL.

**Materialization (R0–R3)** — how much artifact a piece of work requires,
chosen by risk.

**Ready gate** — the deterministic checks a Ticket must pass to be
executable. **Ready stale** — status is `ready` but an input fingerprint
changed.

**Supersession** — an outcome absorbed by another node; status `superseded`,
history kept.

## Context

**Packet** — the bounded JSON an agent reads before working on a Ticket:
contract, applicable docs, Decisions, QA cases, learnings, notes, source
commit. Fenced to a commit.

**Applicability** — which docs or learnings apply to a Ticket, from code
anchors ∩ `scope.paths`, shared tags, or explicit references.

**Tags** — a controlled vocabulary in `.pulse/docs/tags.json`, shared by docs,
Tickets and learnings.

**Registry** — `.pulse/docs/registry.json`, the sidecar listing durable docs
with id, path, summary, owner, kind, status, scope and tags.

## Execution

**Runner** — a role (`worker`, `reviewer`, `qa`, `check`) mapped to a shell
command in `.pulse/config/runners.json`. `pulse run <role> --ticket <id>`
leases, writes input, executes, reads JSON, records a receipt.

**Isolation** — where a runner works. Default is the checkout; `pulse run`
refuses with `run_isolation_required` while another Ticket holds a live
lease, and a Pulse-owned worktree is created only when the caller passes
`--isolation worktree`.

**Handoff** — the worker's typed proposal that a Ticket is ready for
verification. It is not `done`.

## Evidence

**Receipt** — an immutable, content-hashed JSON record in
`.pulse/evidence/receipts/` bound to a work id, contract revision and source
commit. Kinds: `handoff`, `verification`, `qa_checkpoint`,
`docs_validation`, `decision_acceptance`, `close`.

**Artifact** — a file referenced by a receipt, stored by SHA-256 under
`.pulse/evidence/artifacts/`.

**Close gate** — the checks `pulse work close` performs before a Ticket
becomes `done`. Requires independent verification.

**QA baseline** — `works/<story>/qa.md`, markdown with conventional headings
(`## Scope`, `## Posture`, `## Risks`, `## Exit criteria`, `## Cases` with one
`### QA-NNN` section per case) listing the behavioral cases a Story promises;
Pulse parses it into `qa-input.json` for the runner (Decision 0010).
**pulse-check** — an optional block inside a case giving the runner an argv
command and fixed assertions. **QA checkpoint** — a QA run over the
cases a Ticket affects. **Story qualification** — a QA run over the full
required baseline before a Story closes.

**Posture** — a Ticket's declared QA or docs impact: `required`, `none` with
rationale, or `deferred`/`covered_by_story_close` with a link.

**Finding** — one repairable gap reported by a reviewer, QA runner, check or
`doctor`: `summary`, `owner` (path or `DOC-ID#section`) and `check` (the argv
that showed it or a receipt id); reviewer findings add `acceptance_id`, QA
findings add `case_id`. A finding without `check` is kept as `unverifiable`
and cannot alone justify `rework` (Decision 0012).

**Reviewer as evidence** — a reviewer verdict is a receipt to be counted, not
an authority: the reviewer input carries the worker's claims (changed paths,
acceptance ids, proof receipts) and never the worker's summary; the reviewer
re-runs checks itself. **Triangulation** — a profile with `reviewers: 2`
requires two distinct reviewer actors to pass on the same handoff; used only
for high-risk profiles, never globally (Decision 0012).

**Redaction boundary** — the mechanical check on every tracked text field
(receipt payload, note, learning, finding summary) that rejects absolute
paths outside the repo root and secret-shaped strings with
`receipt_privacy_violation`; `session_ref` stays as an opaque join key
(Decision 0012).

## Ratchet

**Evidence ladder** — the per-mechanism label Pulse derives from receipts,
not a score: `present` (exists), `wired` (a task can reach it), `exercised`
(a task used it and left a receipt), `outcome_supported` (a later result
shows it helped), plus `missing`, `unobserved`, `not_applicable`. Shared
vocabulary of `pulse-ratchet` and `pulse doctor` (Decision 0012).

**Lane** — one of the three read-only evidence passes `pulse-ratchet` runs
after a close: `execution` (what happened: receipts, run records, friction
notes), `harness` (what exists and is wired: registry, `AGENTS.md`,
`PULSE.md`, `runners.json`, profiles), `knowledge` (what is already known:
learnings, relations, freshness). A lane sees only its own inputs and returns
at most five candidate findings without severity. **Lead** — the ratchet
session that reconciles lanes: keeps every candidate, merges only on the same
target, consequence, owner and repair route, assigns severity alone, and picks
exactly one intervention by track (`bootstrap`, `operationalize`, `optimize`,
`undetermined`) (Decision 0012).

**Expected signal** — the one-line prediction a `ratchet` learning must carry
of what the next rerun's handoff will show; `knowledge validate` requires both
`knowledge_usage: helpful` and that signal in the rerun's receipts.

**Learning** — a reusable record in `.pulse/knowledge/entries/` with
guidance, applicability, provenance, scope and status
(`candidate → validated → promoted`).

**Learning scope** — what a learning is about: `repository` (the codebase;
injects into packets by path/tag) or `harness` (how to operate Pulse itself;
injects into the runner bootstrap prompt, never by path, and promotes into
the target's `AGENTS.md`).

**Promotion** — moving a learning into a durable owner: a doc, a Decision, a
check in `runners.json`, or an eval. `knowledge promote` inserts the
learning's content after a chosen heading and records the relation bound to
the document's new content hash; a promotion that leaves the target
unchanged is refused.

**Failure class** — the taxonomy applied after a failed run (`context_gap`,
`tool_gap`, `verification_gap`, `docs_stale`, …) that decides which harness
fix follows.

## Communication

**Event log** — append-only events in `.pulse/events/<date>.jsonl`, one JSON
line per event, written by every mutation (Decision 0011). Read with
`pulse events tail`; the cursor is the event ULID.

**Session ref** — the host agent's session id recorded in a handoff receipt and
`run.completed` so a Ticket can be joined to the host transcript. Pulse never
records tool calls itself.

**Friction note** — `pulse note --kind friction`, the mandatory way a worker or
reviewer reports harness friction; the close gate turns it into a `harness`
learning candidate.

**Guidance surface** — the `<!-- PULSE:BEGIN/END -->` block `pulse init` writes
into the target's `AGENTS.md` plus the eight artifact-bound skills
(`wayfind`, `grill`, `spec`, `tickets`, `research`, `ratchet`, `onboard`,
`handoff`);
guidance only, the CLI stays the sole authority (Decision 0009).

**Note** — an event addressed to any node (Epic, Story, Ticket, Decision),
written with `pulse note --work`, at most 2000 characters, shown in a
Ticket's packet and in `events tail`. `--ticket` is an alias.

**Handoff receipt** — the `work handoff` record a worker files for one
Ticket: changed paths, `checks[]`, `acceptance_proofs[]`, bound to lease,
session, commit and `source_dirty_hash`. Not to be confused with the next
term.

**Session handoff** — moving a conversation to a fresh agent session when
context nears its limit. The host counts tokens and enforces the threshold
with a hook; Pulse never reads transcripts. The `pulse-handoff` skill flushes
durable state into `works/`, `docs/` and the graph, writes a **handoff doc**
(live thread only, references not copies) to `.pulse/runtime/handoff/<node>.md`,
and leaves one pointer note (Decision 0013).

**Context guard** — the host-side Stop hook that blocks a turn once the
transcript exceeds a byte threshold and instructs the agent to run
`pulse-handoff`; a sample ships in the target's `docs/operations/`.

**Resume** — `pulse work resume` (Later): a read-only query listing
non-terminal nodes with a handoff note and the command to reopen each; it
never spawns an agent or takes a lease.
