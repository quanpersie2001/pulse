---
name: pulse-shape
description: Turn a vague requirement into a shaped Epic/Story draft through ONE interview — one question per message, each carrying a recommended answer, every settled decision stamped with a stable D-id. Output is Epic/Story records with outcome, BR-* rules, E-* exceptions, QA-* cases, open questions and not_yet_specified fog, plus the product-doc and glossary lines. Use it when the work is bigger than one bounded change, or two reasonable people could deliver it differently and both be right. Do not use it for a small well-understood change (the ordinary `pulse work new ticket` route in the AGENTS.md Pulse block), for read-only analysis, for cutting Tickets (that is pulse-plan), or for implementation.
---

# Pulse Shape

Shape the work before it exists. Finish with Epic/Story records a planner can
cut without re-asking what the words meant.

Shape is one interview, not three: destination, meaning and proof are the same
conversation about one requirement at three depths, so the old
wayfind → grill → spec chain collapsed into this skill. Shape creates no
Ticket and writes no product code. `pulse-plan` owns the cut; `pulse run
worker` owns the code.

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
   code the requirement touches, and — once a candidate Story exists —
   `pulse docs applicable ST-<id>`. Facts never become questions; see §1.

Decide first whether shaping is needed at all. Shaping earns its place when
the work exceeds one bounded Ticket or its meaning is genuinely contested.
When the meaning is already agreed and the change is small, route through the
ordinary flow in the `AGENTS.md` block instead — an interview over a settled
requirement is ceremony, and it teaches the human that shaping wastes time.

## 1. One interview, one question per message

Every ambiguity is either a **researchable fact** (the repo, the docs or the
code already answers it — go find out and report the finding) or a **product
decision** (intent, preference, trade-off — only these are worth a human
turn). Asking the human to guess at something the code states is how a wrong
answer enters the record wearing human authority.

Ask exactly one question per message. Every question carries a **recommended
answer** so the human can confirm in one word — a question without a
recommendation is homework you assigned the human; do the thinking first.

Stamp every settled decision with a stable id (`D-1`, `D-2`, …) the moment it
is made and use the id thereafter ("per D-3, no auth"). D-ids never renumber
and never disappear from the written shape.

Interview in this order; stop as soon as the shape is complete:

1. **Outcome** — the observable end state in one or two sentences. "How will
   you know it worked?" is the question behind every outcome.
2. **Business rules (`BR-1`, `BR-2`, …)** — testable statements of how the
   product behaves. Numbering is per-Story; extend an existing Story's rules
   by continuing its numbers, never by reusing them.
3. **Exceptions (`E-1`, `E-2`, …)** — the failure behavior visible to users
   or callers.
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
pulse work new story "<title>" --epic EP-<id> --risk medium --surface api --from story.json --actor human:quan --json
```

Epic payload — only when the work does not fit under an existing Epic:

```json
{"outcome":"…","success_signals":["…"],
 "out_of_scope":["…"],"not_yet_specified":["…"]}
```

Story payload:

```json
{"outcome":"…",
 "rules":[{"id":"BR-1","text":"…"}],
 "exceptions":[{"id":"E-1","text":"…"}],
 "approach":"tracer-bullet sketch; SHOULD exist when risk >= medium",
 "qa_cases":[{"id":"QA-001","intent":"…","surface":"api","priority":"high",
              "steps":["…"],"expected":["…"]}],
 "open_questions":[{"q":"…","disposition":"resolved","answer":"…","ref":"D-2"}]}
```

`qa_cases[].check` (`{"argv":[…],"assert":[{"exit_code":0}]}`) is written only
when a mechanical oracle already exists or is a small `scripts/qa/cases/`
script — the QA oracle is harness, not product code, and a check beats an
interview. Otherwise omit `check`; the qa lane's agent covers the case.

Fix-ups after creation go through
`pulse work update <id> --from fix.json --actor …`; `revision` moves under
you, so re-read with `pulse work show <id> --json` before every update.

## 3. Write the prose home

The records are the truth layer; docs are where humans read the same shape:

- `docs/product/<slug>.md` (frontmatter `applies_to`/`tags`): one section for
  the Story — outcome, the BR/E tables, the QA cases, and each D-id noted
  where its decision landed.
- `docs/domain/glossary.md`: one line per term the interview settled, when
  new vocabulary appeared.
- `docs/README.md`: one line per new doc.

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

## Report

Report exactly these sections:

```markdown
## Shaped
- EP-… (draft) — <title> — new | existing
- ST-… (ready) — <title> — <outcome one-liner>

## Decisions
- D-1 <decision> — landed in BR-2
- D-2 <decision> — landed in open_questions (resolved)

## QA cases
- QA-001 (api, high) — <intent> — check: yes | no

## Open
- <question> — <disposition> — <what would sharpen it>

## Next
pulse-plan on ST-…
```
