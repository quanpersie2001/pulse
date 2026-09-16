---
name: pulse-planning
description: Turn settled understanding and a settled approach into the work graph, then hand the graph a prose contract. This is the only skill allowed to create Epics, Stories, Tickets, decision_work and dependency edges, so use it whenever a draft under works/_drafts/ has a story, an approach and a QA baseline and needs to become real work items, and whenever a human-confirmed decision frontier needs decision_work Tickets. Do not use it for a bounded R0 change that already fits one session and needs one obvious Ticket, for resolving product ambiguity (pulse-wayfind, pulse-grill), for choosing a solution or test strategy (pulse-spec), for gathering external facts (pulse-research), or for implementing, reviewing or closing work.
---

# Pulse Planning

Decide what work should exist, get the grain approved, create it, then give it the prose contract it will be executed against. Planning owns the cut: which nodes exist, how coarse each one is, and which dependencies are real.

Planning is the **last** step before execution, and it is entered **once** per delivery chain:

```text
pulse-wayfind -> pulse-grill -> pulse-spec -> pulse-planning -> pulse run
```

By the time planning runs, meaning is settled (`story.md`) and the approach is settled (`approach.md`, `qa.md`). That is what makes one pass enough: the cut can be judged against a known solution instead of guessed ahead of one. Earlier skills write that prose into `works/_drafts/<slug>/`, which exists precisely so no node has to be created before it is understood.

Every other skill transitions only the state it gates. Node creation and dependency wiring live here so the discipline cannot drift between eight guidance files.

Read these references when their subject becomes active:

- [`references/graph-breakdown.md`](references/graph-breakdown.md) for the proposal shape, reuse and duplicate checks, and the create-then-wire order.
- [`references/tracer-bullets.md`](references/tracer-bullets.md) for vertical slicing, context sizing, wide refactors, and `BR-*`/`E-*`/`QA-*` traceability.

Use repository file-editing tools only for authored Markdown and temporary command-input files. Send every graph and registry mutation through a literal `pulse` command.

## Establish Authority

1. Read `AGENTS.md` and `PULSE.md`. The Pulse-managed block in `AGENTS.md` owns the common workflow and R0 route.
2. Find the input. Planning accepts exactly two, and they are not two disciplines:

   - **A delivery draft** — `works/_drafts/<slug>/` holding `story.md`, `approach.md` and `qa.md`. This is the normal case and the rest of this skill is about it.
   - **A human-confirmed decision frontier** from `pulse-wayfind` — precise questions that need `decision_work` Tickets. This is transcription, not slicing: the grain was already confirmed with the human, so skip to step 3, create the Tickets and their `blocked_by` edges, and stop.

   If a delivery draft is missing a file, name the skill that produces it — `pulse-grill` for `story.md`, `pulse-spec` for `approach.md` and `qa.md` — and stop. Planning does not invent its own input, and slicing against a missing approach is how horizontal tickets get created.

3. State whether this needs planning at all. A bounded change whose route is already clear does not: it takes one Ticket through the ordinary flow in the Pulse-managed `AGENTS.md` block, and routing it here only to hold a breakdown review is ceremony. Planning earns its place when the cut itself is a real decision — several nodes, or a dependency order that could be got wrong.
4. Read the existing graph before proposing anything new:

   ```text
   pulse work list --kind epic --json
   pulse work list --kind story --json
   pulse work list --role decision_work --json
   pulse work show <id> --json
   ```

5. Treat accepted Decisions and approved product docs as intent, code and tests as implementation, and receipts as observations. If sources disagree in a way that changes acceptance, an invariant, or a public contract, stop, present the conflict, and let the human decide.
6. Refuse to plan around unresolved product ambiguity — but refuse narrowly. An uncertainty that could change what "done" means is not a planning input; return it to `pulse-wayfind` at product level or `pulse-grill` at Story level, and record it as a precise `decision_work` question if it is already precise. Planning may record such a question; it may never answer one.

   Scope the refusal to the acceptance the question would actually change. Ask, for each item in the cut, whether either answer to the open question would alter what proves it done. Where the answer is no, the item is plannable now and blocking it costs the human a week of idle work; where the answer is yes, leave that acceptance uncut and name the question that gates it. Blocking a whole Story because one of its behaviors is undecided is the common failure here, and it is as wrong as inventing an answer.

## 1. Draft the breakdown before touching the graph

Work out the whole cut on paper first. A breakdown is cheap to redraw and expensive to unpick once nodes exist.

Read the draft — `story.md` for the behavior and its open questions, `approach.md` for the solution and its seams, `qa.md` for the case IDs — plus the product rules and exceptions the Story cites and the code the approach names.

Check `qa.md` against the baseline contract while you read it, not later. The baseline is parsed, not prose: `## Posture` takes one of `automated`, `hybrid`, `manual_structured`, `static_proof`, `not_applicable` — `required` is a Ticket-level QA *impact* posture and is not valid here — and each case is a `### QA-NNN <title>` section carrying at least Intent, Surface, Priority, Steps and Expected. A baseline that is a list of one-line case titles will be refused at step 4, which changes the plan rather than just the paperwork: say so in the proposal and send it back to `pulse-spec` instead of planning around prose the gate cannot read.

Then decide which of three things each piece of work is:

- **Epic** — a settled slice of the product with an outcome and an investment boundary. Propose one only when the boundary is already clear, and only when something will hang beneath it. An Epic is never an investigation.
- **Story** — the behavior slice this draft is about, owner of its QA baseline. Usually exactly one per draft. If the draft turns out to hold two independently provable behaviors, say so and ask the human how to split the prose before creating either.
- **Ticket** — a vertical tracer bullet cut by the rules in [`references/tracer-bullets.md`](references/tracer-bullets.md): a narrow complete path, demonstrable on its own, sized for one fresh context window, with prefactoring first and a wide refactor sequenced expand–contract instead of forced into a slice.

Fog that cannot yet be phrased as one precise question stays in prose — the product contract's Open Questions, or `story.md`. Creating a node for fog turns an unstable question into a graph object that outlives it.

Name the dependencies that genuinely gate each item, and nothing more. A shared file is not a dependency; an order preference is `preferred_after`, not `blocked_by`.

## 2. Put the breakdown to the human

Present the proposal before any mutation. The human is reviewing the grain and the edges — the two things hardest to fix later and impossible to derive from the artifacts alone.

Use the proposal shape in [`references/graph-breakdown.md`](references/graph-breakdown.md): readable title, kind and role, parent, what it delivers, blocked by, and risk with the materialization it implies.

Ask explicitly whether the granularity is right, whether each dependency truly gates its dependent, and whether anything should be merged or split. Iterate until the human approves. Do not create a node, an edge, or a content file before that approval.

## 3. Create nodes, then wire edges

Create in dependency order — blockers and parents first — so every edge can reference a real identifier:

```text
pulse work create --kind epic --title <title> --json
pulse work create --kind story --title <title> --parent <epic-id> --json
pulse work create --kind ticket --role implementation --risk medium --parent <story-id> --title <title> --json
pulse work create --kind ticket --role decision_work --risk low --title <title> --json
```

`--parent` is allowed at creation because a parent must already exist. Dependencies are different: wire them in a second pass, once every node has an ID.

```text
pulse graph edge add --type blocked_by --from <dependent-id> --to <blocker-id> --actor <actor> --json
pulse graph validate --json
```

Never infer a dependency from the order items appeared in the proposal list. Wire only the edges the human approved, and confirm the graph still validates.

Creating a Ticket scaffolds `works/<id>/ticket.md` from a template — an implementation contract, or a question-and-output shape for `decision_work`. Epics and Stories get a content directory but no template.

## 4. Adopt the draft prose

The draft has to move from its slug address to the node addresses, because that is where every later command looks for it. Do it in this order, and do not reorder it:

1. **Copy** `story.md`, `approach.md` and `qa.md` into `works/<story-id>/`, and any `research/<topic>.md` under the Ticket that owns the question.

   Copy `qa.md` only if it passed the baseline check in step 1. Adopting a baseline the gate will refuse leaves the Story owning prose that cannot resolve.
2. **Write each `ticket.md`**, replacing the scaffolded template in `works/<ticket-id>/`, from the slice you proposed.
3. **Bind each Ticket** to its prose:

   ```text
   pulse work sync <id> --expected-revision <revision> --actor <actor> --json
   ```

4. **Validate the QA baseline**, which only resolves once the Story exists and `qa.md` sits at its node path:

   ```text
   pulse qa baseline <story-id> --json
   pulse qa resolve <ticket-id> --json
   ```

5. **Only then delete** `works/_drafts/<slug>/`.

The draft stays the source of truth until the graph has bound its content, so an interruption anywhere above leaves work that can simply be re-run. Deleting early is the one step that makes a crash lossy.

For an Epic, keep `brief.md` an index: outcome, investment boundary, success signals, and pointers to the product contract and the `BR-*`/`E-*` it covers. Copying rules into it creates a second place to change them.

A Story is created `draft` and stays there. Planning does not take a Story to `shaped`; `pulse-grill` owns that gate, and by now it has already passed through it in prose.

## 5. Take ready Tickets to ready

Fill each contract at the level the risk justifies and no further. The materialization table in `PRODUCT.md` §5.1 says what each level requires; creating artifacts to satisfy ceremony is the failure this gate exists to prevent. Two things the gate will ask for:

- **Acceptance with IDs.** Every acceptance item needs an `AC-*` identifier, because verification binds proofs to those IDs. At R2/R3, each `AC-*` cites the `BR-*` or `E-*` it exercises; R0 and R1 do not inherit that requirement.
- **Open questions with a disposition.** An undispositioned question fails the shaping gate, and a `blocking` one refuses `ready`. Dispose of what you can and leave `blocking` only where it is true.

Read the gate report rather than guessing what it wants:

```text
pulse work ready <id> --json
```

Then move the Ticket through the two preparation gates, using the current revision each time:

```text
pulse work transition <id> --to shaped --expected-revision <revision> --actor <actor> --json
pulse work transition <id> --to ready --expected-revision <revision> --actor <actor> --json
```

A Ticket whose blockers are still open, whose questions are still `blocking`, or whose QA or docs posture is unknown stays where it is. Report it as blocked with the reason codes the gate returned; do not restate the gate's rules as prose and do not work around them.

Leave every other transition alone. Planning gates the Ticket contract, so it moves Tickets to `ready` and nothing else.

## Report

Report exactly these sections:

```markdown
## Input
<draft slug and its files | confirmed decision frontier>

## Created
- <ID> — <kind/role> — <readable title> — <status>

## Reused
- <ID> — <readable title> — <why it covered the need>

## Edges
- <dependent-id> blocked_by <blocker-id> — <what it gates>

## Adopted
- <source path> -> <works/<id>/ path>
- Draft removed: <yes | no, and what still blocks removal>

## Blocked
- <ID> — <gate reason codes> — <what would unblock it>

## Left in prose
- <fog or deferred question> — <where it lives and who owns it>

## Next
<one next skill, command, or human decision>
```
