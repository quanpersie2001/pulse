---
name: pulse-learn
description: Use when a Ticket has just reached `done`, a Story has just closed, or work was deliberately cancelled, and it recorded `--friction` notes. Not mid-Ticket, not when nothing was recorded, and not for editing AGENTS.md or PULSE.md.
---

# Pulse Learn

Close the loop: a friction that cost this Ticket should never cost the next
one at full price — and every friction ends the loop either *classified
into memory* or *dismissed with a reason*. Output is deliberately capped —
one learning, one intervention — because a loop that files everything
teaches the repo to ignore its own memory. A friction left unclassified
blocks `close-story` for the whole Story: the skill's last step is the one
that keeps the gate honest.

## 0. Preconditions

The Ticket is `done`, its Story just closed, or the work was **cancelled**.
Cancelled work is not exempt: its frictions are knowledge Pulse already
counts — `close-story` refuses while any of them stays unclassified, so a
Ticket abandoned with lessons still ends here.

List what it recorded:

```text
pulse learn friction <ticket-id> --json
pulse work show <ticket-id> --json
pulse events tail --id <ticket-id> --json
```

Each friction line carries its stable key — the `evt_…` id of the
`note.recorded` event — its state (`unclassified`, `learned:<LRN-id>`,
`dismissed`), and its text. The event log is the source of truth: notes cut
from the record after 50 still list here. Also skim the run logs the
Ticket's evidence dir keeps (`.pulse/evidence/<ticket-id>/run-*.log`) — a
crash the lane classified is a friction candidate even when nobody notated
it. No frictions, nothing generalizable: stop here and say so. Forcing a
learning from noise is how `candidate` piles grow unread.

Read what is there; never reconstruct what should have been there. When the
evidence is thin, the honest output is a dismissal naming the gap, not a
learning assembled from what probably happened.

## 1. Distill at most one learning

A learning is a *generalized* failure, constraint, technique or routing —
not the Ticket's diary. Keep one when the friction would recur wherever
`applies_to` matches; drop Ticket-specific specifics (this endpoint, this
component) into the docs instead.

```text
pulse learn add --title "<the generalized failure>" --kind failure|constraint|technique|routing --applies-to "<glob>" --expected-signal "<the concrete thing a future handoff must show>" --friction <ticket-id>#<evt-id> --cite <path>:<from>-<to> --actor human:quan
```

Every flag on that command earns its place —
[references/learning-record.md](references/learning-record.md) has what
each one does, and the full-file form for a learning too long to pass as
flags. Two that decide whether the learning is worth anything:
`--applies-to` globs come from the anchors this Ticket actually touched (a
learning aimed everywhere applies nowhere), and `--check-argv` turns the
lesson into a command `pulse verify` runs once a human activates it — the
highest rung of the ladder in §2, made executable.

The new learning is a `candidate`: it enters packets only after a handoff
records it `helpful` and a human runs `pulse learn activate`. Say so in the
report — a candidate nobody knows about is a candidate nobody will use.

## 2. Choose at most one intervention — check > template > doc > AGENTS

Pick the lowest rung that would have *mechanically* caught or prevented the
friction, and stop there:

1. **check** — a gate that fails loudly: `--check-argv` on the learning
   (enforced by `pulse verify` once activated — prefer this when Pulse
   itself can run the proof), a `check-*` lane in the target's `PULSE.md`
   profile, a `qa_cases[].check` argv, a `scripts/qa/cases/` validator.
   "Make It a Check" beats prose whenever the failure is machine-detectable
   — a shape violation, a missing migration, a schema drift. This is the
   rung the loop exists to reach.
2. **template** — fix the seed the target copied (`scripts/qa/*.mjs`,
   `docs/operations/run.md`, `PULSE.md` profile blocks), so the next
   `pulse init` inherits the fix. Sync with the matching template in the
   Pulse repo when the bug lives there.
3. **doc** — one line in `docs/operations/` or `docs/product/`, with
   frontmatter `applies_to` set to the code the doc DESCRIBES (plan 0025
   F3: Pulse warns when that code changes without the doc) and `tags` for
   the anchors-based hint. For when the fix is knowledge a human must apply
   with judgment.
4. **AGENTS** — the owner edits the AGENTS block, outside any Ticket.
   Never as an in-Ticket action; never when a check would do — a block
   that grows one rule per friction becomes the noise wall everyone skips.

One intervention, not one per friction: several frictions of the same kind
collapse into the check that catches all of them. Leftover frictions stay
recorded — step 3 dismisses them, the loop runs again after the next
Ticket.

Prove the intervention when it is a check: run it against the state that
caused the original friction and watch it fail, then against the fixed
state and watch it pass. An unexercised check is a guess wearing a uniform.

## 3. Dismiss what stays Ticket-specific — never skip this

Every friction still `unclassified` after steps 1–2 ends with an explicit
dismissal. This is the skill's "a leftover friction is that Ticket's own
business" made mechanical:

```text
pulse learn dismiss <ticket-id> --all --reason "<ticket-specific: the rename was local to this story>" --actor human:quan
```

- A dismissal is a **classification, not an erasure** — the event log keeps
  the friction, the reason says why it never generalizes, and any actor may
  record one (the worker that hit it is usually the best judge).
- The reason is mandatory and must be a *why*, not a shrug:
  `"ticket-specific: <what made this one different>"`.
- `--all` settles every unclassified friction of the Ticket at once;
  individual `evt_…` keys work when you want precision. Frictions already
  classified (cited by a learning, or dismissed before) are skipped.
- **Skipping this step blocks `close-story`** with
  `close_story_friction_unclassified` — the Story's milestone cannot close
  while any of its tickets' frictions are raw. If `pulse learn friction
  <id>` shows nothing unclassified when you finish, the loop is closed.

## 4. Keep the loop honest

```text
pulse learn applicable <next-ticket-id> --all
```

`applicable` is the gate a learning must pass to matter: after adding, run
it against the next Ticket and confirm the match fires through `applies_to`
or `tags`. A learning nothing matches was written for nowhere.

The rest of the loop's upkeep — activation and the two halves of trust,
`suspect` learnings, retiring a misleading one, and reading `pulse metrics`
as the loop's scoreboard — is in
[references/loop-hygiene.md](references/loop-hygiene.md).

## Red flags

Each names a failure this loop has actually produced:

- a learning whose `applies_to` is a directory-wide glob — aimed everywhere,
  so it fires on every Ticket and gets ignored on all of them;
- a learning that restates the Ticket ("the tags endpoint needed a
  migration") instead of the generalization that outlives it;
- a `--check-argv` nobody ran against the broken state first;
- more than one learning or more than one intervention from one Ticket;
- a dismissal whose reason is a shrug (`"not important"`) rather than a why;
- finishing while `pulse learn friction <id>` still lists anything
  unclassified.

## Report

```markdown
## Learned
- <LRN-id> (candidate, <kind>) — <one line> — applies_to: <glob>
  frictions: <subject>#<evt-id>… — cite: <path>:<from>-<to>
  check: <argv | "none — activate enforces nothing">

## Intervention
- <rung> — <what changed, where> — proof: <check failed before / passed after | doc link>

## Dismissed
- <subject>#<evt-id> — ticket-specific: <why it never generalizes>

## Next
- candidate activates after one helpful handoff; run `pulse learn applicable` on the next <anchor> Ticket
```
