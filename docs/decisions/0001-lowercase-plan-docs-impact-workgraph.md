# Lowercase plan artifact, mandatory docs impact, and workgraph materialization

Date: 2026-05-25

## Status

Accepted for its principle; the artifact contract below is superseded by
[Decision 0008](0008-narrow-scope-to-truth-layer.md) and `PRODUCT.md` §5.1/§5.4.
The legacy router mechanism was superseded earlier.

What still holds: planning writes a lowercase `plan.md`; the artifact contract
does not depend on any conversational router; documentation impact is declared
rather than left implicit; and approved workgraph items are changed only through
`pulse work` / `pulse graph`, never by hand-editing canonical storage.

What changed:

- `plan.md` lives under the owning **Ticket** (`works/TK-xxx/plan.md`), not the
  story directory, and is required only at materialization R2 or above.
- Documentation impact is a posture (`required` / `none` / `deferred`) against
  document ids in the docs registry, not a fixed checklist of four surfaces.
  Two of those four surfaces — `docs/ARCHITECTURE.md` and `docs/product/` — do
  not exist in this repository; architecture lives at the root.

## Context

Pulse split legacy planning responsibilities into discovery, design, and planning:

```text
explore -> discovery.md
design  -> solution-design.md
plan    -> plan.md
```

Without a strict planning artifact contract, planning could regress into legacy behavior by mixing solution decisions, approach selection, task decomposition, docs updates, and workgraph edits. The workflow also needs to prevent missing documentation updates when product or workflow behavior changes.

## Decision

The approved planning process writes lowercase `plan.md` under the owning story directory. This artifact contract is independent of any conversational router.

Every `plan.md` must include mandatory documentation impact for:

- `docs/ARCHITECTURE.md`
- `docs/GLOSSARY.md`
- `docs/decisions/`
- `docs/product/`

Each docs surface must be marked `Create`, `Update`, or `No change` with rationale and validation evidence.

Approved current-slice workgraph items must be queried, created, or changed through the Rust `pulse work` and `pulse graph` commands. Canonical graph storage must not be hand-edited during planning.

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
- Keep the artifact contract aligned with current Rust CLI and graph semantics.
