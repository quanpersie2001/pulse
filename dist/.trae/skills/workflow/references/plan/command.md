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
- epic/story README handling
- approved TASK/BUG materialization posture
- validation plan
- Gate 2 approval request

## Entry criteria

Run `pulse:workflow plan` when:

- story `discovery.md` exists
- story `solution-design.md` exists and is approved or explicitly approval-ready
- runtime/workgraph posture identifies the active epic/story boundary
- the next work is decomposition, docs impact, README handling, validation plan, and task materialization

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
- active epic/story workgraph state queried through `node .trae/skills/workflow/scripts/pulse.mjs workgraph ... --json`
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

### Work content README handling

Intake creates or matches EPIC/STORY boundaries and writes `intake.md`. Plan handles README content without creating duplicate work items:

- create epic `README.md` from [epic.readme.md](../../templates/epic.readme.md) when the EPIC exists but README content is missing
- enrich existing epic `README.md` from [epic.readme.md](../../templates/epic.readme.md) when useful content already exists
- create story `README.md` from [story.readme.md](../../templates/story.readme.md) when the STORY exists but README content is missing
- enrich existing story `README.md` from [story.readme.md](../../templates/story.readme.md) when useful content already exists
- create TASK/BUG README content from [task.readme.md](../../templates/task.readme.md) only after `workgraph create --json` returns the canonical `content_path`

Do not create duplicate EPIC/STORY items when intake already established the boundary. README creation or enrichment is content handling, not boundary creation.

### Workgraph via CLI only

Use `node .trae/skills/workflow/scripts/pulse.mjs workgraph ... --json` for all workgraph reads and mutations. Treat `plan.md` as the approved information artifact only: it records the items and edges to materialize, but it must not contain a generic operations/how-to table. Keep CLI usage guidance here in `command.md` so agents know exactly how to create the approved work after Gate 2.

Use [workgraph-model.md](../shared/workgraph-model.md) when deciding dependency vs link semantics, readiness behavior, or owner/reservation boundaries.

### Workgraph materialization CLI guide

After explicit Gate 2 approval, materialize only the approved TASK/BUG rows and edge rows recorded in `plan.md`. Use the rendered `node .trae/skills/workflow/scripts/pulse.mjs` value from the installed workflow skill; do not call `scripts/pulse.mjs` by a guessed filesystem path.

#### 1. Create approved TASK/BUG items

For each approved item row, run one create command. Use the active STORY ID as `--parent`; TASK and BUG items must be children of a STORY.

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph create \
  --repo-root <repo> \
  --kind TASK \
  --parent <story-id> \
  --title "<approved task title>" \
  --label "<optional-label>" \
  --risk "<optional-risk-flag>" \
  --json
```

Use `--kind BUG` for bug items. Optional fields are available when approved: `--owner <owner>`, `--priority <n>`, repeated `--label <label>`, and repeated `--risk <flag>`.

Read the JSON response and record the returned values before creating edges:

- `item.id` — canonical work item ID to use instead of the plan temp ref (`W1`, `W2`, etc.)
- `item.content_path` — canonical README path for the item
- `item.verification_path` — canonical verification path for TASK/BUG items

Maintain a temp-ref map while materializing, for example:

```text
W1 -> T-12, content_path=works/epics/<epic>/<story>/T-12-<slug>/README.md
W2 -> T-13, content_path=works/epics/<epic>/<story>/T-13-<slug>/README.md
```

Do not invent IDs, slugs, or content paths. Use only the values returned by `workgraph create --json`.

#### 2. Write TASK/BUG README content at returned paths

After each create command returns, write or enrich the README at `item.content_path` using [task.readme.md](../../templates/task.readme.md) and the approved item information from `plan.md`. Preserve the returned path and ID. Do not write task README files before create returns.

#### 3. Add approved dependency edges

For each dependency edge row in `plan.md`, resolve temp refs through the temp-ref map, then run:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph dep add \
  --repo-root <repo> \
  <item-id> \
  <depends-on-item-id> \
  --json
```

Direction matters: `<item-id>` is blocked by `<depends-on-item-id>`. Use `dep add` only for blocking dependencies that affect readiness.

Example: if `W2` depends on `W1`, and the temp-ref map is `W2 -> T-13`, `W1 -> T-12`, run:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph dep add --repo-root <repo> T-13 T-12 --json
```

#### 4. Add approved traceability links

For each non-blocking traceability row in `plan.md`, resolve temp refs through the temp-ref map, then run:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph link add \
  --repo-root <repo> \
  <item-id> \
  <linked-item-id> \
  --json
```

Links are for related-item traceability only; they must not be used when readiness or execution order depends on another item.

#### 5. Verify workgraph consistency

After all approved items, README content, dependency edges, and links are materialized, run:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph doctor --repo-root <repo> --json
```

If doctor reports issues, repair only issues caused by the materialization pass. Do not create speculative items or unapproved edges while repairing.

### Work item decomposition

Plan must decompose the approved story into TASK/BUG items like a lightweight Jira plan:

- each TASK/BUG has a clear title, purpose, file scope, verification expectation, and design decision refs
- `depends_on` means one item cannot safely execute or complete until another item is closed
- `link` means non-blocking traceability only; it must not affect readiness or execution order
- item IDs and content paths are not hand-authored in `plan.md`; use placeholders until `node .trae/skills/workflow/scripts/pulse.mjs workgraph create ... --json` returns canonical values
- before Gate 2 approval, record the intended TASK/BUG items and edge posture only
- after Gate 2 approval, create items with `node .trae/skills/workflow/scripts/pulse.mjs workgraph create ... --json`, then add approved edges with `node .trae/skills/workflow/scripts/pulse.mjs workgraph dep add ... --json` and `node .trae/skills/workflow/scripts/pulse.mjs workgraph link add ... --json`
- after materialization, run or request `node .trae/skills/workflow/scripts/pulse.mjs workgraph doctor --repo-root <repo> --json`

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
3. Query active epic/story through `node .trae/skills/workflow/scripts/pulse.mjs workgraph ... --json`.
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

Use [plan.template.md](./plan.template.md) as the starting structure for the story `plan.md`. Keep the artifact focused on implementation detail: what will be implemented, where, how, and how completion will be proven.

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
- approved TASK/BUG items, dependency edges, and traceability links
- README creation/enrichment posture when relevant
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
- README handling reuses existing EPIC/STORY items and does not dominate the implementation plan
- TASK/BUG materialization posture includes planned `workgraph create`, `dep add`, and `link add` operations without pre-approval mutations
- no product, architecture, schema, API, UX, migration, or verification-strategy decisions were added

Fix issues once. If serious issues remain, stop and route as below.

### Phase 3 — Approval-ready output

Present `plan.md` for explicit Gate 2 approval.

Gate 2 approves decomposition, docs impact, README handling, scope and completion boundary, and TASK/BUG materialization posture. It does not approve new solution decisions.

Before approval, `pulse:workflow plan` may produce or repair the approval-ready `plan.md`. It must not create TASK/BUG items, mutate workgraph metadata, mark the plan approved, or write task README files.

### Phase 4 — Post-approval materialization

Continue only after explicit Gate 2 approval.

After approval:

1. mark `plan.md` approved
2. create or enrich epic/story `README.md` content as approved, using the README templates only for missing or enriched sections
3. create approved TASK/BUG items through `node .trae/skills/workflow/scripts/pulse.mjs workgraph create ... --json`
4. write task README content at each returned `content_path` using [task.readme.md](../../templates/task.readme.md)
5. add approved dependency/link edges through `node .trae/skills/workflow/scripts/pulse.mjs workgraph dep/link ... --json`
6. run or request `node .trae/skills/workflow/scripts/pulse.mjs workgraph doctor --repo-root <repo> --json`
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
- epic/story README creation/enrichment posture recorded
- approved TASK/BUG materialization posture recorded without mutations
- scope and completion contract
- validation plan with observable evidence
- explicit Gate 2 approval request

### Post-approval exit

After explicit Gate 2 approval, a successful materialization pass requires:

- approved lowercase `plan.md` under the owning story
- epic/story README creation/enrichment completed when approved
- approved TASK/BUG materialization applied only through `node .trae/skills/workflow/scripts/pulse.mjs workgraph`
- task README content written from returned `content_path` values
- `.pulse/runtime` mirrors synchronized
- next recommendation: `pulse:workflow validate`
