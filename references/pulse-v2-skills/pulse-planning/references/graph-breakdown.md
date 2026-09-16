# Graph breakdown

How to propose a cut, check it against work that already exists, and turn an approved proposal into nodes and edges without inventing structure along the way.

## Proposal shape

Propose in readable prose. IDs do not exist yet, and a proposal written in IDs cannot be reviewed.

```markdown
### <Readable title>
- Kind: <epic|story|ticket>
- Role: <implementation|decision_work> (Tickets only)
- Parent: <readable title or none>
- Delivers: <the observable behavior or answer this item produces>
- Blocked by: <readable titles, or "none — can start immediately">
- Risk: <low|medium|high|critical> → materialization <R0|R1|R2|R3>
```

`Delivers` is the field that exposes a bad cut. If it can only be written as a layer ("adds the database column", "writes the API handler"), the item is a horizontal slice and the cut is wrong. If it needs three sentences and an "and then", the item is two items.

State risk explicitly. It decides how much contract the Ticket needs, and a Ticket created without a stated risk silently inherits the cheapest level.

## Choosing the kind

| Kind | Exists when | Does not exist for |
|---|---|---|
| Epic | A settled product slice with an outcome and an investment boundary | An investigation, a theme, a layer, or a convenient folder for unrelated work |
| Story | One independently provable behavior slice that can own a QA baseline | A grouping of Tickets that share a file or a sprint |
| Ticket `implementation` | One agent can execute and verify it inside one context window | Work that cannot be demonstrated until a second Ticket also lands |
| Ticket `decision_work` | One precise question gates a behavior slice | Fog whose question is still unstable, or work that delivers the product |

Hierarchy is optional. A standalone Ticket is valid, and inventing a parent to make the graph look tidy adds a node nobody reads. Create an Epic or a Story because it carries something — an investment boundary, a QA baseline — not because a level felt missing.

## Reuse before create

The cheapest planning outcome is discovering the node already exists. Before proposing anything, read what is there:

```text
pulse work list --kind story --json
pulse work list --role decision_work --json
pulse work show <id> --json
pulse graph neighborhood <id> --depth 2 --json
```

Compare by acceptance, not by title. Two items with different titles that would be proved by the same observable behavior are one item.

- **Covered already** — report it under Reused and drop it from the proposal.
- **Partly covered** — propose extending the existing Ticket's contract instead of creating a near-duplicate, unless the two halves are provable independently.
- **Superseded in substance** — do not quietly create a replacement. Say so in the proposal and let the human decide whether the old node should be superseded through its owning workflow.

## Real dependencies only

A `blocked_by` edge means the blocker must reach a terminal state before the dependent can execute. It is the expensive edge: it removes the dependent from the ready set.

Genuine gate:

- the dependent consumes an interface, schema or decision the blocker creates;
- the dependent cannot be demonstrated until the blocker exists.

Not a gate:

- both items touch the same file;
- one is more natural to do first — that is `preferred_after`, which suggests order without blocking;
- a human prefers to review them in sequence.

Over-wiring produces a graph where nothing is ready and the order is invisible, which is worse than no edges at all. When unsure, leave the edge out and say so in the proposal; a missing soft order costs a sentence, a false hard block costs a stalled frontier.

## Create, then wire

Two passes, in this order, because an edge needs both endpoints to exist:

1. **Create.** Parents and blockers first, then the rest. `--parent` may be passed at creation since the parent already exists; every `blocked_by` waits.
2. **Wire.** Add each approved dependency edge, then validate the graph.

Edge IDs are deterministic from type, source and target, so adding the same edge twice is harmless — but a dangling edge fails validation, which is why the passes cannot be interleaved with creation of the endpoints.

Never derive an edge from the order the items appeared in the proposal list. A numbered list is a reading order, not a dependency graph, and a chain invented from it blocks work that was independent.

## Report what did not become a node

The items left out matter as much as the ones created. Close the report with what stayed in prose and who owns it — fog in the product contract's Open Questions, a question deferred with an owner, a behavior ruled out of the destination. Otherwise the next session cannot tell the difference between "considered and left" and "forgotten".
