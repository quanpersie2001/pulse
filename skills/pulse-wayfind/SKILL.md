---
name: pulse-wayfind
description: Turn a product or initiative too large for one agent session, with an unclear route from intent to buildable outcomes, into registered docs/product contracts and a decision frontier. Use proactively for greenfield products, broad ambiguous initiatives, or work whose business rules, exceptions, and open decisions are not yet sharp enough to shape Stories. Do not use for read-only analysis, a clear R0 change, an already-shaped Story, an existing implementation approach, or ticket slicing; send R0 through the ordinary Pulse workflow and use pulse-grill, pulse-spec, or pulse-planning for the narrower R1-R3 stages.
---

# Pulse Wayfind

Find the route; do not execute the destination. Finish with product contracts that state the destination and known rules, plus a frontier of decisions that must be resolved before planning can produce Epics and Stories.

Wayfinding owns no graph nodes and performs no lifecycle transition. `pulse-planning` is the only skill allowed to decide which nodes and dependency edges should exist.

Read these references when their subject becomes active:

- [`references/product-contract.md`](references/product-contract.md) when creating or revising a product contract.
- [`references/decision-frontier.md`](references/decision-frontier.md) when classifying fog, proposing decision work, or continuing an existing wayfinding effort.

## Establish Authority

1. Read `AGENTS.md` and `PULSE.md`. The Pulse-managed block in `AGENTS.md` owns the common workflow and R0 route.
2. Find existing product intent before drafting another source:

   ```text
   pulse docs search <topic> --kind product --include-draft --json
   pulse docs get <reference> --full --json
   ```

3. Inspect accepted Decisions, then code and tests for implementation facts. Treat accepted Decisions and approved product docs as intent, code and tests as implementation, receipts as observations, and other docs as explanation.
4. If sources disagree in a way that changes acceptance, an invariant, or a public contract, stop. Present the conflict and ask the human to decide; do not silently choose a source.
5. State whether this really needs wayfinding: the effort exceeds one useful session **and** the route is still unclear. If either condition is false, stop and report the narrower route. A clear R0 change returns to the ordinary bounded-change flow in the Pulse-managed `AGENTS.md` block; it does not invoke another skill merely to create ceremony. For R1-R3, name the narrower next skill.

Use repository file-editing tools only for authored Markdown and temporary command-input files. Send registry and graph mutations through literal `pulse` commands.

## 1. Name the destination

Ask one question at a time and include a recommended answer. Before asking, classify each unresolved input as either a **researchable fact** or a **product decision**. Report both categories, but ask only the next product decision; investigate facts from repository authority or route them to research instead of asking the human to guess.

Settle a destination before mapping the route. Express it in one or two sentences as the observable state that will exist when wayfinding is finished. The destination fixes scope; it is not an implementation plan.

Do not create a product contract until the human confirms the destination.

## 2. Chart breadth-first

Sweep the whole product surface before drilling into one thread:

- actors and outcomes;
- business rules and invariants;
- failure and exception behavior visible to users or callers;
- data, integrations, security, and operational constraints;
- explicit in-scope and out-of-scope boundaries.

Separate three things:

- **Known contract:** testable `BR-*` rules and `E-*` exceptions.
- **Sharp question:** precise enough to become `decision_work` through `pulse-planning`, even if it cannot be answered yet.
- **Fog:** in-scope uncertainty that cannot yet be phrased as one precise question; keep it under Open Questions until another answer sharpens it.

If the breadth-first pass surfaces no material fog, stop wayfinding. Route to `pulse-grill` to settle the meaning of the first behavior slice; graph shape follows later, from `pulse-planning`.

## 3. Write the product map

Create or update the smallest appropriate file under `docs/product/`. The product contract is the shared low-resolution map; it points to detailed Decisions, research, and work items rather than copying them.

Follow the exact four-section contract and stable-ID rules in `references/product-contract.md`. Preserve existing `BR-*` and `E-*` numbers. Mark withdrawn entries instead of renumbering or deleting them.

For a new document:

1. Write the Markdown and a temporary document-record JSON.
2. Read the current registry revision:

   ```text
   pulse docs status --json
   ```

3. Register the document using that revision:

   ```text
   pulse docs register --file <record-json> --expected-registry-revision <revision> --actor <actor> --json
   ```

Register an incomplete map as `draft`. Approve it only after its destination and current rules are human-confirmed and every remaining uncertainty is explicitly owned.

For an existing registered document, edit the authored Markdown in place. If its registry metadata or status must change, submit a patch:

```text
pulse docs edit <doc-id> --patch <patch-json> --expected-registry-revision <registry-revision> --expected-document-revision <document-revision> --actor <actor> --json
```

Then check the registry and content:

```text
pulse docs validate --json
```

## 4. Form the decision frontier

Turn only sharp questions into a proposed frontier. For each question, report:

- a human-readable title;
- one precise question;
- type: `grilling`, `prototype`, `research`, or `task`;
- expected decision-ready output;
- what it blocks and why;
- whether it is human-in-the-loop or agent-driven.

Ask the human to confirm the frontier and blocking relationships. Then invoke `pulse-planning` to create `decision_work` Tickets and wire dependencies. Wayfind itself never creates or rewires graph structure.

Research questions may invoke `pulse-research` after planning has provided a Ticket owner. Keep research findings under that Ticket and point to them from the product contract.

## 5. Continue one decision at a time

When continuing an existing map, load the product contract at low resolution and inspect the current decision work:

```text
pulse work list --role decision_work --json
pulse work show <ticket-id> --json
```

Choose one unblocked non-research question for the session. Use `pulse-grill` for a semantic decision, `pulse-research` for external facts, or a deliberately cheap prototype when discussion needs a concrete object. Do not answer the human side of a human-in-the-loop question.

After resolution, update the product contract with a one-line pointer to the owning Decision or Ticket. Graduate newly sharp fog into the proposed frontier and move newly excluded work to the ruled-out list. Detail must continue to live in exactly one owner.

Resolve at most one non-research decision per session. Parallel research is the exception because each research Ticket owns a separate file and question.

## 6. Exit wayfinding

Wayfinding is complete when:

- the destination remains confirmed;
- no blocking fog remains in the product contract;
- every precise open question is resolved or explicitly deferred with an owner;
- no nonterminal `decision_work` Ticket remains on the route;
- product docs validate.

Hand the confirmed product contracts to `pulse-grill`, which settles the meaning of one behavior slice before any graph shape is proposed. Epic and Story boundaries come later, from `pulse-planning`, once `pulse-spec` has also fixed the approach. Do not start implementation or skip directly to execution.

## Report

Report exactly these sections:

```markdown
## Destination
<confirmed destination>

## Product contracts
- <DOC-ID> — <path> — <draft|approved>

## Decision frontier
- Ready questions: <titles or none>
- Fog: <items or none>
- Ruled out: <items or none>

## Authority
- Confirmed: <sources>
- Facts to research: <items or none>
- Product decisions for human: <items or none>
- Conflicts requiring human decision: <items or none>

## Next
<one next skill or human decision; never implementation>
```
