# `pulse:workflow onboard` Migration Warnings

Use these warnings when a repo still carries legacy Pulse assumptions that the router should not normalize silently.

## Warning model

A migration warning is not the same thing as a hard blocker.

- **Blocker** -> the repo cannot proceed safely at all
- **Warning** -> the repo can still proceed, but the operator should know a legacy contract is still in play

## Common warnings

| Legacy artifact or assumption | Why it matters | Suggested message |
| --- | --- | --- |
| `.beads/` exists | the repo may still think beads are the canonical metadata system | `Legacy bead state detected; treat it as migration context, not the target Pulse v2 source of truth.` |
| `history/` is the main workflow source | the repo may still be organized around the old feature-history model | `Legacy history artifacts detected; keep them explicit while migrating toward workgraph + works separation.` |
| `br` or `bv` is treated as mandatory | the old runtime contract is still driving readiness language | `Legacy bead tooling assumptions still exist; Pulse v2 targets pulse:workflow + pulse-work instead.` |
| docs or helpers still mention `preflight` | bootstrap authority has not fully collapsed into `onboard` yet | `Legacy preflight references remain; pulse:workflow onboard is the new router authority.` |
| docs or helpers still mention `using-pulse` | routing language has not fully collapsed into `pulse:workflow` yet | `Legacy using-pulse references remain; pulse:workflow is the new public surface.` |
| `dream` appears as an active route | obsolete capability is still visible | `Legacy dream surface remains visible; it is removed from the target command table.` |

## Operator guidance

When warnings exist:

1. surface them plainly
2. keep them separate from true blockers
3. explain what the target contract expects instead
4. avoid silently routing the user back into the legacy surface

## Escalate when warnings become blockers

Escalate a warning into a blocker only when the legacy state creates conflicting sources of truth or makes the repo unsafe to continue without human intervention.
