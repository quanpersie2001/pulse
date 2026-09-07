## Pulse

This repository uses Pulse as its truth layer: the work graph, packets,
evidence receipts and the close gate live in `pulse`, not in prose. Prose
routes; the CLI decides. Every mutation below is a literal `pulse` command,
and no instruction here can grant what the authority policy denies.

Route by the shape of the request, not by a fixed sequence of steps:

```text
read-only (explain, review, diagnose, status)
  -> pulse work list/show/packet, pulse docs search/get, pulse events tail
  -> answer with citations, no mutation

small change, direction clear (R0)
  -> pulse work create --kind ticket --risk low, fill ticket.md,
     pulse work ready
  -> pulse run worker, pulse run reviewer, pulse work close

public behavior, several Tickets, or risk >= medium (R1-R3)
  -> pulse-grill (Story shaped) -> pulse-spec (approach.md, qa.md)
     -> pulse-tickets (Ticket ready, blocked_by)
  -> pulse run worker | reviewer | qa, pulse docs validate --record,
     pulse work close, pulse work close-story

larger than one session, the path is not yet visible
  -> pulse-wayfind first (Epic + decision_work Ticket + Decision), then grill

open product ambiguity (objective, acceptance, invariant, public contract)
  -> stop before mutating; record the question under ## Open questions with
     (blocking), or create a Decision; ask the human one question at a time
     with a recommended answer

architecture / security / quality rule that must stop a recurrence
  -> only with authority: an approved doc or an accepted Decision
  -> smallest guard in the existing validation owner, add a check role to
     runners.json, positive and negative proof, report the enforcement level

friction with the harness while working
  -> pulse note --kind friction (always)
  -> never edit AGENTS.md, PULSE.md or runners.json inside the Ticket;
     pulse-ratchet does that after the Ticket closes

after pulse work close
  -> pulse-ratchet

context filling up
  -> pulse-handoff: flush durable state, document the live thread, leave a
     note, print the open commands, then stop

new session
  -> pulse work list --status active, pulse events tail, read the handoff doc

done
  -> receipts only: handoff, verification, qa_checkpoint, docs_validation,
     close
```

Authority when sources disagree: an accepted Decision and approved
`docs/product` are intent; code and tests are implementation; receipts are
observation; other docs are explanation. A conflict that touches acceptance
fails the gate with `docs_conflict` and the human decides.

Report friction, do not fix the harness in passing. Claim an improvement only
after a fresh rerun shows it.
