# Lowercase plan artifact, mandatory docs impact, and workgraph materialization

Date: 2026-05-25

## Status

Accepted

## Context

Pulse split legacy planning responsibilities into discovery, design, and planning:

```text
explore -> discovery.md
design  -> solution-design.md
plan    -> plan.md
```

Without a strict planning artifact contract, planning could regress into legacy behavior by mixing solution decisions, approach selection, task decomposition, docs updates, and workgraph edits. The workflow also needs to prevent missing documentation updates when product or workflow behavior changes.

## Decision

`pulse:workflow plan` writes lowercase `plan.md` under the owning story directory.

Every `plan.md` must include mandatory documentation impact for:

- `docs/ARCHITECTURE.md`
- `docs/GLOSSARY.md`
- `docs/decisions/`
- `docs/product/`

Each docs surface must be marked `Create`, `Update`, or `No change` with rationale and validation evidence.

Approved current-slice workgraph items must be queried, created, or changed through `{{pulse_command}} workgraph`. `.pulse/workgraph/items.jsonl` is database-like storage behind the CLI and must not be read or hand-edited during planning.

## Alternatives Considered

1. Keep uppercase `PLAN.md`.
   - Rejected because the workflow now standardizes story artifacts as lowercase markdown names (`intake.md`, `discovery.md`, `solution-design.md`, `plan.md`).
2. Let docs updates remain optional.
   - Rejected because docs are part of Pulse's product/workflow contract and missing docs updates create durable drift.
3. Let planning describe workgraph items but leave creation manual.
   - Rejected because manual workgraph edits bypass schema, IDs, derived views, and readiness semantics.

## Consequences

Positive:

- Planning has a single canonical lowercase artifact.
- Documentation impact is never implicit.
- Workgraph metadata remains owned by the runtime CLI.
- Validate can check plan/docs/workgraph consistency with clearer evidence.

Tradeoffs:

- Even small changes must explicitly say why docs do or do not change.
- Plan authors must understand docs/product and docs/decisions conventions.

## Follow-Up

- Keep validation references aligned with lowercase `plan.md`.
- Regenerate packaged skill outputs after source skill changes.
