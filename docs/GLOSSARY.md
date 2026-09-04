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

**Isolation** — where a runner works. Default is the checkout; a worktree is
used only for a second concurrent Ticket or on request.

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

**QA baseline** — the `pulse-qa` block in `works/<story>/qa.md` listing the
behavioral cases a Story promises. **QA checkpoint** — a QA run over the
cases a Ticket affects. **Story qualification** — a QA run over the full
required baseline before a Story closes.

**Posture** — a Ticket's declared QA or docs impact: `required`, `none` with
rationale, or `deferred`/`covered_by_story_close` with a link.

## Ratchet

**Learning** — a reusable record in `.pulse/knowledge/entries/` with
guidance, applicability, provenance and status
(`candidate → validated → promoted`).

**Promotion** — moving a learning into a durable owner: a doc, a Decision, a
check in `runners.json`, or an eval.

**Failure class** — the taxonomy applied after a failed run (`context_gap`,
`tool_gap`, `verification_gap`, `docs_stale`, …) that decides which harness
fix follows.

## Communication

**Event log** — append-only JSON events in `.pulse/events/`, one file per
event, written by every mutation. Read with `pulse events tail`.

**Note** — an event addressed to a Ticket, written with `pulse note`, shown in
that Ticket's packet.
