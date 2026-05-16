# `/pulse rescue`

Structural intervention command for work that is failing because the approved shape no longer matches reality.

Default mode is diagnosis and recovery shaping, not immediate execution.

## When to run

Run `/pulse rescue` when one or more of these hold:

- repeated execution attempts fail without reducing uncertainty
- approved shape assumptions no longer fit constraints/repo reality
- fixes are cascading across boundaries (scope/seam/ownership/sequence)
- ownership drift makes continued execution unsafe

If failure is primarily defect diagnosis, route to `systematic-debug`.

## Default mode

Default to **rescue-report-only** unless the user explicitly asks to continue into replanning or execution.

In rescue-report-only mode:

- do not restart execution
- do not mutate workgraph status solely to make progress appear unblocked
- do not broaden scope implicitly

## Required reads

For the target rescue slice, read:

1. current work item metadata in `.pulse/workgraph/items.jsonl`
2. runtime gate context in `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
3. latest approved shape artifacts under `works/epics/**/<story>/` (`README.md`, `SPEC.md`, and story `validation.md` when present)
4. execution evidence for affected task/bug items under `works/epics/**/<story>/tasks/**/verification.md` when present
5. command appendices:
   - `skills/pulse/commands/rescue/references/LANGUAGE.md`
   - `skills/pulse/commands/rescue/references/DEEPENING.md`
   - `skills/pulse/commands/rescue/references/INTERFACE-DESIGN.md`

## Inputs

- failing work slice and failure pattern timeline
- latest approved shape artifacts + story validation and task/bug verification evidence
- current constraint conflicts (scope/runtime/ownership/dependencies)
- known invariants that should remain stable

## Phase order (rescue protocol)

### Phase 1 — Stabilize and stop-the-bleed

Immediately halt unsafe churn:

- stop speculative patch cascades
- preserve current evidence/state for diagnosis
- mark what cannot safely proceed under current shape

### Phase 2 — Diagnose at shape level

Classify root mismatch source:

- scope error
- seam/interface mismatch
- ownership ambiguity/drift
- sequencing/dependency violation
- assumption drift since prior approval

Explicitly separate structural mismatch from ordinary bug-fix work.

### Phase 3 — Generate bounded recovery options

Produce 2-3 credible rescue paths. For each path, include:

- blast radius
- coordination and timeline cost
- reversibility
- invariants preserved vs boundaries changed
- residual risk if chosen

### Phase 4 — Require explicit boundary approval

Any architecture/scope boundary change requires explicit user sign-off.

Before re-entry, lock:

- new in-scope/out-of-scope boundaries
- stop/go criteria
- required feasibility checks

### Phase 5 — Route to correct downstream command

- `plan` when contracts/boundaries need reshaping
- `validate` when feasibility proof is required before execution
- `execute` only when rescued path is bounded, approved, and execution-ready

## Output contract

- structural diagnosis + failure classification
- approved recovery direction and rejected alternatives
- explicit boundary updates (in/out-of-scope)
- re-entry command with readiness conditions

## Stopping rules

Stop and escalate when:

- no bounded credible recovery path exists
- required approval for boundary change is missing
- risk surface appears systemic beyond current slice

Do not restart execution from an unapproved rescue proposal.

## Guardrails

- Never continue a known-bad path for momentum.
- Never relabel architecture drift as a minor bug.
- Never skip boundary re-approval before re-entry.
- Never hide rescue trade-offs behind vague “cleanup” framing.

## Anti-patterns

- “One more patch” loops after repeated failures
- Recovery plans with no reversibility analysis
- Implicitly expanding scope to absorb structural issues
- Returning to execution without updated contracts

## Escalation posture

Escalation output must include:

- what failed structurally
- decisions that now require operator approval
- smallest safe recovery path
- explicit consequences of non-decision (continue blocked vs downgraded)

## Next command guidance

- `plan` for new shape synthesis
- `validate` for feasibility confirmation
- `systematic-debug` for root-cause defect tracks
- `execute` only after rescue boundary acceptance
