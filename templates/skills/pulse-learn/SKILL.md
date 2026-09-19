---
name: pulse-learn
description: Distill a just-done Ticket's frictions into institutional memory and leave nothing unclassified — at most one learning candidate via `pulse learn add`, at most one intervention chosen on the evidence ladder check > template > doc > AGENTS, and a `pulse learn dismiss` for every friction that stays Ticket-specific. Use it right after `pulse close` succeeded (or a Story closed) and the Ticket recorded `--friction` notes. Do not use it mid-Ticket (record friction there, processing comes after done), for a Ticket with zero frictions worth generalizing (dismiss nothing if nothing was recorded — write nothing), or to edit AGENTS.md/PULSE.md as the intervention (that is owner work, never done inside a Ticket).
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

The Ticket is `done` (or its Story just closed). List what it recorded:

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

## 1. Distill at most one learning

A learning is a *generalized* failure, constraint, technique or routing —
not the Ticket's diary. Keep one when the friction would recur wherever
`applies_to` matches; drop Ticket-specific specifics (this endpoint, this
component) into the docs instead.

```text
pulse learn add --title "<the generalized failure>" --kind failure|constraint|technique|routing --applies-to "<glob>" --expected-signal "<the concrete thing a future handoff must show>" --friction <ticket-id>#<evt-id> --cite <path>:<from>-<to> --actor human:quan
```

- `--friction <subject>#<evt-id>` cites the friction this learning
  classifies — that is what turns its state to `learned`. Repeat the flag
  when one lesson covers several recorded frictions.
- `--cite <path>:<from>-<to>` pins the code the lesson is about: Pulse
  hashes those exact lines itself, and `pulse doctor` reports the cite when
  the code moves (`stale` in the packet) — a signal to re-read, not an
  auto-retire.
- If the lesson is *checkable by a command*, pass
  `--check-argv '["cargo","test","--lib"]'` (and `--check-cwd <dir>` if
  needed). That is the highest rung of the evidence ladder, made executable:
  after a human runs `pulse learn activate <LRN-id>`, Pulse runs that check
  inside `pulse verify` for every matching Ticket — the packet flags it
  `"enforced": true`, a failing run blocks the handoff, and a worker that
  reads the learning fixes the cause. Only human activation arms it (a
  candidate never runs); say that in the report.

Or write the full file (frontmatter `id/status/kind/applies_to/tags`,
body `## Summary / ## Do / ## Avoid / ## Check`) and
`pulse learn add --from <file> --friction …`. The `Check` section is the
part that earns activation: one command or observation proving the learning
was applied. `applies_to` globs come from the anchors this Ticket actually
touched — a learning aimed everywhere applies nowhere.

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
pulse learn show
pulse learn applicable <ticket-or-story-id> --all
pulse learn activate <LRN-id> --actor human:quan
pulse learn retire <LRN-id> --reason "<why>" --actor human:quan
pulse metrics --json
```

`learn show` with no id lists everything. `applicable` is the gate a
learning must pass to matter: after adding, run it against the next
Ticket's id and confirm the match fires through `applies_to` or `tags`.
`--all` also shows `suspect` learnings — reported `misleading` more often
than `helpful`, already excluded from packets and from `pulse verify`,
waiting for a human to retire or re-trust them. Activation needs a handoff
to have recorded `helpful` first — the two halves of trust. Retire with a
reason when a learning misleads; the file stays. `pulse metrics` is the
loop's scoreboard: friction per done Ticket, unclassified remaining,
rework rate, verify runs, learning usage.

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
