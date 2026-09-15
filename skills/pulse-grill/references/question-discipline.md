# Running the interview

Grilling is a relentless interview, not a questionnaire. Its value is that the
human's attention is spent only on things nobody else can answer, and that every
answer given is recorded in a form the next session can rely on.

## Classify before you ask

Every uncertainty is one of two things, and mixing them up is the most expensive
mistake available here.

**Researchable fact** — the repository, the docs, the tests, a receipt or an
external source already determines the answer. Go and find it:

```text
pulse docs search <topic> --kind product --include-draft --json
pulse docs get <reference> --full --json
pulse docs tree <path> --json
pulse work list --kind story --json
pulse work show <id> --json
```

Plus the code and tests themselves. Asking the human what the current behavior
is invites a recollection, and a wrong recollection now carries the human's
authority into every artifact downstream.

**Product decision** — intent, preference, priority, authority, or a trade-off
where no side is objectively better. These need the human, and only these.

Report both lists. Showing the facts you established is what earns the human's
patience for the questions that remain, and it makes visible any fact you got
wrong while it is still cheap to correct.

When a fact needs a source outside the repository, do not guess and do not ask
the human to be the source. Note it and route it to `pulse-research`, which can
write under `works/_drafts/<slug>/research/<topic>.md` while no Ticket exists to
own the question.

## Shape of a question

One question per message. Then wait.

Three properties make a question cheap to answer:

1. **It names alternatives.** "Should a failed signature block the publish, or
   publish with a warning banner?" beats "How should signature failures work?"
   An open question asks the human to author a specification; a closed one asks
   them to make a decision, which is the thing you actually need.
2. **It carries a recommendation with a reason.** "I'd block the publish,
   because a warning that can be clicked past makes the signature advisory and
   `BR-04` reads as mandatory." The human can now correct one specific claim
   instead of starting from nothing, and correcting is much faster than
   composing.
3. **It stays at product altitude.** How it will be built is `pulse-spec`'s
   question. Asking it here produces an implementation commitment made before
   the behavior was settled.

Start broad — the outcome, then the boundary — and narrow into constraints and
edge behavior once the shape holds. Narrowing first produces precise answers
about a slice that turns out to be the wrong slice.

## Lock each answer

Confirm the decision back in one line and give it a stable ID: `D1`, `D2`, `D3`.

The ID is the point. `approach.md`, a ticket's acceptance, a later Decision and
the next grilling session can all cite `D2` and mean the same thing, and the ID
survives the wording around it being rewritten. Reusing or renumbering an ID
silently breaks every citation, so IDs are append-only within a draft — a
reversed decision gets a new ID that says what it supersedes.

Write the confirmation into the draft in the same turn. An answer that lives
only in the transcript is lost at the next handoff, and re-deriving it costs a
second interview about a question the human already settled.

## Scope creep

The human will raise things outside the slice. That is useful information, not
an interruption.

Record it under Scope boundary as out of scope, with where it went — deferred
with an owner, another slice, or ruled out with a reason — and return to the
question that was open. Following it is how one interview becomes three and the
original slice ends up half-settled.

If what they raised reveals that the slice itself was drawn wrong, say so
explicitly and redraw it with them. That is a different move from chasing an
adjacent idea, and it is worth doing before more decisions are locked against a
boundary that will not hold.

## When to stop

Stop asking when every remaining uncertainty either has a disposition or is a
researchable fact you can settle yourself.

Continuing past that point is not thoroughness. It reads to the human as an
inability to commit, and the questions available at that point are almost always
implementation questions in disguise — which belong to `pulse-spec`, after the
approach has a shape to hang them on.

Stop early, before the draft is complete, when a question is genuinely
`blocking`: nobody present can answer it and it would change what "done" means.
Record it with what it would change and who must answer, report it, and say
plainly that grilling is unfinished. Handing `pulse-spec` a draft with a hidden
blocking question produces an approach built on a guess.

## Anti-patterns

- **Bundled questions.** You get an answer to the easiest one and silence on the
  two that mattered.
- **Answering your own question.** A recommendation invites correction; an
  answer stated as settled records your assumption as the human's decision.
- **Acting before confirmation.** No file is written from an answer you assumed.
- **Deep implementation analysis.** Reading code to establish current behavior
  is research; designing the change is `pulse-spec`.
- **Proposing architecture.** Seams and structure are `pulse-spec`'s output, and
  proposing them here fixes the solution before the problem is settled.
- **Proposing a breakdown.** Which Epics, Stories and Tickets should exist is
  `pulse-planning`'s decision, and it is a better one made after the approach is
  known.
- **Creating graph nodes.** Nothing in this session creates a node, a dependency
  edge or a lifecycle transition — including the Decision nodes grill proposes.
- **Writing code.** Nothing here is implementation.
- **Skipping the lock.** A decision without an ID and a line in the draft is a
  decision that will be made again.
