# The story draft contract

`works/_drafts/<slug>/story.md` is the first durable artifact of a delivery
chain. Three later skills read it: `pulse-spec` builds `approach.md` and `qa.md`
beside it, `pulse-planning` cuts the graph against it and then copies it to
`works/<story-id>/story.md`, and `pulse run` executes Tickets whose acceptance
traces back to it.

That is why the shape is fixed. A draft that invents its own sections forces
every reader to go looking, and the sections below are exactly the ones a
downstream skill needs to find without reading the whole file.

## Where the draft lives

```text
works/
  _drafts/<feature-slug>/
    story.md                 # grill writes
    approach.md              # spec writes
    qa.md                    # spec writes
    research/<topic>.md      # research writes while no Ticket owns the question
```

Tracked, not runtime. This prose survives a handoff, and a draft in an ignored
directory is work that quietly disappears between sessions.

`_drafts` cannot be mistaken for a node: the leading underscore is outside the
identifier pattern, and graph validation only constrains the `content_dir` of
nodes that exist. A draft directory therefore neither appears in `pulse work
list` nor fails `pulse graph validate`.

### Choosing the slug

- Lowercase, hyphenated, named for the behavior rather than the mechanism:
  `offline-release-rollback`, not `rollback-service-refactor`.
- Stable across sessions. `pulse-spec` addresses this directory later, often in
  a different session with a different agent, and a slug it cannot guess becomes
  a second draft of the same work.
- One behavior slice per slug. Two behaviors that can be proved independently
  are two drafts. Splitting prose after it is written is the expensive version
  of this decision.

## Template

```markdown
# <Readable behavior title>

Slug: <feature-slug>
Draft: no node exists yet; `pulse-planning` adopts this file into `works/<story-id>/`.

## Outcome

<One to three sentences naming the observable state that will exist when this
slice is delivered. Written so someone outside the conversation can tell
whether it has happened.>

## Success signals

- <Observable signal, checkable without reading the implementation.>
- <Another.>

## Scope boundary

In scope:

- <behavior included in this slice>

Out of scope:

- <excluded item> — <where it went: deferred with an owner, another slice, or ruled out and why>

## Vocabulary

- <term> — <what it denotes here> — <settled this session | already in the glossary>

## Decisions

- D1 — <the question> — <the answer> — <why, and what was rejected>
  - Decision node: <no, reason kept here | proposed, conditions it meets>

## Open questions

- (resolved) <question> — <choice and reason, or the Decision it points at>
- (rejected) <question> — <reason strong enough that it will not be reopened>
- (delegated) <question> — <whose choice it is and the freedom it sits inside>
- (deferred) <question> — <owner, plus the trigger or linked work that reopens it>
- (blocking) <question> — <what it would change and who must answer it>

## Product rules

- <BR-* or E-*> — <how this slice exercises it>
```

The last section applies when the slice sits under a product contract and the
work is heading for R2 or R3, where acceptance items cite the rule they
exercise. R0 and R1 do not inherit that requirement, and adding empty
traceability to a small change is ceremony.

## Outcome and success signals

The outcome says what will be true. The success signals say how anyone would
know. Keeping them separate matters because `pulse-spec` turns the signals into
QA cases and `pulse-planning` cuts Tickets so each one demonstrates a signal on
its own.

A signal is usable when it survives this test: could a person who has never seen
the diff check it?

| Unusable | Usable |
|---|---|
| The rollback flow is robust. | A release rolled back from the hub returns the previous version to every client on the next poll. |
| Error handling is improved. | A failed signature check returns a stable error code and never names the internal key path. |
| Publishing feels faster. | Publishing a 200 MB bundle reports progress and completes without a manual retry. |

The left column is not a wording problem. It is an unsettled decision wearing a
sentence, and it will be settled by whoever implements it — which is exactly
what this session exists to prevent.

## Dispositions

Every question that could change the objective, the acceptance, an invariant, a
public contract, or a hard-to-reverse direction needs one of five dispositions,
defined by the ambiguity gate in `PRODUCT.md` §5.1:

| Disposition | Means | Needs |
|---|---|---|
| `resolved` | chosen | the choice and its reason, or a Decision to point at |
| `rejected` | considered and ruled out | a reason strong enough not to reopen it |
| `delegated` | the implementer chooses | it genuinely sits inside implementation freedom |
| `deferred` | not needed in this scope | an owner and a trigger, or linked follow-up work |
| `blocking` | cannot be dispatched | names what it would change and who must answer |

These dispositions are inherited. `pulse-planning` copies this file onto the
Story and carries the questions into the Tickets cut from it, where the ready
gate genuinely refuses a `blocking` question and genuinely fails an
undispositioned one.

At Story altitude there is no such enforcement — the `shaped` profile evaluates
only `ticket_ambiguity`, which is not applicable to a node with no
implementation role. So a sloppy disposition here is not caught here; it is
caught in `pulse-planning` after the cut was already made against it, or not at
all. Write them as if the gate were running, because downstream it is.

Two failure modes are worth naming:

- **`delegated` as an escape hatch.** Delegating a question that changes
  acceptance hands a product decision to whoever picks up the Ticket. Delegate
  naming and internal structure; never behavior a signal depends on.
- **`deferred` with no owner.** A deferral without an owner and a trigger is an
  open question that has been made invisible. It reappears mid-implementation,
  when it is most expensive.

## Reading the draft back

Before reporting, read the file as someone who was not in the conversation.
They should be able to state what will be true when this is done, and what is
deliberately not being done. Anything they could not — a placeholder, a
preference with no reason, a term used in two senses, a signal nobody can check
— is the next question rather than an editing pass.
