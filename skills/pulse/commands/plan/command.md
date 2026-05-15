# `/pulse plan`

Maps from legacy `planning`.

## Intent

Use this command to convert explored context and approved direction into a concrete implementation shape.

## Inputs expected

Bring:

- approved context from earlier discovery work
- constraints and decision boundaries
- target files, systems, or work slices when known
- the desired implementation scope for this cycle

Useful shared references:

- `../../references/shared/workflow-contract.md`
- `../../references/shared/approval-gates.md`
- `../../references/shared/planes-and-artifacts.md`
- `../../references/shared/workgraph-model.md`

## Primary outputs/artifacts

Typical outputs are:

- work breakdown
- selected shape artifact
- recommended execution mode
- explicit follow-up steps and dependencies

## Interaction model

`plan` is a shaping command.
It should turn context into an execution-ready structure without silently mutating runtime state or starting implementation.

## Approval expectations

This command prepares Gate 2.
The selected shape artifact should be approved before validation or execution treats it as authoritative.

## Next command recommendations

- `validate` in the normal case
- `brainstorm` if the chosen shape still depends on unresolved product-level trade-offs

## Failure / escalation behavior

- if the context is not approved or still contradictory, go back to `explore`
- if the shape is still fundamentally ambiguous, go back to `brainstorm`
- if the proposed work cannot be scoped cleanly, say so explicitly instead of hiding it inside the plan
