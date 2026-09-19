---
name: pulse-shape
description: Use when a requirement is too big for one bounded change, or when two reasonable people could deliver it differently and both be right — before any Story or Ticket for it exists. Not for a small well-understood change, for read-only analysis, for cutting Tickets, or for implementation.
---

# Pulse Shape

Shape the work before it exists. Finish with Epic/Story records a planner can
cut without re-asking what the words meant.

Shape is one interview, not three: destination, meaning and proof are the same
conversation about one requirement at three depths, so the old
wayfind → grill → spec chain collapsed into this skill. Shape creates no
Ticket and writes no product code. `pulse-plan` owns the cut; a worker
session (`.pulse/prompts/worker.md`) owns the code.

## 0. Establish authority

1. Read the Pulse block in `AGENTS.md` and `PULSE.md`.
2. Read what already exists before asking anyone anything:

   ```text
   pulse work tree
   pulse work list --json
   ```

   An Epic or Story may already cover the request — say so and stop rather
   than shaping a duplicate. Ids are never recycled; a new Story under an
   existing Epic continues that Epic's story order.
3. Read the repo as a fact source: `docs/README.md` and the docs it maps, the
   code the requirement touches — grep/glob `docs/` first; once a candidate
   Story exists, `pulse docs applicable ST-<id>` is a frontmatter-driven
   cross-check, not the search itself. Facts never become questions; see §1.

### Choose the lightest level that protects the work

Name the level out loud before interviewing anything, and say why the level
below it is not enough. Pulse has three, and each costs a human more than
the last:

| Level | Fits when | Costs |
|---|---|---|
| **Ticket** | the meaning is already agreed and the change is bounded | `pulse work new ticket` + `ready`; no interview |
| **Story** | the work exceeds one Ticket, or two reasonable people would deliver it differently | one interview, then `pulse-plan` |
| **Epic + Story** | the way to the destination is not visible yet — several Stories, and which ones is part of what you are finding out | a map that stays open across sessions |

An interview over a settled requirement is ceremony, and it teaches the
human that shaping wastes time. Route a Ticket-level request through the
ordinary flow in the `AGENTS.md` block and stop. Announce the level so the
human can overrule it — "this reads Story-level: the rules are contested
but the destination is clear, so no Epic". The ratchet runs one way:
complexity found mid-interview moves you up a level, never down.

## 1. One interview, one question per message

Every ambiguity is either a **researchable fact** (the repo, the docs or the
code already answers it — go find out and report the finding) or a **product
decision** (intent, preference, trade-off — only these are worth a human
turn). Asking the human to guess at something the code states is how a wrong
answer enters the record wearing human authority.

Ask exactly one question per message. Every question carries a **recommended
answer** so the human can confirm in one word — a question without a
recommendation is homework you assigned the human; do the thinking first.

Stamp every settled decision with an id (`D-1`, `D-2`, …) the moment it is
made and use the id thereafter ("per D-3, no auth"). D-ids are interview
handles only: they restart at `D-1` in the next interview, so they never
reach `docs/`. A decision that outlives the interview — a trade-off someone
will later ask "why?" about — becomes a Decision record with a durable
`DEC-…` id in §2; a mere clarification just lands in the rule it settled.

Interview in this order; stop as soon as the shape is complete:

1. **Outcome** — the observable end state in one or two sentences. "How will
   you know it worked?" is the question behind every outcome.
2. **Business rules (`<CAP>-BR-<n>`)** — testable statements of how the
   product behaves. A rule belongs to a product **capability**, not to the
   Story that introduced it: prefix the id with the capability's short
   uppercase name (`TAG-BR-1`, `TASK-BR-4`) and number within that
   capability across every Story, so an id is unique in the repo and stays
   true after this Story closes. Read `docs/product/<capability>.md` first:
   continue its numbers, never reuse one; a Story that changes an existing
   rule carries that rule's id with the new text.
3. **Exceptions (`<CAP>-E-<n>`)** — the failure behavior visible to users or
   callers; same capability-scoped numbering.
4. **Boundaries** — what is explicitly out goes to the Epic's
   `out_of_scope`; visible-but-unshaped fog goes to the Epic's
   `not_yet_specified` rather than being invented into a rule.
5. **QA cases (`QA-001`, …)** — what proves the Story: `intent`, `surface`,
   `priority` (high-priority cases gate `close-story`), `steps`, `expected`.
   At least one case; usually one per surface the Story touches.
6. **Open questions** — anything genuinely undecided gets a disposition
   (`resolved|rejected|delegated|deferred|blocking`) and, when answered, the
   answer plus its D-id. A `blocking` question means the shape is not done.

End the interview by reading the whole shape back — outcome, every rule,
every case, every open question — and taking one confirmation. Mutations come
only after that yes.

## 2. Write the records

Write payload JSON to a temp file and seed from it. Record updates merge
shallowly, so send complete arrays, never diffs. Every mutating command
carries an actor (`--actor human:<name>`, or `PULSE_ACTOR` in the
environment):

```text
pulse work new epic "<title>" --from epic.json --actor human:quan --json
pulse work new decision "<title>" --from decision.json --actor human:quan --json
pulse work new story "<title>" --epic EP-<id> --risk medium --surface api --from story.json --actor human:quan --json
```

Payload shapes for every record kind — Decision, Epic, Story, with the
field-by-field rules — are in
[references/record-payloads.md](references/record-payloads.md). Read it
when you are about to write a payload, not before.

Two rules that decide the shape, so they stay here:

- **The Epic is a map, not a folder.** Create one only when the effort is
  bigger than a Story and its way is not yet clear. Its `outcome` is the
  destination; `success_signals[]` is how anyone tells the destination was
  reached — the ready gate refuses an Epic missing either. `out_of_scope[]`
  is ruled-out work (closed; it never graduates back in) and
  `not_yet_specified[]` is in-scope fog you can see but cannot shape yet.
  Fog is a debt: `pulse close-epic` refuses while any remains, so each item
  must graduate into a Story that shaped it or move to `out_of_scope`.
- **The Epic never restates its Stories.** A rule lives in exactly one
  place — the Story that owns it — and the Epic only points. An Epic that
  copies its Stories' content goes stale the first time one of them changes.

Fix-ups after creation go through
`pulse work update <id> --from fix.json --actor …`; `revision` moves under
you, so re-read with `pulse work show <id> --json` before every update.

## 3. Write the prose home

The records are the truth layer and the trace (which Story added which
rule, which QA case proves it); docs are where a human learns the product.
Keep the two apart. Which file takes what, and how to fold a changed rule
into a capability doc without leaving a Story-shaped seam, is in
[references/prose-homes.md](references/prose-homes.md).

Then prove the docs still stand:

```text
pulse docs check
```

## 4. Exit through the ready gate

```text
pulse work ready ST-<id> --json
```

A clean report is what makes shaping done. Fix the shape and re-run; never
work around the gate. A Story left `draft` means a `blocking` question
remains — resolve it or the Story waits. Epics stay `draft` forever; they
have no gate. From here the Story goes to `pulse-plan`.

## Red flags

- an interview over a requirement nobody actually disputes;
- a question the repo already answers — facts are researched, not asked;
- a question sent without a recommended answer attached;
- an Epic created because the work felt big, with no fog to chart;
- a rule numbered per Story (`BR-1`) instead of per capability
  (`TAG-BR-1`), so two Stories collide on one id;
- a D-id written into `docs/`, or a decision recorded in the product doc
  rather than in `docs/decisions/`;
- a Story left `draft` with a `blocking` open question and called shaped.

## Report

Report exactly these sections:

```markdown
## Shaped
- EP-… (draft) — <title> — new | existing
- ST-… (ready) — <title> — <outcome one-liner>

## Decisions
- D-1 <decision> — DEC-ab12, docs/decisions/DEC-ab12-<slug>.md
- D-2 <clarification> — landed in TAG-BR-2 (no record)

## QA cases
- QA-001 (api, high) — <intent> — check: yes | no

## Open
- <question> — <disposition> — <what would sharpen it>

## Next
pulse-plan on ST-…
```
