---
id: <task-id>
---

# <Task Title>

## Objective

What this task must accomplish for the validated approved work.

## Parent Story

- `<story-id>` — <story title>

## Source Plan

- `plan.md` — <task row / approved work mapping>
- `solution-design.md` — <decision IDs this task implements>
- Decision refs: [D...]
- Learning refs: []

## Scope

### In scope

- <specific bounded work>

### Out of scope

- <explicit exclusions>

## Expected Touched Surfaces

### Code

- `<path>` — <why touched, or N/A>

### Explicit File Scope

List concrete files this task may touch. Empty is allowed when the task is docs-only or discovery-only; omission is not.

- `<path>`

### Docs

- `docs/ARCHITECTURE.md` — <Create | Update | No change> — <reason>
- `docs/GLOSSARY.md` — <Create | Update | No change> — <reason>
- `docs/decisions/` — <Create | Update | No change> — <reason>
- `docs/product/` — <Create | Update | No change> — <reason>

### Tests / Validation

- `<path or command>` — <expected evidence>

## Implementation Notes

- <notes inherited from `plan.md`; do not add new solution decisions>

## Dependencies

- Blocking dependencies: []
- Non-blocking links: []

## Testing Mode

**Mode:** standard | tdd-required

If `tdd-required`, record planned red/green steps:

- Red command: `<command>` — <expected failure signal>
- Green command: `<command>` — <expected pass signal>

## Verification

Runnable checks with expected outcomes:

- Command: `<command>`
  - Expect: <expected result>

## Verification Evidence

Explicit artifact paths or concrete records validation/execution must produce:

- `works/epics/<epic>/<story>/verification/<task-id>.md` — <evidence to capture>

## Caveats / Risks

- None.
