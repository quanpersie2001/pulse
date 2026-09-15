---
name: pulse-spec
description: Fix how one settled behavior slice will be built and how it will be proven, by reading the codebase rather than reinterviewing the human, and writing approach.md and qa.md beside the story.md that pulse-grill settled. Use it whenever a draft under works/_drafts/ has a story whose meaning is agreed but whose solution, seams or test strategy are still open, and whenever a Story needs a QA baseline before it can own one. Do not use it for settling what the work means (pulse-grill), for mapping a whole product or initiative (pulse-wayfind), for deciding which Epics, Stories or Tickets should exist (pulse-planning), for gathering external facts (pulse-research), or for implementing, reviewing or closing work.
---

# Pulse Spec

Fix the solution and the proof for one settled slice. Finish with an approach a worker can execute against and a QA baseline the close gate can parse, both written beside the story that produced them.

Spec sits at the hinge of the chain:

```text
pulse-wayfind -> pulse-grill -> pulse-spec -> pulse-planning -> pulse run
```

Everything before it is about intent; everything after it is about work. It is the first skill that reads the codebase, and the last one where the plan is still cheap to change.

Two consequences follow, and both change how the session is spent:

- **The interview is over.** `story.md` arrives with its decisions locked and its vocabulary settled. Spec reads those as input, not as an opening position. Reopening a settled decision costs more than the time it takes — it teaches the human that answering grill's questions bought them nothing.
- **No gate will check `qa.md` here.** `pulse qa baseline` resolves the canonical baseline of a Story *node*, and during spec no node exists. The baseline written here is checked by you against the contract in [`references/qa-baseline.md`](references/qa-baseline.md), or it is checked for the first time in `pulse-planning`, which refuses to adopt a baseline the parser rejects.

Read these references when their subject becomes active:

- [`references/approach-draft.md`](references/approach-draft.md) when writing the approach: the section contract, what makes a seam real, and how to record a rejected alternative.
- [`references/qa-baseline.md`](references/qa-baseline.md) when writing the baseline: the parsed heading and field vocabulary, the `pulse-check` block, and what makes a case survive a refactor.

Use repository file-editing tools for authored Markdown — `approach.md` and `qa.md` are authored prose. Spec sends no mutation through the CLI at all; every command below is a read.

## Establish Authority

1. Read `AGENTS.md` and `PULSE.md`. The Pulse-managed block in `AGENTS.md` owns the common workflow and the R0 route.
2. Find the draft. Spec accepts one input: `works/_drafts/<slug>/story.md`, settled by `pulse-grill`.

   If it is absent, name `pulse-grill` and stop. Designing a solution for a slice whose meaning is still moving is how an approach gets written twice, and the second writing is the expensive one.

   If a `blocking` open question survives in the draft, grilling is not finished. Return it rather than designing around an answer nobody gave.

   If `approach.md` or `qa.md` already exist, you are resuming. Read them and their `## Decisions` before changing anything in them.
3. State whether this slice needs spec at all. It does when the solution could take more than one defensible shape, when the change crosses a seam, or when the Story will own a QA baseline. It does not when the route is already obvious and the work is one bounded change — that is the R0 flow in the Pulse-managed `AGENTS.md` block, and holding a design session over it is ceremony.
4. Read what constrains the solution before designing one:

   ```text
   pulse docs search <topic> --kind architecture --json
   pulse docs search <topic> --kind domain --json
   pulse docs get <reference> --full --json
   pulse work list --kind story --json
   pulse knowledge list --json
   ```

   Architecture documents carry the boundaries and the dependency direction a solution may not cross. Learnings carry what previous work in this area got wrong; an approach that re-proposes something a learning warns against is the ratchet failing silently.
5. Treat accepted Decisions and approved product docs as intent, code and tests as implementation, and receipts as observations. If sources disagree in a way that changes what the solution must do, stop, present the conflict, and let the human choose.

## 1. Read the code before proposing anything

Most of this session is reading. The value spec adds over grill is that its output is answerable by the repository: real file paths, real seams, real names.

Work from the story outward. For each success signal in `story.md`, find where that behavior lives today, or where it would have to live. Name the modules, the entry points, the existing patterns the change should follow, and the tests that already cover the area.

Say plainly what you could not find. An approach that names a seam nobody located is a guess wearing a file path, and a worker will discover that at the worst moment — after the packet is committed and the lease is held.

## 2. Fix the seams

A seam is a place the change enters the system: the module that gains a responsibility, the interface that gains a case, the boundary the new behavior must not cross. Fixing them is the decision this skill exists to make, because `pulse-planning` cuts Tickets along them.

For each seam, settle three things:

- **Reuse or build.** What already exists that this should extend rather than duplicate. Duplication proposed here becomes duplication a reviewer has to argue about later.
- **What must survive.** The public contract, the error envelope, the invariant that has nothing to do with this change and must still hold after it. These become the Ticket invariants.
- **Forced order.** Where one piece genuinely cannot land before another. This is what planning turns into `blocked_by`; everything else is preference, and calling a preference a dependency serializes work for nothing.

Record the shape you rejected as well as the one you chose. The alternative is the part a later reader needs most, and it is the part that disappears if nobody writes it down in the turn it was rejected.

## 3. Ask once, and only about what the repository cannot settle

Grill asks one question per turn because each answer changes the next one. Spec's questions are different: they are independent trade-offs at seams, they can be judged together, and the human has already spent their turns. So gather them and ask **once**, after the reading, before the writing.

A question belongs in that batch only when the repository cannot answer it and the answer changes the solution — a trade-off with no objectively better side, a preference about an externally visible shape, an authority question about what may be touched. Anything the code, the tests or the docs state is yours to go and find out.

Carry a recommended answer with its reason for each, so the human corrects a proposal instead of authoring a design.

If a fact needs a source outside the repository, note it and route it to `pulse-research`, which writes under `works/_drafts/<slug>/research/<topic>.md` while no node exists.

If the reading shows a settled decision cannot be built as the human understood it, that is not a second interview. Name the specific decision ID, say what the code shows, and return the slice to `pulse-grill`. One escalation with evidence is honest; reopening the interview generally is not.

## 4. Write the approach

Write `works/_drafts/<slug>/approach.md` following the contract in [`references/approach-draft.md`](references/approach-draft.md): Solution, Seams, Reused, Invariants, Alternatives rejected, Risks, Out of scope, Open questions.

Write it as you settle it, not at the end. A session that ends early — context exhausted, the human called away — should leave the seams it fixed behind, not a transcript nobody will reread.

Aim it at a worker who was not in this conversation and will read it through a packet. Name files and seams; do not write a line-by-line plan. Layer-by-layer instructions date faster than the code and leave a worker with no way to judge a case you did not foresee.

## 5. Write the QA baseline

Write `works/_drafts/<slug>/qa.md` following the parsed contract in [`references/qa-baseline.md`](references/qa-baseline.md). This file is not prose: `pulse qa baseline` parses it, its headings and field keys are a fixed vocabulary, and an unrecognised `Key:` line is refused by name.

Cases come from the success signals in `story.md`, not from the approach you just wrote. A case that names a function, a selector or an internal structure passes until the first refactor and then fails for a reason that has nothing to do with the behavior it was supposed to protect. Write what an observer outside the system would see.

Keep the boundary clear. `qa.md` answers *does the behavior this Story promised still hold on this snapshot*. The `Verify` commands inside each `ticket.md` answer *does this Ticket satisfy its technical contract*, and `pulse-planning` writes those. The same test can serve both; the intent, the actor and the receipt differ, and only the first belongs here.

Give a case a `pulse-check` block when its surface is `cli` or `api` and the assertion vocabulary can express it. That is what lets `pulse run qa` prove the case mechanically instead of returning `inconclusive`.

## 6. Check the baseline by hand

Nothing validates this file until a Story node owns it, so read it against the contract deliberately:

- `## Posture` holds one of `automated`, `hybrid`, `manual_structured`, `static_proof`, `not_applicable`. `required` is a Ticket-level QA *impact* posture and is refused here.
- Every case has a `### QA-NNN <title>` heading and carries at least Intent, Surface, Steps and Expected.
- Every field key is in the vocabulary; a helpful extra line is a parse failure.
- A case marked `Applicability: not_applicable` carries a `Reason`.
- A `pulse-check` block appears at most once per case, and only on surface `cli` or `api`.

A baseline that is a list of one-line case titles will be refused at adoption. Finding that here costs a paragraph; finding it in `pulse-planning` changes the plan.

## 7. Hand off

Spec is complete when every success signal has a seam that implements it and a case that proves it, the invariants are named, the rejected alternatives carry their reasons, and no question is `blocking`.

Hand the draft to `pulse-planning`, which owns every node in the graph and will cut Tickets along the seams fixed here. Do not propose a breakdown, create a node, or start implementing: a cut proposed before the graph is owned is a cut that has to be re-argued where it can actually be made.

## Report

Report exactly these sections:

```markdown
## Slice
<draft slug and the story it specs>

## Drafts
- works/_drafts/<slug>/approach.md — <written | updated>
- works/_drafts/<slug>/qa.md — <written | updated>

## Seams
- <seam> — <reuse or build> — <what must survive>

## Forced order
- <piece> before <piece> — <why it is genuinely gated, or none>

## Alternatives rejected
- <shape> — <why>

## QA baseline
- Posture: <value>
- Cases: <QA-001 …> — <how many carry a pulse-check block>
- Hand-check: <passed | what failed>

## Asked
- <question> — <answer given, or unanswered>

## Escalated
- <decision ID returned to pulse-grill, external fact routed to pulse-research, or none>

## Open questions
- <disposition> — <question> — <what it rests on>

## Next
<one next skill or human decision; never implementation>
```
