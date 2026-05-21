---
name: dev-note-distil
description: >-
  Use when the user asks to distill accumulated dev-notes into reader-facing
  topic knowledge, rather than to capture a new note or update machine memory.
metadata:
  version: '1.0'
  ecosystem: pulse
  type: support
  dependencies: []
---

# dev-note-distil

Turn pending raw developer notes into durable topic knowledge and keep the global topic TOC aligned.

This is a reader-facing synthesis pass over accumulated dev-notes. It is not for runtime memory consolidation and not for post-cycle compounding.

This skill exists to prevent raw dev notes from becoming a pile of fragments that never turn into reusable understanding.

## HARD-GATE: template-first, no exceptions

Before creating or updating any markdown artifact for this workflow:

1. Read `references/topic-template.md`.
2. Read `references/topics-index-template.md`.
3. Read `references/topic-merge-rules.md`.
4. If the intended artifact does not match those templates, stop.

Do not patch topic files freeform.
Do not treat `TOPICS.md` as optional cleanup.
Do not create extra markdown reports in v1.

## Output contract

This skill may create or update only these markdown artifacts in v1:

- `dev-notes/distil/topics/<slug>/<slug>.md`
- `dev-notes/distil/TOPICS.md`
- `dev-notes/raws/YYYYMMDD.md` status fields for raw notes that were actually distilled

Topic files must follow `references/topic-template.md`.
The global index must follow `references/topics-index-template.md`.

## Required inputs

Read the pending raw notes first.
Look for raw-note entries that have not been distilled yet.

When relevant, also read the current distilled topic files and the current `TOPICS.md` so the update is based on existing knowledge, not on naming guesses.

## Workflow

### 1. Collect pending raw notes

Identify raw-note entries still awaiting distillation.
Use the raw note content as the evidence base.

Do not invent stronger conclusions than the cited notes support.
If a note is thin, distill it carefully and mark uncertainty through precise wording instead of polishing it into false certainty.

### 2. Decide merge vs create

Load `references/topic-merge-rules.md` before deciding.

Default behavior:
- merge into an existing topic when the core reusable lesson overlaps strongly
- create a new topic only when the lesson is genuinely distinct

Do not create a new topic only because the wording differs.
Do not force unrelated notes into one topic just to keep the list short.

### 3. Normalize topic files while updating them

If a topic file exists but does not match `references/topic-template.md`, normalize it while applying the update.

Do not preserve inconsistent structure just to minimize churn.
A managed topic artifact should stay reusable after every update.

### 4. Update the topic knowledge

For each affected topic file:
- update frontmatter fields honestly
- keep `source_notes` grounded in the raw entries actually used
- refine `Core idea`, `Heuristics`, `Common failure shapes`, and `Examples from notes`
- keep `Related topics` useful but lean

Each topic file represents a stable concept, not a run log.

### 5. Rebuild the global TOC

After changing the topic set or topic summaries, rebuild `dev-notes/distil/TOPICS.md` using `references/topics-index-template.md`.

This is required, not optional.
If topics change, the index changes too.

Do not leave `TOPICS.md` stale because the user did not mention it explicitly.
The index is part of the managed output contract of this skill.

### 6. Update raw note status

Mark the raw note entries as distilled and record which topic slugs they fed.

Use this minimal provenance convention for every distilled raw entry:
- set `**Status:** distilled`
- set `**Distilled into:** [topic-slug, ...]` with only the topic slugs actually updated/created from that note

Do not add extra status fields or narrative run logs in raw notes.
Keep this update factual.
Do not claim a note was distilled into a topic it did not actually influence.

### 7. Return a short confirmation

After the update, respond briefly with:
- how many raw notes were distilled
- which topics were created or updated
- whether the global `TOPICS.md` index was rebuilt

Do not create a markdown run summary.
Just confirm in chat.

## Red flags

Stop and correct the approach if any of these appear:
- treating `TOPICS.md` as optional or deferrable
- patching an inconsistent topic file without normalizing it to the template
- creating near-duplicate topics because wording differs
- writing a cleaner conclusion than the raw notes actually support
- creating any extra `.md` artifact without a matching hard template

## Handoff

```text
Distillation complete. If the user wants to capture more in-session learnings later, return to `pulse:dev-note`.
```
