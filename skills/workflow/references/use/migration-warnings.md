# `pulse:workflow use` Migration Warnings

Use migration warnings when a repo still carries legacy workflow assumptions that `use` should not normalize silently.

## Warning model

Migration warnings are not hard blockers by default.

- **Blocker** -> cannot proceed safely
- **Warning** -> can proceed, but legacy contracts remain visible and should be addressed

Escalate a warning to a blocker only when legacy state creates conflicting active truth or makes safe routing impossible without human intervention.

## Locked legacy warning catalog

| Legacy artifact or assumption | Why it matters | Suggested message |
| --- | --- | --- |
| `pulse:preflight` appears as an active route | Readiness authority has moved into `pulse:workflow use` | `Deprecated readiness route detected; use pulse:workflow use as the supported authority.` |
| `pulse:using-pulse` appears as an active route | Workflow routing has collapsed into `pulse:workflow` | `Deprecated workflow router detected; route workflow phases through pulse:workflow <command>.` |
| old workflow phase routes appear as active options | Workflow phases are no longer standalone public skills | `Legacy workflow phase routes detected; use pulse:workflow explore/plan/validate/swarm/execute/review/compound.` |
| `pulse:dream` appears as an active route | The capability was removed from the packaged public surface | `Removed command surface detected; do not route users to pulse:dream.` |
| `skill-catalog.json` is used as active manifest truth | The catalog is redundant with router metadata and can drift | `Legacy skill catalog detected; workflow metadata belongs in skills/workflow/scripts/command-metadata.json.` |
| `br` is required for baseline readiness | Pulse v2 uses `pulse-work` for runtime mutations | `Legacy work-item CLI requirement detected; v2 readiness should rely on pulse-work.` |
| `bv` is required for baseline readiness | Pulse v2 readiness and graph state no longer depend on bv-specific behavior | `Legacy validation CLI requirement detected; v2 readiness should rely on pulse-work and workflow validation.` |
| `.beads/` exists | Repo may still assume old metadata is canonical | `Legacy metadata artifacts detected; treat them as migration context, not Pulse runtime authority.` |
| `history/` is treated as primary workflow source | Active v2 work content belongs under `works/` and metadata belongs in `.pulse/workgraph/items.jsonl` | `Legacy history artifacts detected; maintain explicit separation from current workgraph-driven flow.` |
| top-level `.pulse/current-feature.json` exists | V2 derives current work from `.pulse/runtime/state.json` and workgraph metadata | `Legacy current-feature mirror detected; do not treat it as current runtime truth.` |
| top-level `.pulse/runtime-snapshot.json` exists | V2 no longer persists a separate runtime snapshot artifact | `Legacy runtime snapshot detected; use .pulse/runtime/state.json and generated status instead.` |
| top-level `.pulse/reservations.json` exists | V2 reservations live under `.pulse/runtime/reservations.json` | `Legacy reservation path detected; runtime leases belong under .pulse/runtime/reservations.json.` |
| `.pulse/harness/HARNESS.md` exists | `HARNESS.md` is a workflow reference, not a runtime template | `Runtime HARNESS.md detected; canonical harness guidance lives at skills/workflow/references/HARNESS.md.` |
| docs root lacks canonical shape (`ARCHITECTURE.md`, `GLOSSARY.md`, `decisions/`, `product/`) | Documentation may need downstream target-repo normalization | `Docs structure is non-compliant; use should back up, rebuild, and migrate mapped content when normalization is requested.` |
| legacy `works` layouts persist | Work artifacts may carry older hierarchy assumptions | `Legacy works layout detected; onboarding may migrate structure and require manual follow-up.` |

## Operator guidance

When warnings exist:

1. Surface them explicitly.
2. Keep them separate from blockers.
3. State the expected v2 target contract.
4. Avoid routing operators back into deprecated surfaces.
5. Do not silently dual-write legacy and v2 sources.

## Safe warning posture

A greenfield Pulse v2 repo can operate without legacy CLIs, legacy workflow skill routes, `.beads/`, or `history/`.

Warnings should help brownfield operators migrate deliberately; they should not make old artifacts feel like active runtime dependencies.
