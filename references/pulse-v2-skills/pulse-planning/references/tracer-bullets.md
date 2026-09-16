# Tracer bullets

How to slice one shaped Story into implementation Tickets: vertical cuts, context sizing, the wide-refactor exception, and the traceability the close gate will ask for.

## Vertical, not horizontal

A tracer bullet cuts a narrow but complete path through every layer the behavior needs — schema, service, interface, tests — so that finishing it proves something works end to end.

- A completed slice is demonstrable or verifiable on its own.
- Each slice fits one fresh context window: a worker can read the packet, do the work, and verify it without running out of room.
- Prefactoring comes first, as its own Ticket. Make the change easy, then make the easy change.

Horizontal slicing feels tidier and fails predictably: every layer Ticket is "done" while no behavior works, nothing can be verified until the last one lands, and a mistake in the first layer is discovered after four Tickets were built on it.

Two useful tests for a slice:

- **Demonstration test.** Say what you would show a human when it is done. If the answer is "the column exists", the slice is horizontal.
- **Context test.** If a fresh agent would need to hold two subsystems, two decisions and a migration in mind at once, split it.

## Sizing against risk

Risk sets how much contract the Ticket needs, and the contract is what a worker gets instead of the conversation that produced it.

| Risk | Typical slice | Contract weight |
|---|---|---|
| low | One obvious change, one verification | Objective, acceptance, anchors, verify |
| medium | One behavior across a couple of modules | Full contract, invariants, scope and non-scope |
| high / critical | Architecture, migration, security, destructive change | Full contract plus a required Decision, rollback and deeper QA |

Raising materialization when slicing reveals risk is normal. Lowering it needs a recorded reason. Neither is a reason to create an artifact nobody will read.

## The wide refactor exception

A wide refactor is one mechanical change — rename a shared symbol, retype a column — whose blast radius fans across the codebase, so a single edit breaks thousands of call sites and no vertical slice can land green. Do not force it into a tracer bullet. Sequence it expand–contract:

1. **Expand.** Add the new form beside the old so nothing breaks. One Ticket, no blockers.
2. **Migrate.** Move call sites in batches sized by blast radius — per package, per directory — each batch its own Ticket blocked by the expand. The old form still exists, so each batch stays green.
3. **Contract.** Delete the old form once no caller remains, in a Ticket blocked by every migrate batch.

When even a batch cannot stay green alone, keep the sequence but let those Tickets share an integration branch and all block a final integrate-and-verify Ticket. Green is promised there, and the proposal should say so plainly rather than implying each batch is independently verifiable.

## Traceability the gate will ask for

The thread runs `BR-*` → Story → `QA-*` → `AC-*` → check → receipt. Slicing is where the middle of it is either connected or lost.

- **Acceptance IDs.** Every acceptance item carries an `AC-*` identifier, because verification binds proofs to those IDs. An acceptance item that cannot be phrased as an observable condition is not acceptance; it is scope.
- **Rule citation at R2/R3.** Each `AC-*` cites the `BR-*` or `E-*` it exercises, drawn from the product contract the Story came from. A rule or exception in scope with no acceptance citing it is unverified. R0 and R1 work does not inherit this.
- **QA impact.** When the Story owns a QA baseline, name the owner Story and the specific `QA-*` case IDs the Ticket touches, with the reason. An unknown posture refuses `ready`; `none` needs a rationale.
- **Docs impact.** Name the document IDs a change invalidates, or state why durable docs are unaffected.

Cite IDs; do not copy the rule text. The product contract owns the rule, the Ticket owns the acceptance that exercises it, and duplicating prose creates two places to change one behavior.

## Writing the contract

Aim at the behavior, from the perspective of whoever experiences it. Layer-by-layer instructions date faster than the code and leave a worker with no way to judge an unforeseen case.

- Give code anchors so a worker can orient — files and the relevant seam, not a line-by-line plan.
- State invariants that must survive: the public contract, the error envelope, the rotation nobody should touch.
- Set implementation freedom honestly. `locked` when a Decision fixed the approach, `guided` when anchors and invariants bound it, `open` when the agent chooses inside a boundary. An `open` Ticket whose uncertainty could change acceptance is not open; it needs a decision first.
- Avoid pasting code. The exception is a snippet that encodes a decision more precisely than prose can — a state machine, a schema, a type shape — trimmed to the decision-rich part and marked as coming from a prototype or Decision.

## Working the frontier

Order the created Tickets so blockers come first, and let the ready set decide what runs next rather than the numbering. Dispatch one Ticket at a time with fresh context. A breakdown whose Tickets can only be executed in exactly the listed order has hidden a dependency chain in the prose instead of wiring it.
