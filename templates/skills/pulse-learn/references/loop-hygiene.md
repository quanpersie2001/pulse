# Keeping the learning loop honest

Upkeep between runs of `pulse-learn`. None of it belongs to one Ticket;
read it when a learning's state, or the loop's own numbers, is the
question.

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

