# `/pulse note`

Tactical capture command for preserving exactly one reusable learning from the current session as a managed raw note.

This is deliberate capture, not automatic logging, transcript archiving, or post-cycle synthesis.

## Mission

Write one evidence-grounded raw learning entry that is easy to trust later and easy to distill into durable topic knowledge.

## Entry criteria

Run `/pulse note` only when all are true:

- user explicitly asks to save, capture, note, or preserve a learning
- there is one concrete learning worth keeping now
- full synthesis would be premature or unnecessary

Do not run when:

- user wants a broad session summary rather than one reusable learning
- multiple independent learnings compete and no single target is chosen
- the goal is topic-level consolidation (`note-distill`)
- the goal is post-cycle memory/ratchet synthesis (`compound`)

## Required reads

Before writing anything, read:

1. `skills/pulse/commands/note/references/daily-note-template.md`
2. `skills/pulse/commands/note/references/note-entry-template.md`
3. `works/notes/raws/YYYYMMDD.md` for the target day, if it already exists

Template-first is mandatory. Do not improvise file structure because the capture looks small.

## Output contract

This command may create or update exactly one artifact:

- `works/notes/raws/YYYYMMDD.md`

Inside that artifact, it may append exactly one new raw-note entry.

The daily file must match `daily-note-template.md`.
The new entry must match `note-entry-template.md`.
No sidecar reports, summaries, or alternate markdown artifacts are allowed.

## Phase model (mandatory order)

### Phase 1 — Lock the single-learning target

Determine the one learning being captured.

If the target is ambiguous:

- ask for exactly one clarification
- do not guess
- do not merge unrelated learnings into one entry to avoid asking

One invocation equals one learning.

### Phase 2 — Normalize the daily raw-note container

Target path:

- `works/notes/raws/YYYYMMDD.md`

If the file does not exist:

- create it from `daily-note-template.md`

If it exists but does not match managed structure closely enough for safe append:

- normalize it first
- then append the new entry

The daily file is a managed artifact, not a scratchpad.

### Phase 3 — Append one managed raw-note entry

Append exactly one entry using `note-entry-template.md`.

Required fields must stay explicit:

- `Status: raw`
- `Created`
- `Summary`
- `Topic hints`
- `Distilled into: []`
- `What I learned`
- `Why it matters`
- `Evidence from this session`
- `Reuse hint`

Evidence must stay proportional to what actually happened in the session. Do not turn intuition into certainty.

### Phase 4 — Confirm write result

Return a short operator confirmation with:

- path written
- one-line learning summary
- whether the daily file required normalization first

Do not generate a markdown report about the note capture.

## Role boundaries

`/pulse note` owns:

- raw learning capture
- template conformance for the raw-note artifact
- minimal provenance needed for later synthesis

`/pulse note` does not own:

- topic synthesis (`note-distill`)
- global process/memory ratchets (`compound`)
- automatic note capture without user request

## Guardrails

- Never auto-capture without explicit user request.
- Never record multiple independent learnings in one entry.
- Never append unstructured prose to the daily file.
- Never create extra markdown artifacts outside the raw-note contract.
- Never inflate confidence beyond the supporting evidence.

## Red flags

Stop and correct the approach if any of these appear:

- turning the note into a transcript dump
- treating the note as a task log or TODO list
- appending to a messy daily file without normalization
- combining several unrelated learnings into one entry
- skipping template reads because the change looks small

## Exit contract

Successful exit requires:

- exactly one new raw-note entry appended
- daily artifact normalized when needed
- provenance fields sufficient for later `note-distill`
- concise chat confirmation only

## Next command guidance

- `note-distill` when enough raw notes have accumulated to justify topic synthesis
- otherwise return to the previously active workflow command
