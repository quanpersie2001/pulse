# `pulse:workflow plan`

Task-planning command for turning approved `solution-design.md` into a durable lowercase story-scoped `plan.md`, docs impact, work content enrichment, and approved workgraph materialization posture.

Plan answers:

> How should the approved solution be decomposed into validate-ready work?

Plan does **not** decide product behavior, technical approach, architecture, schema, API, UX, migration posture, or verification strategy. Those belong to `pulse:workflow design`.

Plan may map approved verification requirements into task-level checks, evidence paths, and validation commands.

## Mission

Produce an approved `plan.md` that validation and execution can follow without changing the solution.

`plan.md` must define:

- full task breakdown
- mandatory docs impact
- epic/story README enrichment
- approved TASK/BUG materialization posture
- validation plan
- Gate 2 approval request

## Entry criteria

Run `pulse:workflow plan` when:

- story `discovery.md` exists
- story `solution-design.md` exists and is approved or explicitly approval-ready
- runtime/workgraph posture identifies the active epic/story boundary
- the next work is decomposition, docs impact, README enrichment, validation plan, and task materialization

Block planning when:

- `solution-design.md` is missing, draft, contradictory, or unapproved
- decomposition requires changing or inventing solution decisions
- `discovery.md` lacks evidence needed for docs/task/validation plan
- runtime/workgraph posture is stale, blocked, or cannot identify the active boundary

## Inputs

Minimum story inputs:

- story `discovery.md`
- approved story `solution-design.md` (authoritative)

Runtime/workgraph/docs inputs:

- `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- active epic/story workgraph state queried through `{{pulse_command}} workgraph ... --json`
- targeted docs context after affected surfaces are known:
  - inspect only the required docs surfaces needed to judge impact
  - read `docs/ARCHITECTURE.md` or `docs/GLOSSARY.md` only when architecture/runtime/workgraph terms may change
  - list `docs/decisions/` or `docs/product/` when relevant, then read only docs tied to the approved design or affected surfaces
  - do not crawl all of `docs/` during orientation

Optional fallback inputs; read only for boundary drift, missing carried-forward context, cited evidence, or iteration state:

- story `intake.md`
- story `work-brief.md` when brainstorm was used
- story `references/*.md` when cited but not summarized enough for planning
- `.pulse/memory/critical-patterns.md` when relevant
- prior `plan.md` when iterating

If plan needs `intake.md` or `work-brief.md` to infer missing solution scope, behavior, direction, or constraints, stop and route back to `pulse:workflow design`.

## Core contracts

### Immutable design

`solution-design.md` is immutable input.

Plan may decompose, sequence, map docs/validation evidence, enrich work content, and prepare approved TASK/BUG materialization.

Plan must not revise design, alter schema/API/UX/product behavior, add solution decisions, or silently resolve design gaps.

### Docs impact is mandatory and targeted

Every `plan.md` must include docs impact for:

```text
docs/
├── ARCHITECTURE.md
├── GLOSSARY.md
├── decisions/
└── product/
```

Mandatory docs impact does not mean reading all docs. First identify affected code/runtime/workgraph/product surfaces from `solution-design.md`; then inspect only the docs needed to judge whether those surfaces require `Create`, `Update`, or `No change`.

Each required docs surface must still record an action, rationale, and validation evidence. Do not write “update docs if needed”.

### Work content enrichment

Intake creates or matches EPIC/STORY boundaries and writes `intake.md`. Plan enriches content:

- enrich existing epic `README.md` from [`epic.readme.md`](./epic.readme.md) when needed
- enrich existing story `README.md` from [`story.readme.md`](./story.readme.md)
- create TASK/BUG README content from [`task.readme.md`](./task.readme.md) only after `workgraph create --json` returns the canonical `content_path`

Do not create duplicate EPIC/STORY items when intake already established the boundary.

### Workgraph via CLI only

Treat `.pulse/workgraph/items.jsonl` as database-like storage behind the CLI. Do not read or edit it during planning.

Use `{{pulse_command}} workgraph ... --json` for all workgraph reads and mutations. Use [`../shared/workgraph-model.md`](../shared/workgraph-model.md) when deciding dependency vs link semantics, readiness behavior, or owner/reservation boundaries.

### Planning mode

Use the lightest mode that still makes the implementation reviewable.

| Mode | Use when | Required emphasis |
| --- | --- | --- |
| `spike` | one approved-design assumption needs proof before execution | proof question, evidence, stop/continue criteria |
| `small_change` | <=3 files, LOW risk, simple approved design | concise file-level plan, docs impact, validation commands |
| `standard_feature` | ordered capability or multi-surface change | sequencing, integration points, validation plan |
| `high_risk_feature` | hard-to-reverse, external/security/data/public contract risk | risks, rollback/repair posture, decision/docs evidence |

Above `small_change`, record why the smaller mode is insufficient.

## Phase model

### Phase 0 — Orientation

1. Read minimum story inputs.
2. Choose the planning mode using the table above.
3. Query active epic/story through `{{pulse_command}} workgraph ... --json`.
4. Confirm:
   - active story boundary
   - approved design status
   - design decision IDs and planning constraints
   - discovery evidence needed for task/docs/validation plan
   - runtime mirror sync
   - existing epic/story README posture
5. Identify affected surfaces from the approved design before reading docs.
6. Inspect only docs surfaces relevant to those affected surfaces.
7. Read optional fallback inputs only when needed.

Hard stop if `solution-design.md` is not authoritative or approved.

### Phase 1 — Plan draft

Create or update:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/plan.md
```

Use [`plan.template.md`](./plan.template.md) as the starting structure for the story `plan.md`. Keep the artifact focused on implementation detail: what will be implemented, where, how, and how completion will be proven.

Draft must include:

- inputs read, with reasons
- planning mode
- referenced design decisions
- affected surfaces
- docs impact
- implementation structure focused on what/where/how to implement
- change strategy
- task breakdown
- sequencing/parallelization
- scope and completion contract
- validation plan
- approved work item materialization posture
- README enrichment posture when relevant
- risks and repair posture

### Phase 2 — Self-review

Check `plan.md` before Gate 2 approval:

- inputs read have reasons, including targeted docs reads only after affected surfaces are known
- every task or implementation choice traces to approved design decision IDs
- affected surfaces and file-level implementation structure are concrete
- change strategy covers implementation approach, integration points, data/control flow, and non-goals
- task breakdown covers the full approved work, not only the next/current execution slice
- sequencing and parallelization boundaries are explicit
- scope and completion contract defines the full plan boundary
- validation plan includes proof strategy, test layers, fixtures, commands, expected results, and evidence to produce
- docs impact covers `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, `docs/decisions/`, and `docs/product/`
- README enrichment reuses existing EPIC/STORY items and does not dominate the implementation plan
- TASK/BUG materialization posture uses workgraph CLI only and avoids pre-approval mutations
- no product, architecture, schema, API, UX, migration, or verification-strategy decisions were added

Fix issues once. If serious issues remain, stop and route as below.

### Phase 3 — Approval-ready output

Present `plan.md` for explicit Gate 2 approval.

Gate 2 approves decomposition, docs impact, README enrichment, scope and completion boundary, and TASK/BUG materialization posture. It does not approve new solution decisions.

Before approval, `pulse:workflow plan` may produce or repair the approval-ready `plan.md`. It must not create TASK/BUG items, mutate workgraph metadata, mark the plan approved, or write task README files.

### Phase 4 — Post-approval materialization

Continue only after explicit Gate 2 approval.

After approval:

1. mark `plan.md` approved
2. enrich epic/story `README.md` content as approved, using the README templates only for the sections being enriched
3. create approved TASK/BUG items through `{{pulse_command}} workgraph create ... --json`
4. write task README content at each returned `content_path` using [`task.readme.md`](./task.readme.md)
5. add approved dependency/link edges through `{{pulse_command}} workgraph dep/link ... --json`
6. run or request `{{pulse_command}} workgraph doctor --repo-root <repo> --json`
7. sync `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
8. recommend `pulse:workflow validate`

Create only approved TASK/BUG items. Do not create speculative future backlog.

## Reroutes

Route to `pulse:workflow design` when:

- a solution decision is missing, contradictory, or infeasible
- decomposition requires a different approach
- docs impact reveals a product/architecture decision absent from `solution-design.md`
- plan needs `intake.md` or `work-brief.md` to infer solution scope/direction/behavior/constraints

Route to `pulse:workflow explore` when:

- `discovery.md` lacks evidence needed for task, docs, or validation plan
- external/provider/security/domain research is required

Route to `pulse:workflow use` when:

- runtime/workgraph posture is stale, blocked, invalid, or conflicts with mirrors

## Exit contracts

### Approval-ready exit

Before Gate 2 approval, a successful planning pass requires:

- lowercase `plan.md` under the owning story
- docs impact recorded for all required docs surfaces
- epic/story README enrichment posture recorded
- approved TASK/BUG materialization posture recorded without mutations
- scope and completion contract
- validation plan with observable evidence
- explicit Gate 2 approval request

### Post-approval exit

After explicit Gate 2 approval, a successful materialization pass requires:

- approved lowercase `plan.md` under the owning story
- epic/story README enrichment completed when approved
- approved TASK/BUG materialization applied only through `{{pulse_command}} workgraph`
- task README content written from returned `content_path` values
- `.pulse/runtime` mirrors synchronized
- next recommendation: `pulse:workflow validate`
