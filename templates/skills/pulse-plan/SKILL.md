---
name: pulse-plan
description: Use when a shaped Story has no Tickets yet, or its Tickets must be re-cut after the shape changed. Not for shaping meaning, not for a single bounded change that needs no Story, and never for execution.
---

# Pulse Plan

Cut one shaped Story into Tickets the runner can execute. The cut is the
decision this skill owns: which vertical slices exist, how coarse each is, and
which dependency truly gates. Plan's entire output is Ticket records that are
`ready` — it never runs a worker and never edits product source.

## 0. Preconditions

The input is exactly one Story in `draft` or `ready`, with a non-empty
`outcome` and at least one rule or QA case:

```text
pulse work show ST-<id> --json
pulse work tree ST-<id>
```

No Story, or an empty one, is a `pulse-shape` job — cutting against an
unshaped Story is how horizontal tickets get created. Return it and stop.

A `blocking` open question on the Story also stops the plan. Planning may
carry a settled question into a Ticket's `open_questions` as context; it may
never answer one.

## 1. Read the code before proposing a cut

Anchors and verify commands come from the repo, never from memory. Walk the
paths the Story's behavior touches and note what lives at each
(`api/app/main.py: routers included here`); check how the stack runs in
`docs/operations/run.md`, package manifests and existing tests to find verify
commands that actually run here. A `verify[]` argv that has never executed in
this repo is a bug baked into the Ticket before a worker ever starts — and
since decision 0026 Pulse runs it itself at `pulse verify`, it has to be
argv (never a shell string), non-interactive, and finished well inside the
timeout (default 900s).

```text
pulse work show <sibling-ticket> --json
```

(You have already grep/glob'd `docs/` for what the Story touches;
`pulse docs applicable ST-<id>` is a frontmatter-driven cross-check after
that search, not a substitute for it.)

## 2. The cut

Each Ticket is a tracer bullet: a narrow complete path, demonstrable on its
own, sized for one fresh context window. Cut vertical — a UI ticket talks to
the real API — never horizontal ("add models", then "add endpoints", then
"wire UI" is three tickets of which none proves anything).

The usual two-ticket cut for a Story that spans api and ui: one API ticket,
one UI ticket integrating against it, the UI ticket `blocked_by` the API one
when it cannot be demonstrated without the API's behavior. When a ticket
consumes what another produces — a model, an endpoint, a fixture — wire
`blocked_by` even where a shared file already forces the ordering: the
frontier's file arithmetic cannot see a content prerequisite (dogfood 0025,
F3). A shared file is
not a dependency and an ordering preference is not a dependency — wire only
edges that truly gate, because every edge delays work.

Set each Ticket's `risk` and `surface` from the slice itself. They pick the
close profile in `PULSE.md`: `high` means adversarial review plus a human
gate. Do not inflate them, and do not deflate them to dodge lanes.

Every cross-cutting concern the Story implies but no ticket owns (middleware,
a migration, config) must land in exactly one ticket's `change.required` —
an orphaned concern is how the next friction note gets written.

**Stay inside the map.** Read the Story's Epic before cutting
(`pulse work show <epic> --json`): `out_of_scope[]` is work the effort
ruled out, and a Ticket that delivers any of it is a cut nobody asked for —
raise it with the human instead of planning it. `not_yet_specified[]` is
the opposite: in-scope fog. If your cut resolves one of those items, say so
in the report, because the Epic cannot close while the item is still listed
and someone must move it.

**Cut only the Story in front of you.** Tickets are created for work whose
shape is settled now — never for a Story further down the Epic, and never
as a placeholder for work the cut has not reached. A Ticket that exists
before its Story is shaped ages into a wrong plan that someone still has to
read, and it occupies `touches` globs that fence live work out of the
frontier. The Epic's own record is the queue; `pulse work tree` reads it.

## 3. Put the cut to the human

One message, not an interview: the proposed tickets (title, surface, risk,
one-line objective each), the `blocked_by` edges, and which Story QA case
each ticket carries. Ask once — right grain? right edges? — and iterate on
the answer. Create nothing before the yes.

## 4. Create, wire, ready

Create each Ticket from a payload file (shallow merge — complete arrays).
Every mutating command carries an actor (`--actor human:<name>`, or
`PULSE_ACTOR` in the environment):

```text
pulse work new ticket "Tasks API: due dates + views" --story ST-<id> --risk medium --surface api --from ticket.json --actor human:quan --json
pulse work dep add TK-<ui> blocked_by TK-<api> --actor human:quan --json
```

The Ticket payload and the field-by-field rules that make a cut survive
the gates are in
[references/ticket-payload.md](references/ticket-payload.md) — read it when
you are about to write a payload.

Then take each Ticket through the ready gate and read the frontier back:

```text
pulse work ready TK-<id> --json
pulse frontier ST-<id> --json
```

A Ticket that will not go `ready` is not planned yet. Fix the record and
re-run; never work around the gate.

## Red flags

- a horizontal cut — "models", then "endpoints", then "wire UI" — where no
  single Ticket demonstrates anything;
- Tickets for a Story that has not been shaped yet, or placeholder Tickets
  named after a phase;
- two Tickets whose `touches` overlap with no `blocked_by` between them;
- a `verify[]` argv that has never run in this repo;
- an anchor whose path does not exist on disk;
- a `docs_to_update` path declared by more than one Ticket of the Story;
- a cut that delivers something the Epic listed in `out_of_scope`.

## Report

Report exactly these sections:

```markdown
## Cut
- ST-… → TK-… (api, medium) — <objective one-liner> — QA-001
- ST-… → TK-… (ui, medium) — <objective one-liner> — QA-002 — blocked_by TK-…

## Anchors
- <ticket> — <anchors verified on disk>

## Ready
- TK-… pass
- TK-… blocked: <reason codes>

## Frontier
- runnable: TK-…, TK-… — disjoint `touches`; the host spawns one worker per
  ticket, each its own actor (`agent:worker-<n>`)
- waiting: TK-… on TK-… (<reason: frontier | reserved | blocked_by>)

## Next
pulse claim TK-… --actor agent:worker
```
