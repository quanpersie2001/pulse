# Distilled Topic Template

Use this template when creating or normalizing `dev-notes/distil/topics/<slug>/<slug>.md`.

**Save to:** `dev-notes/distil/topics/<slug>/<slug>.md`

Rules for filling this in:
- Keep YAML frontmatter on line 1.
- One topic file represents one stable concept, not one distillation run.
- Merge into an existing topic when the core idea overlaps strongly.
- Keep conclusions grounded in the raw notes actually cited.
- Normalize inconsistent topic files to this structure when updating them.
- Do not leave placeholders behind.

---

## Template

```markdown
---
name: <topic-slug>
description: <one-line description of the topic>
type: dev-note-topic
note_count: <integer>
updated_at: <ISO-8601>
source_notes:
  - dev-notes/raws/YYYYMMDD.md#note-<slug>
related_topics: [optional-related-topic-slug]
---

# <Human Topic Title>

## Core idea

<2-4 sentences describing the durable principle shared across the raw notes.>

## Heuristics

- <practical rule or heuristic>
- <practical rule or heuristic>

## Common failure shapes

- <failure shape or misunderstanding>
- <failure shape or misunderstanding>

## Examples from notes

- <short grounded example mapped from a raw note>
- <short grounded example mapped from a raw note>

## Related topics

- <topic-slug>
```
