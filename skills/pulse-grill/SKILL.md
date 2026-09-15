---
name: pulse-grill
description: Settle what one behavior slice actually means before any work item exists, by interviewing the human one question at a time and writing the settled meaning into a story.md draft under works/_drafts/ and into the domain glossary. Use it whenever a request is about to become real work but its outcome, success signals or scope boundary are still fuzzy, whenever pulse-wayfind hands over a slice of a product contract to sharpen, and whenever the same term is being used in two senses. Do not use it for mapping a whole product or initiative (pulse-wayfind), for choosing a solution, seams or test strategy (pulse-spec), for deciding which Epics, Stories or Tickets should exist (pulse-planning), for gathering external facts (pulse-research), or for a bounded R0 change whose meaning is already agreed.
---

# Pulse Grill

Settle the meaning of one behavior slice. Finish with prose a later session can execute against without relitigating what the words meant: an outcome, observable success signals, a scope boundary, settled vocabulary, and every ambiguity given a disposition.

Grilling happens before the work graph has anything in it:

```text
pulse-wayfind -> pulse-grill -> pulse-spec -> pulse-planning -> pulse run
```

That ordering is the point. Drawing work-item boundaries before the boundary is understood is how horizontal tickets get created, so `works/_drafts/<slug>/` exists precisely to hold reviewed prose while no node does. `pulse-planning` is the only skill that decides which nodes exist; grill decides only what the slice means.

Two consequences follow from working before the graph, and both change how carefully you have to write:

- **There is no node to transition.** Grill does not move anything to `shaped`. The shaping happens in the prose; the state change happens much later, to Tickets, in `pulse-planning`.
- **No gate will catch a weak `story.md`.** The `shaped` profile evaluates one family, `ticket_ambiguity`, and that family returns *not applicable* for anything without an implementation role — which includes every Story. A vague outcome or an undispositioned question at this altitude is caught by you and the human here, or it is not caught at all. Read the draft back as a hostile reader before you call it settled.

Read these references when their subject becomes active:

- [`references/question-discipline.md`](references/question-discipline.md) when running the interview: classifying facts against decisions, shaping one question, handling scope creep, and knowing when to stop asking.
- [`references/story-draft.md`](references/story-draft.md) when writing the draft: the exact section contract, the disposition table, and what makes a success signal observable.

Use repository file-editing tools for authored Markdown — `story.md` and the glossary are authored prose. Send registry mutations through a literal `pulse` command.

## Establish Authority

1. Read `AGENTS.md` and `PULSE.md`. The Pulse-managed block in `AGENTS.md` owns the common workflow and the R0 route.
2. State whether this slice needs grilling at all. It does when the request could be delivered in more than one way that a reasonable person would call correct, or when two people in the conversation are plainly meaning different things by the same word. It does not when the outcome is already agreed and only the implementation is open — that is `pulse-spec`, or for a bounded change the ordinary R0 flow written in the Pulse-managed block in `AGENTS.md`. When you decline, name that block as the route and give the reason as *the meaning is already settled*, not merely that the change looks small: size is not the test, and a small change with a contested meaning still needs grilling. Holding an interview over a settled meaning is ceremony, and it teaches the human that grilling wastes their time.
3. Find what is already settled before asking anyone anything:

   ```text
   pulse docs search <topic> --kind product --include-draft --json
   pulse docs get <reference> --full --json
   pulse docs get DOC-GLOSSARY --full --json
   pulse work list --kind story --json
   ```

   A product contract from `pulse-wayfind` carries the destination, the `BR-*` rules and the `E-*` exceptions this slice sits inside. An existing Story may already cover the behavior, in which case say so and stop rather than starting a second draft of the same meaning.
4. Check whether a draft already exists at `works/_drafts/`. Resuming one is normal: grilling spans sessions, and a half-settled draft is the record of where the last one stopped. Re-read its `## Decisions` before reopening anything in it.
5. Treat accepted Decisions and approved product docs as intent, code and tests as implementation, and receipts as observations. If sources disagree in a way that changes what "done" would mean, stop, present the conflict, and let the human choose. Do not silently pick a source.

## 1. Fix the slice and its slug

Name the one behavior this session is about, and say out loud what you are leaving out. A request that spans two behaviors provable on their own is two drafts, not one — deciding that here is far cheaper than discovering it in `pulse-planning`, where the prose would have to be split after it was written.

Propose a `<feature-slug>`: lowercase, hyphenated, named for the behavior rather than the mechanism, short enough to type. Confirm it with the human, because `pulse-spec` will address the same directory in a different session and a slug nobody remembers becomes a second draft of the same work.

Create `works/_drafts/<slug>/` only once the human has confirmed the slug and the slice.

## 2. Separate what you can look up from what only the human knows

Before each question, classify the uncertainty:

- **Researchable fact** — the repository, the docs, the tests or an external source already answers it. Go and find out. Asking the human to guess at something the code states is how a wrong answer enters the record with a human's authority behind it.
- **Product decision** — intent, preference, priority, authority, or a trade-off with no objectively better side. Only these are worth the human's turn.

Report both categories so the human can see you did the reading, then ask only about the second. If a fact needs a source outside the repository, note it and route it to `pulse-research`, which can write under `works/_drafts/<slug>/research/<topic>.md` while no Ticket exists to own it.

## 3. Lock one decision per turn

Ask one question per message and wait. Bundling three questions gets you one answer to whichever was easiest and silence on the two that mattered.

Each question carries a recommended answer with the reason for it, so the human can correct a specific proposal instead of authoring a specification from scratch. Prefer a question with named alternatives over an open one.

When the human answers, confirm the decision back in one line and give it a stable ID — `D1`, `D2`, `D3`. The ID is what later prose, `approach.md` and ticket acceptance can cite, and it stays stable even when the wording around it is rewritten. Start broad — what the outcome is — and narrow into constraints and edge behavior once the shape holds.

When the human raises something outside the slice, record it as deferred with an owner and return to the open question. Following it is how one interview becomes three.

Ask nothing further once every remaining uncertainty either has a disposition or is a researchable fact. Grilling past that point is not thoroughness; it is asking the human to design the implementation, which belongs to `pulse-spec`.

Do not act on a decision before the human confirms it. No file is written from an answer you assumed.

## 4. Record vocabulary the moment it settles

When a term gets a fixed meaning, write it into `docs/domain/glossary.md` in the same turn. The next session starts from the word, not from the argument that produced it, and the cost of recording it later is relitigating it.

The glossary is vocabulary only: what a word denotes here. Rules, state machines and error taxonomies live in their own documents, and a glossary entry that explains how something works is the beginning of a second source of truth for the mechanism.

`pulse init` seeds and registers `DOC-GLOSSARY`, so edit the Markdown in place — no registry command is needed to change its content. Then confirm the registry still agrees with the tree:

```text
pulse docs validate --json
```

If `DOC-GLOSSARY` does not exist, the repository predates the current `pulse init`. Register it rather than inventing another location:

```text
pulse docs status --json
pulse docs register --file <record-json> --expected-registry-revision <revision> --actor <actor> --json
```

## 5. Write the draft

Write `works/_drafts/<slug>/story.md` following the contract in [`references/story-draft.md`](references/story-draft.md): Outcome, Success signals, Scope boundary, Vocabulary, Decisions, Open questions.

The draft grows through the interview rather than appearing at the end of it. Each confirmed decision goes in as it is locked, in the same turn it was confirmed. Holding everything in the transcript until the slice is fully settled means a session that ends early — context exhausted, the human called away, a handoff — leaves nothing behind, and the next session re-asks questions that were already answered. This step is where the file is completed and read back, not where it is started.

Two sections carry most of the weight downstream:

- **Success signals** must be observable without reading the implementation. `pulse-spec` turns them into QA cases and `pulse-planning` cuts Tickets against them, so a signal phrased as "the code is clean" gives both of them nothing to work with.
- **Open questions** each need one of the five dispositions from the ambiguity gate in `PRODUCT.md` §5.1 — `resolved`, `rejected`, `delegated`, `deferred`, `blocking` — with the evidence that disposition requires. These are copied onto the Story and inherited by the Tickets cut from it, where `blocking` genuinely refuses `ready`. Writing them properly here is what makes that gate mean something later.

A `blocking` question at this altitude means grilling is not finished. Say so plainly in the report instead of handing `pulse-spec` a draft it cannot build an approach on.

Then read the draft back as someone who was not in the conversation. The test is whether that reader could tell you what would be true when the work is done, and what is deliberately not being done. Anything they could not — a placeholder, a preference with no reason, a term used in two senses — is the next question, not a formatting problem.

## 6. Propose a Decision only when it earns one

A settled answer becomes a Decision node when all three hold:

- it is **hard to reverse** — later work will be built on top of it;
- it is **hard to understand without context** — the reason will not survive as a one-liner;
- it has a **real trade-off** — the alternative was defensible and was rejected for a stated reason.

Miss any one and the answer belongs in `## Open questions` as `(resolved)` with its reason. A Decision node created for an easy answer costs a permanent acceptance ceremony and buries the decisions that mattered in a list of ones that did not.

Grill never creates the node. Record the proposal in the draft — the question, the answer, the alternative rejected, and which of the three conditions it meets — and hand it to `pulse-planning`, which owns every node in the graph and will create it along with the rest of the shape. This is the same handoff `pulse-wayfind` makes for its decision frontier: the judgment is made where the context is, and the graph is written where the graph is owned.

## 7. Hand off

Grilling is complete when the outcome is confirmed, the success signals are observable, the scope boundary names what is excluded and where it went, every settled term is in the glossary, and no question is `blocking`.

Hand the draft to `pulse-spec`, which fixes the approach and the QA baseline in the same directory without interviewing again. Do not propose a breakdown, design the solution, or start implementing — each belongs to a later skill, and doing it here means it was decided before the meaning was settled.

## Report

Report exactly these sections:

```markdown
## Slice
<confirmed behavior slice and slug>

## Draft
- works/_drafts/<slug>/story.md — <written | updated>

## Decisions locked
- D1 — <decision> — <why>

## Vocabulary
- <term> — <written to glossary | already settled>

## Proposed Decision nodes
- <question> — <which of the three conditions it meets> — <or none>

## Open questions
- <disposition> — <question> — <what it rests on>

## Authority
- Confirmed: <sources>
- Facts to research: <items or none>
- Conflicts requiring human decision: <items or none>

## Next
<one next skill or human decision; never implementation>
```
