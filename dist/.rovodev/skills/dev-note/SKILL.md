---
name: dev-note
description: >-
  Use only when the user explicitly asks to save, note, record, or preserve one
  learning from the current conversation as a raw developer note.
metadata:
  version: '1.0'
  ecosystem: pulse
  type: support
  dependencies: []
---

# dev-note

Capture one reusable developer learning from the current session into a structured raw note.

Invoke this skill only on an explicit user request to note, capture, or save something from the current conversation. Never auto-capture.

This skill exists to help the developer keep learning while coding with AI instead of letting useful insight disappear into chat history.

## HARD-GATE: template-first, no exceptions

Before writing or updating any markdown artifact for this workflow:

1. Read `references/daily-note-template.md`.
2. Read `references/note-entry-template.md`.
3. If the intended artifact does not match those templates, stop.

Do not write freeform markdown because the user said "quickly".
Do not preserve a messy raw note file shape just to avoid churn.
Do not invent extra markdown artifacts.

## Output contract

This skill writes or updates exactly one markdown artifact in v1:

- `dev-notes/raws/YYYYMMDD.md`

The daily file must follow `references/daily-note-template.md`.
Every appended learning entry must follow `references/note-entry-template.md`.

## What counts as a good dev note

A good raw dev note captures:
- one concrete learning
- why it matters for future coding, debugging, or AI collaboration
- enough session evidence to trust the note later
- one quick reuse hint

A raw dev note is **not**:
- a transcript dump
- a task log
- a TODO list
- a broad summary of everything that happened in the last ten minutes

Prefer signal over completeness.

## Workflow

### 1. Identify the target learning

Use the current conversation context plus the user's note request.

If one clear learning is implied, proceed.
If multiple plausible learnings are present, ask exactly one short clarification question.

Good clarification pattern:
- "Bạn muốn note insight nào: stale closure, debugging heuristic, hay rule về không over-trust AI summary?"

Do not guess when ambiguity is real.
Do not combine multiple unrelated learnings into one note just to avoid asking.

### 2. Normalize the daily file if needed

If `dev-notes/raws/YYYYMMDD.md` does not exist, create it from `references/daily-note-template.md`.

If it exists but does not match the template closely enough to append safely, normalize it to the template first, then add the new entry.

Do not append ad hoc sections to a messy file.
The daily file is a managed artifact, not an informal scratchpad.

### 3. Write one entry using the hard entry block

Append exactly one new entry using `references/note-entry-template.md`.

Rules:
- one note = one learning
- keep the summary to one sentence
- keep topic hints practical and sparse
- start `Distilled into` as `[]`
- capture only evidence that materially supports the learning

### 4. Keep the note reusable

When shaping the content:
- prefer a reusable lesson over a play-by-play transcript
- keep dead-end chatter out unless it changes the future heuristic
- name the insight plainly
- keep the reuse hint actionable

### 5. Return a short confirmation

After the write/update, respond briefly with:
- where the note was saved
- the summary of the learning
- whether you had to normalize the daily file first

Do not create a markdown report.
Just confirm in chat.

## Red flags

Stop and correct the approach if any of these appear:
- writing freeform markdown because there is no template in front of you
- appending a quick section to a messy daily file without normalization
- turning the note into a transcript summary
- combining several unrelated learnings into one note without user confirmation
- creating another `.md` file outside the raw daily note contract

## Handoff

```text
Raw learning captured. If the user later wants to organize or consolidate these notes into durable topic knowledge, load `pulse:dev-note-distil`.
```
