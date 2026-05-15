# `/pulse validate`

Maps from legacy `validating`.

## Intent

Use this command to prove that the planned work is feasible, safe enough, and ready for execution.

## Inputs expected

Bring:

- the selected shape artifact
- the current scope or active work slice
- known risks, dependencies, and open questions
- any existing proof, prototype output, or verification expectations

Useful shared references:

- `../../references/shared/approval-gates.md`
- `../../references/shared/verification-contract.md`
- `../../references/shared/workgraph-model.md`

## Primary outputs/artifacts

Typical outputs are:

- risk surface
- blockers
- evidence summary
- execution-readiness call
- recommended execution mode

## Interaction model

`validate` is proof-seeking work.
It may inspect code, run targeted checks, or gather feasibility evidence, but it should not quietly become execution.

## Approval expectations

This command prepares Gate 3.
Execution should not start until the user explicitly approves the validated work slice.

## Next command recommendations

- `swarm` when the validated work should be executed in parallel
- `execute` when a single executor is the right mode
- `plan` or `brainstorm` when validation exposes shape problems

## Failure / escalation behavior

- if proof is missing, say exactly what evidence is still required
- if the plan is unsound, send the work back to `plan`
- if the product shape itself is wrong, send the work back to `brainstorm`
