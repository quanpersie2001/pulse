# `/pulse note`

Maps from legacy `dev-note`.

## Intent

Use this command to capture tactical notes, decisions, breadcrumbs, and partial insights without forcing a full synthesis step.

## Inputs expected

Bring:

- the observation or decision worth saving
- the context where it was discovered
- any supporting file paths, commands, or evidence links

Useful shared references:

- `../../references/shared/handoff-and-resume.md`
- `../../references/HARNESS.md`

## Primary outputs/artifacts

Typical outputs are:

- a raw note entry
- a breadcrumb for later resume or synthesis
- a short decision record when the insight is tactical rather than durable policy

## Interaction model

`note` is lightweight capture.
It should be cheap to use and should not force a larger planning or compounding ceremony.

## Approval expectations

No gate is required for private or session-scoped capture.
Ask before writing shared durable guidance or changing canonical docs.

## Next command recommendations

- `note-distill` when enough raw notes have accumulated
- the active workflow command when capture was only a side move

## Failure / escalation behavior

- if the note is actually a workflow state handoff, route it through the handoff mechanism instead
- if the content is already a polished reusable lesson, consider `compound` instead of keeping it raw
