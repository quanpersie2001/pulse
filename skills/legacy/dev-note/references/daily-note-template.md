# Daily Raw Dev Note Template

Use this template when creating or normalizing `dev-notes/raws/YYYYMMDD.md`.

**Save to:** `dev-notes/raws/YYYYMMDD.md`

Rules for filling this in:
- Keep the YAML frontmatter on line 1.
- This file is the daily container for raw dev-note entries.
- Every entry inside this file must use the hard entry block from `note-entry-template.md`.
- Do not leave placeholder text behind.
- Do not add ad hoc sections outside the template.
- If the file already exists in a different shape, normalize it to this structure before adding new entries.

---

## Template

```markdown
---
date: YYYY-MM-DD
type: dev-note-raw-daily
status: active
entry_count: <integer>
last_updated: <ISO-8601>
---

# Dev Notes Raw — YYYY-MM-DD

This file stores raw developer learnings captured during coding, debugging, and brainstorming with AI.

## Entries

<!-- Append each new entry below using the exact block from references/note-entry-template.md -->
```
