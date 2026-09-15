# Approach draft

The section contract for `works/_drafts/<slug>/approach.md`, what makes a seam real, and how to record an alternative so it stays useful.

The approach is read twice: once by `pulse-planning`, which cuts Tickets along its seams, and once by a worker, who reaches it through the packet as the Story's shared approach. Write for both. Planning needs the seams and the forced order; the worker needs the invariants and the reasons.

## Section contract

```markdown
# <slug> approach

## Solution
<the shape in a few sentences: what changes, where, and why this shape>

## Seams
- <module or boundary> — <reuse or build> — <what enters here>

## Reused
- <existing component> — <what it already does that this extends>

## Invariants
- <what must still hold after the change, and has nothing to do with it>

## Alternatives rejected
- <shape> — <why it was defensible> — <why it lost>

## Risks
- <what could go wrong in execution> — <the cheapest early signal>

## Out of scope
- <deliberately excluded> — <where it went, or why nowhere>

## Open questions
- (resolved) <question> — <answer and its reason>
- (deferred) <question> — <owner and trigger>
```

`## Open questions` uses the five dispositions from the ambiguity gate in `PRODUCT.md` §5.1 — `resolved`, `rejected`, `delegated`, `deferred`, `blocking`. They are inherited by the Tickets cut from this Story, where `blocking` genuinely refuses `ready`. A `blocking` question here means the approach is not settled: say so rather than handing planning a design it cannot cut.

## What makes a seam real

A seam is a place the change enters the system. It is real when you can name the file or module, say what responsibility it gains, and point at the pattern it should follow.

Three tests:

- **Locatable.** You found it, and you can cite the path. A seam nobody located is a guess wearing a file path, and the worker discovers that after the lease is held.
- **Bounded.** You can say what it must *not* do. A seam with no stated boundary grows during implementation, and the growth is invisible until review.
- **Cuttable.** Planning can draw a Ticket around it that is demonstrable on its own. If every seam has to land at once, the slice is one Ticket, and saying so here is cheaper than discovering it in the breakdown.

For each seam settle reuse-or-build explicitly. Duplication proposed here becomes duplication a reviewer has to argue about with a worker who was only following the approach.

## Forced order versus preference

Name only the order the system forces: one piece cannot compile, run or be verified before another. That is what planning turns into `blocked_by`.

Everything else is preference. A shared file is not a dependency. A tidier reading order is not a dependency. Calling a preference a dependency serializes work that could run in parallel, and the cost lands on every later session, not this one.

When you genuinely do not know whether an order is forced, say which one you suspect and what would settle it. An honest uncertainty is plannable; a confident wrong edge is not.

## Invariants

An invariant is what must still be true afterwards and has nothing to do with the change: the public response envelope, the error taxonomy, the rotation nobody should touch, the migration that must stay reversible.

These become the Ticket invariants a worker is held to and a reviewer checks against. Write the ones this change could plausibly break — an invariant nothing in this slice comes near is noise, and noise in this section trains people to skim it.

## Recording an alternative

The rejected shape is the part a later reader needs most, and it is the first thing lost if it is not written in the turn it was rejected.

Record three things: what the alternative was, why it was defensible, and what made it lose. An alternative recorded as "we didn't do X" teaches nothing and gets re-proposed at the next review.

If the rejection was **hard to reverse**, **hard to understand without context**, and carried a **real trade-off**, it has earned a Decision node. Note that in the draft — the question, the answer, the alternative, and which conditions it meets — and hand it to `pulse-planning`, which owns every node. Spec never creates one.

## Aim, and what not to write

Aim at the behavior, from the perspective of whoever experiences it. Give a worker enough to orient and judge an unforeseen case: the files, the seam, the existing pattern, the invariant.

Do not write a line-by-line plan. Layer-by-layer instructions date faster than the code they describe, and a worker who follows them past the point they stopped being true produces exactly the change the approach asked for and the wrong change for the repository.

Avoid pasting code. The exception is a snippet that encodes a decision more precisely than prose can — a state machine, a schema, a type shape — trimmed to the decision-rich part and marked as coming from a prototype or a Decision.
