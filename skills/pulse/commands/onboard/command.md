# `/pulse onboard`

Maps from legacy `preflight` + `using-pulse`.

## Intent

Use this command to bootstrap, inspect, or repair Pulse readiness for the current repo.
It is the authority for readiness posture in the single-router model.

## Inputs expected

Bring whichever of these are available:

- the repo root or current checkout
- the user's requested mode if they have one
- any suspicious runtime symptoms or stale state
- any legacy artifacts already present in the repo

Supporting local references:

- `references/readiness.md`
- `references/migration-warnings.md`
- `../../references/HARNESS.md`
- `../../references/shared/planes-and-artifacts.md`

## Primary outputs/artifacts

Typical outputs are:

- a readiness brief
- recommended operating mode
- migration warnings for legacy assumptions or artifacts
- a next-command recommendation

Phase 1 also provides a thin onboarding entrypoint at `scripts/onboard_pulse.mjs`.

## Interaction model

`onboard` may inspect repo state, read runtime artifacts, and call bootstrap helpers.

In Phase 1, the local script is intentionally a thin wrapper around the existing onboarding logic so the new command surface exists before runtime relocation happens.

## Approval expectations

No human gate is required just to inspect readiness.
Ask for confirmation before applying repo mutations or repair actions that change local files.

## Next command recommendations

- `explore` when the repo is ready and the next need is context discovery
- `brainstorm` when the feature shape is still vague
- `plan` when context is already locked and the next move is execution shaping

## Failure / escalation behavior

- if core prerequisites are missing, stop with explicit remediation guidance
- if the repo still depends on legacy artifacts, surface them as migration warnings instead of hiding them
- if readiness is ambiguous, stay in `onboard` until the state is trustworthy enough to route onward
