# `/pulse brainstorm`

Maps from legacy `brainstorming`.

## Intent

Use this command when the user goal is real but the feature shape is still open.
`brainstorm` explores option space, compares trade-offs, and produces a direction that planning can turn into an execution shape.

## Inputs expected

Useful inputs include:

- the user goal or desired outcome
- known constraints and non-goals
- existing product or repo context that should constrain the options
- any design ambiguity that would benefit from structured comparison

Command-local references and assets:

- `references/spec-reviewer-prompt.md`
- `references/visual-support-guidance.md`
- `scripts/start-visual-server.sh`
- `scripts/stop-visual-server.sh`
- `scripts/visual-server.cjs`

## Primary outputs/artifacts

Typical outputs are:

- candidate approaches
- trade-off framing
- a chosen direction or approved design brief
- a clear recommendation for what planning should shape next

## Interaction model

`brainstorm` is analysis and questioning work.
It may use command-local visual support assets when seeing alternatives is materially clearer than describing them.
It should not quietly become planning or execution.

## Approval expectations

The user should explicitly sign off on the selected direction before `plan` treats it as settled.
`brainstorm` can prepare direction approval, but it does not replace the later workflow gates.

## Next command recommendations

- `plan` in the normal case
- `explore` only when the chosen direction still depends on repo context that has not been investigated yet

## Failure / escalation behavior

- if the question is not actually about shape or trade-offs, route to the more appropriate command
- if required context is missing, say so before pretending the options are complete
- if the user rejects all presented directions, stay in `brainstorm` and reopen the shape space deliberately
