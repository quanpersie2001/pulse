# CONTEXT.md Template

This template is written by `pulse:workflow explore` in Phase 4.
It is read by `pulse:workflow plan`, `pulse:workflow validate`, and downstream implementation/review phases.

**Save to:** `works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/CONTEXT.md`

Rules:
- Be concrete: `Card layout, not timeline` not `modern and clean`
- Every locked decision must have a stable ID (`D1`, `D2`, ...)
- Code context must cite actual file paths found during the quick scout
- Remove unused sections instead of leaving placeholders

---

## Template

```markdown
# <Story Name> — Context

**Story path:** works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>
**Date:** YYYY-MM-DD
**Exploring session:** complete
**Scope:** quick | standard | deep
**Project docs consumed:** [.pulse/project-docs.json -> <doc paths>] | [detected docs: <doc paths>] | none

---

## Feature Boundary

[One sentence: what this story delivers and where it ends.]

**Domain type(s):** SEE | CALL | RUN | READ | ORGANIZE

---

## Locked Decisions

These are fixed for downstream phases.

### Terminology & Domain Model
[Canonical terms plus rejected/conflicting meanings when terminology was a gray area.]

### <Category>
- **D1** [Specific, concrete decision]
  *Rationale: [Why this matters for implementation, optional]*

- **D2** [Specific, concrete decision]

### <Next category>
- **D3** [Specific decision]

### Agent's Discretion
[Areas explicitly delegated by the user and any constraints.]

---

## Specific Ideas & References

[User examples such as "like X" references, links, mockups, or neighboring features.]

---

## Scenario Checks

[Optional. Capture 2–4 concrete scenarios/edge cases that validate boundaries and terminology.]

- [Scenario] -> [What this confirms]

---

## Existing Code Context

From the quick scout during explore.

### Reusable Assets
- `path/to/file.ts` — [how it applies]

### Established Patterns
- [Pattern name] — [where used and implication]

### Integration Points
- [Path + contract to extend or call]

---

## Canonical References

Downstream agents should read these before planning or implementation.

- `path/to/spec.md` — [what it defines]
- `path/to/adr.md` — [decision it records]

*[Remove section if none]*

---

## Project Docs Follow-up

[Optional. Repo-level ambiguity discovered during explore.]

- **Target:** [doc path or artifact]
  *Why:* [ambiguity it resolves]

---

## Outstanding Questions

### Resolve Before Planning
[Product decisions that block planning. Remove section if none.]

- [ ] [Question] — [why it blocks]

### Deferred to Planning
[Technical questions that planning can investigate.]

- [ ] [Question] — [investigation needed]

---

## Deferred Ideas

[Out-of-scope ideas captured for future work items.]

- [Idea] — [why deferred]

---

## Gate 1 Handoff Note

This CONTEXT.md is the active source of truth for this story's locked decisions.

- `pulse:workflow plan` consumes locked decisions and open-question partitions
- `pulse:workflow validate` checks that planned checks and artifacts reflect decisions
- review/execution use decision IDs as invariant references

Decision IDs (`D1...Dn`) are stable and must be referenced unchanged downstream.
```
