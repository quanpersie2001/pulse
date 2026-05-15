# `/pulse explore`

Maps from legacy `exploring`.

## Intent

Use this command when the work needs repo-aware understanding before it can be shaped confidently.

## Inputs expected

Bring the problem statement and whatever context already exists:

- current request or bug statement
- relevant repo docs, code paths, or prior decisions
- known constraints, stakeholders, or success criteria
- any existing runtime or work artifacts that bound the investigation

Useful shared references:

- `../../references/shared/workflow-contract.md`
- `../../references/shared/planes-and-artifacts.md`
- `../../references/shared/approval-gates.md`
- `../../references/shared/handoff-and-resume.md`

## Primary outputs/artifacts

Typical outputs are:

- exploration findings
- context synthesis
- locked assumptions or decision candidates
- a recommendation for what planning should treat as true

## Interaction model

`explore` is primarily read, search, and synthesis work.
It should avoid mutating runtime or workgraph state unless the user explicitly asks for artifact creation.

## Approval expectations

The command prepares Gate 1 material.
Before downstream planning relies on the context as settled, the user should explicitly approve the exploration output.

## Next command recommendations

- `plan` when context is now clear enough to shape implementation
- `brainstorm` when the investigation reveals the feature shape itself is still underdetermined

## Failure / escalation behavior

- if the repo state is not trustworthy yet, route back to `onboard`
- if the problem statement is still too vague, route to `brainstorm`
- if contradictory evidence appears, keep the contradiction explicit instead of forcing a false synthesis
