# Decision frontier

Adapt the useful parts of Matt Pocock's Wayfinder to Pulse without importing an issue tracker or a second control plane.

## Destination before frontier

A frontier has meaning only relative to a destination. Confirm the destination first, then judge every question against it. Work beyond the destination is out of scope, not fog.

## Sharp question or fog

The test is whether the question can be stated precisely **now**, not whether it can be answered now:

- A precise question belongs in a proposed `decision_work` frontier.
- An uncertainty whose question is still unstable remains in the product contract's Open Questions.
- A known product behavior belongs in `BR-*` or `E-*`, not in either bucket.
- A consciously excluded behavior belongs under Ruled out and never graduates unless the destination changes.

Do not pre-slice fog. One vague area may later become several questions or disappear after another decision.

## Decision-work types

| Type | Driver | Use when | Decision-ready output |
|---|---|---|---|
| `grilling` | Human in the loop | Product intent or a trade-off must be chosen | Confirmed answer, with a Decision when costly to reverse |
| `prototype` | Human in the loop | Discussion needs a cheap concrete object | Referenced prototype plus the decision it enabled |
| `research` | Agent-driven | A fact outside current repository authority blocks a choice | Cited primary-source research file |
| `task` | Agent or human | Manual work must happen before a decision can be made | Facts or access produced by the task, not destination delivery |

A task earns a place only by unblocking a decision. If it delivers the destination, wayfinding has crossed into implementation.

## Frontier proposal

For each sharp question, propose:

```markdown
### <Readable title>
- Question: <one exact question>
- Type: <grilling|prototype|research|task>
- Driver: <human-in-the-loop|agent-driven>
- Expected output: <decision-ready artifact or answer>
- Blocked by: <titles or none>
- Blocks: <titles and reason>
```

Refer to work by readable title in narration. Include IDs as references after they exist, but do not make a bare ID carry the meaning.

The proposal is not graph truth. Ask the human to confirm its grain and dependencies, then hand it to `pulse-planning`, the sole owner of node creation and dependency wiring.

## Working discipline

- Load the product contract first; open detailed work only on demand.
- Resolve one non-research question per session.
- Never let an agent answer the human half of a human-in-the-loop question.
- Research may run in parallel only after each question has its own Ticket owner.
- After a resolution, put detail in one owner and add only a one-line pointer to Decisions so far.
- Revisit fog after each answer. Graduate only what has become precise.
- Close or supersede wrongly scoped decision work through the owning workflow; record the ruled-out gist in the product contract.

## Completion test

The route is clear when there is no blocking fog, no unresolved in-route decision work, and the known product behavior is expressed as stable rules and exceptions. At that point hand the contract to `pulse-grill` to settle the meaning of the first behavior slice; Epic and Story boundaries follow later from `pulse-planning`. Do not build from the map.
