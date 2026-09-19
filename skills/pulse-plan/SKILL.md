---
name: pulse-plan
description: Cut one shaped Story into Tickets and take each through the ready gate. Reads the Story's rules and QA cases plus the actual code to write full Ticket records — objective, path-annotated anchors, when/then acceptance, verify argv, qa_cases references, blocked_by dependencies. Use it when a Story is draft/ready and needs its Tickets. Do not use it to shape meaning (pulse-shape), for a single bounded change that needs no Story (the ordinary route in the AGENTS.md Pulse block), or for any execution — planning never runs a worker and never edits product source.
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
when it cannot be demonstrated without the API's behavior. A shared file is
not a dependency and an ordering preference is not a dependency — wire only
edges that truly gate, because every edge delays work.

Set each Ticket's `risk` and `surface` from the slice itself. They pick the
close profile in `PULSE.md`: `high` means adversarial review plus a human
gate. Do not inflate them, and do not deflate them to dodge lanes.

Every cross-cutting concern the Story implies but no ticket owns (middleware,
a migration, config) must land in exactly one ticket's `change.required` —
an orphaned concern is how the next friction note gets written.

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

Ticket payload — the fields the whole harness reads:

```json
{"objective":"what this ticket delivers, one paragraph",
 "description":"## Approach\n…markdown, see below…",
 "touches":["api/app/"],
 "context":{"anchors":["api/app/main.py:routers included here",
                       "docs/operations/run.md:how to start the stack"],
            "docs":["docs/architecture/overview.md"]},
 "change":{"required":["…"],"invariants":["…"],"docs_to_update":["docs/…"]},
 "non_scope":["…"],
 "acceptance":[{"id":"AC-1","when":"POST /tasks {\"title\":\"x\"}",
                "then":"201 with task body; blank title 422 (BR-1, E-1)"}],
 "verify":[{"name":"pytest","argv":["uv","run","pytest","-q"],"cwd":"api"}],
 "qa_cases":["QA-001"],
 "open_questions":[]}
```

- `description` is the how, and it is the field that decides whether an
  isolated worker drifts. It is free-form markdown — no fixed sections, the
  schema checks nothing but that it is a string — and the ready gate refuses
  a medium/high-risk ticket without one. The worker starts with none of what
  you learned reading the code, so write it for a capable engineer who has
  never opened this repo: the approach and why this one over the obvious
  alternative; every file and symbol to touch and what changes in each; the
  existing code to imitate (`path:line`, not "follow conventions"); the
  signatures, data shapes and error codes that must come out exactly so
  sibling tickets fit; the order to work in; the traps you hit while
  reading. Paste the short snippet instead of describing it. If you cannot
  write this, you have not read enough code yet — go read, do not pad.
- `touches` lists every file the ticket will edit or create — repo-relative
  globs (`dir/**`, one `*` within one segment; never absolute, never `..`).
  It is the parallel-claim key (decision 0025): while another ticket holds
  an overlapping `touches`, a claim is refused — so a missing entry stops
  the worker mid-flight to add one, and a greedy entry parks an unrelated
  ticket for no reason. Two tickets whose `touches` overlap should carry a
  `blocked_by` edge, or accept that they run serially. The ready gate
  refuses a medium/high-risk ticket without it.
- `context.anchors` entries are `"path: what lives there"` — the part before
  the `:` must exist on disk; the ready gate checks it.
- `acceptance` is EARS-minimal: one observable behavior per item, `when` and
  `then` non-empty, citing the Story's BR-*/E-* in `then`. Reviewer and QA
  map 1:1 against these ids.
- The Story's rules and exceptions must end up in `docs/**` by id (the
  close-story gate refuses a story whose BR-*/E-* live only in
  `issues.jsonl`, plan 0025 F4). Give that writing an owner now: the last
  ticket of the Story — or whichever ticket owns the rule's code, via its
  `change.docs_to_update` — carries the doc work, so it never piles up at
  close.
- `verify[].argv` is argv, never a shell string; `cwd` defaults to repo root
  and must name a directory that exists. Pulse runs these itself (decision
  0026): no shell, one at a time, each killed at the timeout — so keep them
  non-interactive, bounded, and prefer a command whose *output* says what
  failed.
- `qa_cases` holds Story case ids, and each referenced case's surface should
  match the ticket's own surface.
- `non_scope` names the adjacent work deliberately not being done; the worker
  honors it literally, so a missing boundary becomes scope creep with
  receipts.

Wire every approved `blocked_by` edge in a second pass, after all ids exist.
Then run the gate per ticket and read the report — it lists every violation,
so fix all of them and re-run:

```text
pulse work ready TK-<id> --json
```

A ticket whose blockers are still open stays put; report it blocked with the
gate's reason codes instead of working around the gate.

When every ticket is through the gate, read the parallel map of your cut:

```text
pulse frontier ST-<id> --json
```

This is decision 0025 made visible: which tickets can run at the same time
because their `touches` are disjoint, and which one waits on which — a
waiting ticket here is the host's signal to plan a `blocked_by` edge or a
narrower cut, not something to fix by editing `touches` after the fact.

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
