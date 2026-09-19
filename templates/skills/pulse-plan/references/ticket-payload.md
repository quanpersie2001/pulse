# Ticket payload

Create each Ticket from a payload file (shallow merge — send complete
arrays, never diffs). Every mutating command carries an actor
(`--actor human:<name>`, or `PULSE_ACTOR` in the environment):

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
                "then":"201 with task body; blank title 422 (TASK-BR-1, TASK-E-1)"}],
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
  refuses a medium/high-risk ticket without it. Every `change.docs_to_update`
  path is a file the ticket must edit, so it belongs in `touches` too
  (dogfood 0025, F11: three workers each had to `pulse reserve` their own
  shared doc mid-flight because no planner put it there) — the doc's single
  owner's `touches`.
- `context.anchors` entries are `"path: what lives there"` — the part before
  the `:` must exist on disk; the ready gate checks it.
- `acceptance` is EARS-minimal: one observable behavior per item, `when` and
  `then` non-empty, citing the Story's rule/exception ids (`TAG-BR-1`,
  `TAG-E-1`) in `then`. Reviewer and QA map 1:1 against these ids.
- The Story's rules and exceptions must end up in `docs/**` by id (the
  close-story gate refuses a story whose rules live only in
  `issues.jsonl`, plan 0025 F4). They go into `docs/product/<capability>.md`
  as the product's current behavior, folded into the capability's sections —
  never as a per-Story section. Give that writing an owner now: the last
  ticket of the Story — or whichever ticket owns the rule's code, via its
  `change.docs_to_update` — carries the doc work, so it never piles up at
  close. **One doc, one owner** (dogfood 0025, F10): if several tickets of
  the Story touch the same doc, exactly one of them declares it in
  `change.docs_to_update` — a doc is a file like any other, the reservation
  holds it through review, and three tickets declaring it serialize the
  whole Story at that one file.
- **Architecture docs have an owner too.** When a ticket adds or moves a
  boundary — a new service, module, table, external dependency, a
  cross-cutting convention (error shape, auth, caching, state management) —
  it declares the matching `docs/architecture/<area>.md` in
  `change.docs_to_update` (and `touches`), and the handoff gate then
  refuses the ticket until that doc really changed. Write one doc per area
  (`data-model.md`, `api-conventions.md`, `frontend-state.md`, …) with
  file-level `applies_to`, and keep `overview.md` as the map of them: system
  context, components and what each may depend on, the main request/data
  flows end to end, and the invariants a new contributor would break first.
  A diagram or a table beats a paragraph. "Where files go" alone is not
  architecture — say why the boundary exists and what crossing it costs.
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
