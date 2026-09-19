# The learning record

What each flag of `pulse learn add` does, and the file form for a learning
too long to pass as flags.

## Flags

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

## The full-file form

Write the file instead when the lesson needs prose: frontmatter
`id/status/kind/applies_to/tags`, body `## Summary / ## Do / ## Avoid /
## Check`, then `pulse learn add --from <file> --friction …`.

The `Check` section is the part that earns activation: one command or
observation proving the learning was applied. A learning with no checkable
`Check` can still be activated, but it will never be enforced — it stays
prose a worker may skim.
