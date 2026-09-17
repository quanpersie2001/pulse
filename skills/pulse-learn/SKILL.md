---
name: pulse-learn
description: Distill a just-done Ticket's frictions into institutional memory — at most one learning candidate via `pulse learn add`, plus at most one intervention chosen on the evidence ladder check > template > doc > AGENTS. Use it right after `pulse close` succeeded (or a Story closed) and the Ticket recorded `--friction` notes. Do not use it mid-Ticket (record friction there, processing comes after done), for a Ticket with zero frictions worth generalizing (write nothing — a learning needs a recurrence to prevent), or to edit AGENTS.md/PULSE.md as the intervention (that is owner work, never done inside a Ticket).
---

# Pulse Learn

Close the loop: a friction that cost this Ticket should never cost the next
one at full price. Output is deliberately capped — one learning, one
intervention — because a loop that files everything teaches the repo to
ignore its own memory.

## 0. Preconditions

The Ticket is `done` (or its Story just closed). Gather what it recorded:

```text
pulse work show <ticket-id> --json
pulse events tail --id <ticket-id> --json
```

Frictions live in the Ticket's notes (`--friction`) and the event trace.
Also skim the run logs the Ticket's evidence dir keeps
(`.pulse/evidence/<ticket-id>/run-*.log`) — a crash the lane classified is
a friction candidate even when nobody notated it. No frictions, nothing
generalizable: stop here and say so. Forcing a learning from noise is how
`candidate` piles grow unread.

## 1. Distill at most one learning

A learning is a *generalized* failure, constraint, technique or routing —
not the Ticket's diary. Keep one when the friction would recur wherever
`applies_to` matches; drop Ticket-specific specifics (this endpoint, this
component) into the docs instead.

```text
pulse learn add --title "<the generalized failure>" --kind failure|constraint|technique|routing --applies-to "<glob>" --expected-signal "<the concrete thing a future handoff must show>" --actor human:quan
```

Or write the full file (frontmatter `id/status/kind/applies_to/tags`,
body `## Summary / ## Do / ## Avoid / ## Check`) and
`pulse learn add --from <file>`. The `Check` section is the part that
earns activation: one command or observation proving the learning was
applied. `applies_to` globs come from the anchors this Ticket actually
touched — a learning aimed everywhere applies nowhere.

The new learning is a `candidate`: it enters packets only after a handoff
records it `helpful` and a human runs `pulse learn activate`. Say so in the
report — a candidate nobody knows about is a candidate nobody will use.

## 2. Choose at most one intervention — check > template > doc > AGENTS

Pick the lowest rung that would have *mechanically* caught or prevented the
friction, and stop there:

1. **check** — a gate that fails loudly: a `check-*` role in the target's
   `.pulse/runners.json`, a `qa_cases[].check` argv, a `scripts/qa/cases/`
   validator. "Make It a Check" beats prose whenever the failure is
   machine-detectable — a shape violation, a missing migration, a schema
   drift. This is the rung the loop exists to reach.
2. **template** — fix the seed the target copied (`scripts/qa/*.mjs`,
   `docs/operations/run.md`, `PULSE.md` profile blocks), so the next
   `pulse init` inherits the fix. Sync with the matching template in the
   Pulse repo when the bug lives there.
3. **doc** — one line in `docs/operations/` or `docs/product/`, with
   frontmatter `applies_to`/`tags` so `pulse docs applicable` can surface
   it. For when the fix is knowledge a human must apply with judgment.
4. **AGENTS** — the owner edits the AGENTS block, outside any Ticket.
   Never as an in-Ticket action; never when a check would do — a block
   that grows one rule per friction becomes the noise wall everyone skips.

One intervention, not one per friction: several frictions of the same kind
collapse into the check that catches all of them. Leftover frictions stay
recorded — the loop runs again after the next Ticket.

Prove the intervention when it is a check: run it against the state that
caused the original friction and watch it fail, then against the fixed
state and watch it pass. An unexercised check is a guess wearing a
uniform.

## 3. Keep the loop honest

```text
pulse learn show
pulse learn applicable <ticket-or-story-id>
pulse learn activate <LRN-id> --actor human:quan
pulse learn retire <LRN-id> --reason "<why>" --actor human:quan
```

`learn show` with no id lists everything. `applicable` is the gate a
learning must pass to matter: after adding, run it against the next
Ticket's id and confirm the match fires through `applies_to` or `tags`.
Activation needs a handoff to have recorded `helpful` first — the two
halves of trust. Retire with a reason when a learning misleads; the file
stays.

## Report

```markdown
## Learned
- <LRN-id> (candidate, <kind>) — <one line> — applies_to: <glob>

## Intervention
- <rung> — <what changed, where> — proof: <check failed before / passed after | doc link>

## Skipped
- <friction> — <why it stays Ticket-specific>

## Next
- candidate activates after one helpful handoff; run `pulse learn applicable` on the next <anchor> Ticket
```
