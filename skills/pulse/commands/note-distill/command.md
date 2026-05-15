# `/pulse note-distill`

Maps from legacy `dev-note-distil`.

## Intent

Use this command to turn several raw notes into synthesized guidance that another session can actually reuse.

## Inputs expected

Bring:

- the relevant raw notes
- the topic or question that links them together
- any desired audience for the distilled output
- evidence that should survive the distillation process

Useful shared references:

- `../../references/shared/handoff-and-resume.md`
- `../../references/HARNESS.md`

## Primary outputs/artifacts

Typical outputs are:

- distilled guidance
- consolidated takeaways
- candidate durable memory or reusable operator advice

## Interaction model

`note-distill` is synthesis work.
It should merge and clarify notes, not merely concatenate them.

## Approval expectations

Ask before publishing the distilled result as shared durable guidance or changing canonical references.

## Next command recommendations

- `compound` when the distilled output should feed broader workflow learning
- `plan` or `review` when the synthesis immediately informs active work

## Failure / escalation behavior

- if the source notes are too thin or contradictory, keep them raw and say why
- if the real need is just quick capture, route back to `note`
