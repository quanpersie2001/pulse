# Pulse v2 Workflow Router + Workgraph Implementation Spec

## Status

- Status: implementation baseline for Pulse v2 workflow-router + standalone-utility plugin
- Source plan: `PLAN.md`
- Audience: maintainers implementing Pulse v2 in this plugin repo and validating self-host / dogfood behavior

## 1. Purpose

This document turns the current Pulse v2 migration plan into an implementation-ready specification.

It defines:

- the workflow public command surface for Pulse
- the boundary between the workflow router, standalone public utilities, and the `pulse-work` runtime CLI
- the canonical source tree for the plugin repo
- the canonical installed runtime layout under `.pulse/`
- the v1 workgraph data model and validation rules
- the onboarding, hook, eval, and docs rewiring required to remove the legacy workflow-skill contract
- the migration strategy away from `preflight`, `dream`, `skill-catalog.json`, `br`, `bv`, `.beads/`, and `history/` as active runtime contracts

This spec is intentionally concrete. It is not a second proposal.

---

## 2. Locked architectural decisions

### 2.1 One workflow router: `pulse:workflow`

Pulse must expose exactly one public workflow router skill:

- `pulse:workflow onboard`
- `pulse:workflow explore`
- `pulse:workflow brainstorm`
- `pulse:workflow plan`
- `pulse:workflow validate`
- `pulse:workflow execute`
- `pulse:workflow swarm`
- `pulse:workflow review`
- `pulse:workflow compound`

Future workflow-facing commands such as `status` may be added later, but Pulse must not return to a model where each workflow phase ships as its own public skill.

### 2.2 Standalone public utilities remain outside the workflow router

The following are public Pulse skills, but they are **not** workflow phases and must not be folded into `pulse:workflow`:

- `pulse:architecture-rescue`
- `pulse:systematic-debug-fix`
- `pulse:dev-note`
- `pulse:dev-note-distil`
- `pulse:prompt-leverage`
- `pulse:gitnexus`

These skills belong to a different mental model than the happy-path workflow pipeline. They must remain directly invokable and must not be documented as required workflow stages.

### 2.3 Clear separation between router, utilities, and runtime

Pulse v2 has three distinct layers:

- `pulse:workflow ...` = user-facing workflow router
- `pulse:<standalone-skill>` = user-facing utility skill outside the workflow pipeline
- `pulse-work ...` = runtime CLI that manipulates workgraph and runtime state

Examples:

- a user asks for planning through `pulse:workflow plan`
- a user invokes `pulse:architecture-rescue` for an off-pipeline architecture recovery task
- the agent or harness uses `pulse-work create`, `pulse-work ready`, or `pulse-work close` to manipulate canonical workgraph state

The workflow router layer, the standalone utility layer, and the runtime layer must be described, implemented, and tested separately.

### 2.4 `HARNESS.md` is a reference source, not a runtime template

The canonical harness reference must live at:

```text
skills/workflow/references/HARNESS.md
```

It is documentation for the workflow router contract, not a runtime seed file.

### 2.5 `HARNESS_BACKLOG.md` is a template / seed artifact

The canonical backlog template must live at:

```text
skills/workflow/templates/HARNESS_BACKLOG.md
```

The runtime materialization path is:

```text
.pulse/harness/HARNESS_BACKLOG.md
```

### 2.6 `skill-catalog.json` is removed

With a workflow router plus standalone utility skills, `skill-catalog.json` becomes redundant and drift-prone.

The new sources of truth are:

- `skills/workflow/SKILL.md` — workflow router contract and command table
- `skills/workflow/scripts/command-metadata.json` — workflow command description / hint / category metadata
- `skills/<standalone-skill>/SKILL.md` — standalone utility contracts

Until cleanup removes the file, `skill-catalog.json` is treated only as a legacy artifact and must not be used as an active design or manifest truth source.

### 2.7 `preflight` and `dream` are removed from the packaged surface

- `preflight` is absorbed into `pulse:workflow onboard`
- `dream` is removed rather than migrated as a new public route

If any useful behavior from `dream` survives, it must be absorbed deliberately elsewhere rather than kept as a standalone packaged surface.

### 2.8 Residual utility skills must be split into keep vs remove

The following remain public standalone skills outside the workflow router:

- `prompt-leverage`
- `gitnexus`

The following are removed from the target packaged surface:

- `refresh-project-docs`
- `writing-pulse-skills`

Cleanup must preserve that split directly in the packaged skill surface.

### 2.9 Legacy concepts are migration-only language

The following are not active runtime contracts in Pulse v2:

- `br`
- `bv`
- `.beads/`
- `history/<feature>/...`
- `pulse:preflight`
- `pulse:dream`

They may still appear in migration documents, compatibility readers, or audit notes, but they must not remain the live contract for a greenfield Pulse v2 repo.

---

## 3. Goals

Pulse v2 v1 must:

1. collapse the workflow public surface into a single router skill: `pulse:workflow`
2. keep rescue/debug/note/support capabilities as standalone public skills outside that router
3. replace the runtime dependency on `br` and `bv` with `pulse-work`
4. ship a minimal built-in workgraph owned by Pulse
5. make `.pulse/workgraph/items.jsonl` the only canonical writable metadata source
6. move canonical runtime state under `.pulse/runtime/`
7. keep canonical runtime source under `skills/workflow/scripts/runtime/`
8. make the plugin source tree and the installed runtime layout explicit and non-conflicting
9. reduce runtime duplication and top-level `.pulse/` file sprawl
10. support self-hosting and brownfield migration without silent dual sources of truth
11. remain repo-local and zero-service: no external daemon, no hosted coordinator

---

## 4. Non-goals

Pulse v2 v1 does **not** need to:

- keep multiple standalone workflow-phase public skills
- force rescue/debug/note utilities into the workflow router
- keep `preflight` or `dream` as packaged routes
- keep `skill-catalog.json`
- preserve long-term dual-write compatibility with `history/`, `.beads/`, `br`, or `bv`
- introduce a separate event log beyond `items.jsonl`
- add a persisted `assignee` field to canonical item metadata
- preserve `current-feature.json` and `runtime-snapshot.json` as separate persisted artifacts
- duplicate `HARNESS.md` into the runtime plane as a second canonical source
- automate one-shot migration of every legacy repo on day one
- add a new external runtime dependency if repo-local Node scripts are sufficient

During migration, temporary read-compatibility is allowed. Temporary dual-source-of-truth is not.

---

## 5. Current-state constraints in this repo

Pulse v2 is an architectural migration, not a folder rename.

The current implementation is tightly coupled to the old model:

- workflow behavior is spread across many packaged skills instead of one workflow router
- utility skills and workflow phases are not cleanly separated in the public mental model
- onboarding and readiness are split between `preflight` and `using-pulse`
- runtime source scripts currently live under `skills/using-pulse/scripts/`
- the repo also mirrors installed runtime helpers under `.pulse/scripts/`
- runtime state is currently spread across top-level `.pulse/state.json`, `.pulse/STATE.md`, `.pulse/current-feature.json`, `.pulse/runtime-snapshot.json`, `.pulse/reservations.json`, and `.pulse/handoffs/manifest.json`
- `preflight` and `dream` still appear across docs, tests, hooks, manifests, and evals
- `skill-catalog.json` reflects the old multi-skill era and adds another routing layer
- the current skill inventory still mixes standalone public skills that should stay with legacy utilities that should be removed, which creates packaging ambiguity
- docs currently blur the boundary between this plugin repo and downstream/self-hosted target repos, especially around `.pulse/` and `.gitignore`

Because of this, Pulse v2 implementation must update runtime scripts, router contracts, standalone utility classification, docs, onboarding, hooks, evals, and tests in coordinated phases.

---

## 6. Canonical repo architecture

### 6.1 Target plugin source tree

The canonical target structure for this repository describes the plugin repo itself, not only a downstream installed repo.

```text
pulse/
|-- .agents/
|   `-- plugins/
|       `-- marketplace.json
|-- .claude-plugin/
|   |-- plugin.json
|   `-- marketplace.json
|-- .codex-plugin/
|   `-- plugin.json
|-- .codex/
|   `-- hooks/
|-- .plugin-eval/
|   `-- benchmark.json
|-- .pulse/
|   |-- workgraph/
|   |   |-- items.jsonl
|   |   |-- schema.json
|   |   |-- write.lock
|   |   `-- views/
|   |       |-- active.json
|   |       |-- closed.json
|   |       |-- ready.json
|   |       `-- graph.json
|   |-- runtime/
|   |   |-- tooling-status.json
|   |   |-- state.json
|   |   |-- STATE.md
|   |   |-- reservations.json
|   |   |-- handoffs/
|   |   |   `-- manifest.json
|   |   `-- checkpoints/
|   |-- harness/
|   |   `-- HARNESS_BACKLOG.md
|   |-- memory/
|   `-- scripts/
|       |-- pulse-work
|       |-- pulse_work.mjs
|       |-- pulse_state.mjs
|       |-- pulse_status.mjs
|       |-- pulse_session_context.mjs
|       `-- pulse_reservations.mjs
|-- assets/
|-- docs/
|   |-- ARCHITECTURE.md
|   |-- evaluation/
|   `-- examples/
|-- hooks/
|-- pulse-eval-workspace/
|-- references/
|   `-- impeccable/
|-- scripts/
|   |-- pulse-plugin-eval.mjs
|   `-- sync-skills.sh
|-- skills/
|   |-- pulse/
|   |   |-- SKILL.md
|   |   |-- references/
|   |   |   |-- HARNESS.md
|   |   |   `-- shared/
|   |   |       |-- workflow-contract.md
|   |   |       |-- planes-and-artifacts.md
|   |   |       |-- workgraph-model.md
|   |   |       |-- approval-gates.md
|   |   |       |-- verification-contract.md
|   |   |       |-- swarm-execution-rules.md
|   |   |       `-- handoff-and-resume.md
|   |   |-- commands/
|   |   |   |-- onboard/
|   |   |   |   |-- command.md
|   |   |   |   |-- references/
|   |   |   |   |   |-- readiness.md
|   |   |   |   |   `-- migration-warnings.md
|   |   |   |   `-- scripts/
|   |   |   |       `-- onboard_pulse.mjs
|   |   |   |-- explore/
|   |   |   |   `-- command.md
|   |   |   |-- brainstorm/
|   |   |   |   |-- command.md
|   |   |   |   |-- references/
|   |   |   |   |   |-- spec-reviewer-prompt.md
|   |   |   |   |   `-- visual-support-guidance.md
|   |   |   |   `-- scripts/
|   |   |   |       |-- start-visual-server.sh
|   |   |   |       |-- stop-visual-server.sh
|   |   |   |       |-- visual-frame-template.html
|   |   |   |       |-- visual-helper.js
|   |   |   |       `-- visual-server.cjs
|   |   |   |-- plan/
|   |   |   |   `-- command.md
|   |   |   |-- validate/
|   |   |   |   `-- command.md
|   |   |   |-- swarm/
|   |   |   |   `-- command.md
|   |   |   |-- execute/
|   |   |   |   `-- command.md
|   |   |   |-- review/
|   |   |   |   `-- command.md
|   |   |   `-- compound/
|   |   |       `-- command.md
|   |   |-- templates/
|   |   |   |-- HARNESS_BACKLOG.md
|   |   |   `-- works/
|   |   |       |-- epic-README.md
|   |   |       |-- story-README.md
|   |   |       |-- story-SPEC.md
|   |   |       |-- task-README.md
|   |   |       `-- verification.md
|   |   `-- scripts/
|   |       |-- command-metadata.json
|   |       |-- runtime/
|   |       |   |-- pulse_work.mjs
|   |       |   |-- workgraph_store.mjs
|   |       |   |-- workgraph_validate.mjs
|   |       |   |-- workgraph_ids.mjs
|   |       |   |-- workgraph_paths.mjs
|   |       |   |-- workgraph_views.mjs
|   |       |   |-- workgraph_lock.mjs
|   |       |   |-- workgraph_templates.mjs
|   |       |   |-- pulse_state.mjs
|   |       |   |-- pulse_status.mjs
|   |       |   |-- pulse_session_context.mjs
|   |       |   `-- pulse_reservations.mjs
|   |       `-- lib/
|   |           |-- resolve-command.mjs
|   |           |-- render-help.mjs
|   |           `-- paths.mjs
|   |-- architecture-rescue/
|   |-- dev-note/
|   |-- dev-note-distil/
|   |-- gitnexus/
|   |-- prompt-leverage/
|   `-- systematic-debug-fix/
|-- tests/
|   |-- pulse/
|   |-- runtime/
|   `-- integration/
|-- AGENTS.md
|-- CLAUDE.md
|-- CONTRIBUTING.md
|-- README.md
|-- PLAN.md
`-- SPEC.md
```

### 6.2 Materialized runtime layout in a self-hosted or downstream repo

The plugin source tree above is not the same thing as the work-content layout created and managed by `pulse-work`.

When Pulse v2 is self-hosted or installed in a downstream repo, work content must still be materialized under `works/` and runtime state must live under `.pulse/`.

```text
project/
|-- docs/
|   |-- ARCHITECTURE.md
|   |-- GLOSSARY.md
|   |-- decisions/
|   `-- product/
|-- works/
|   |-- backlog.md
|   |-- test-matrix.md
|   `-- epics/
|       `-- E-<id>-<slug>/
|           |-- README.md
|           `-- S-<id>-<slug>/
|               |-- README.md
|               |-- SPEC.md                # brainstorm output
|               |-- approach.md            # optional
|               |-- execplan.md            # optional
|               |-- validation.md          # optional
|               |-- lifecycle-summary.md   # optional
|               |-- references/            # optional
|               `-- tasks/
|                   `-- T-.../ or B-.../
|                       |-- README.md
|                       `-- verification.md
`-- .pulse/
    |-- workgraph/
    |   |-- items.jsonl
    |   |-- schema.json
    |   |-- write.lock
    |   `-- views/
    |       |-- active.json
    |       |-- closed.json
    |       |-- ready.json
    |       `-- graph.json
    |-- runtime/
    |   |-- tooling-status.json
    |   |-- state.json
    |   |-- STATE.md
    |   |-- handoffs/
    |   |   `-- manifest.json
    |   |-- checkpoints/
    |   `-- reservations.json
    |-- harness/
    |   `-- HARNESS_BACKLOG.md
    |-- memory/
    `-- scripts/
```

### 6.3 Explicit removals from the target state

Pulse v2 removes these as active packaged or canonical surfaces:

- `skills/preflight/`
- `skills/using-pulse/`
- `skills/exploring/`
- `skills/planning/`
- `skills/validating/`
- `skills/swarming/`
- `skills/executing/`
- `skills/reviewing/`
- `skills/compounding/`
- `skills/brainstorming/`
- `skills/dream/`
- `skill-catalog.json`
- top-level `.pulse/current-feature.json`
- top-level `.pulse/runtime-snapshot.json`
- top-level `.pulse/reservations.json`

The following may remain only as migration terms or compatibility readers:

- `history/<feature>/...`
- `.beads/`
- `br`
- `bv`

### 6.4 `.gitignore` clarification for this plugin repo

The `.gitignore` of this plugin repo does **not** change as part of the Pulse v2 migration.

In this repository, `.pulse/` remains local dogfood/runtime state and stays ignored by the existing repo policy.

If Pulse v2 later defines a track/ignore policy for downstream or self-hosted target repos, that policy belongs to the installed target repo contract, not to Phase 0 cleanup of this plugin repo.

### 6.5 Skill classification and packaging policy

The skill classification is locked as follows:

- the workflow pipeline collapses into `pulse:workflow`
- `architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil`, `prompt-leverage`, and `gitnexus` remain standalone public utility skills
- `bootstrap-project-context`, `dream`, `refresh-project-docs`, and `writing-pulse-skills` are removed from the target packaged surface
- `skill-catalog.json` remains only a legacy artifact until cleanup and is not a source of truth from this phase onward

This packaging rule is a required dependency for the legacy-surface collapse.

---

## 7. Public surface contract

### 7.1 `pulse:workflow` router behavior

`skills/workflow/SKILL.md` is the only packaged workflow router contract.

With no arguments, `pulse:workflow` must render a command menu or help surface for the supported workflow subcommands.

With a recognized first token, `pulse:workflow` must route to the corresponding command reference.

If the first token does not match a known command, the router must fall back to help / command listing behavior rather than silently dispatching to a legacy skill.

### 7.2 Supported workflow command set

The command table in `skills/workflow/SKILL.md` must support:

- `onboard`
- `explore`
- `brainstorm`
- `plan`
- `validate`
- `swarm`
- `execute`
- `review`
- `compound`

### 7.3 Canonical source files for workflow behavior

The workflow surface is composed from these sources:

- `skills/workflow/SKILL.md` — router entrypoint and workflow command table
- `skills/workflow/commands/<command>/command.md` — per-command behavior entrypoint
- `skills/workflow/commands/<command>/references/*` — command-local reference material
- `skills/workflow/commands/<command>/scripts/*` — command-local helper scripts and assets
- `skills/workflow/references/shared/*.md` — shared workflow contracts and reference material
- `skills/workflow/scripts/command-metadata.json` — structured metadata for workflow command descriptions, hints, and categories

`skill-catalog.json` must not be reintroduced as a second routing metadata layer.

### 7.4 Standalone public utility surface

The following remain standalone public skills and are **not** workflow router subcommands:

- `pulse:architecture-rescue`
- `pulse:systematic-debug-fix`
- `pulse:dev-note`
- `pulse:dev-note-distil`
- `pulse:prompt-leverage`
- `pulse:gitnexus`

Their source-of-truth behavior lives in their own skill directories, not under `skills/workflow/commands/`.

### 7.5 Legacy skill mapping

The public surface maps from the old model as follows:

- `using-pulse` + `preflight` → `pulse:workflow onboard`
- `exploring` → `pulse:workflow explore`
- `brainstorming` → `pulse:workflow brainstorm`
- `planning` → `pulse:workflow plan`
- `validating` → `pulse:workflow validate`
- `swarming` → `pulse:workflow swarm`
- `executing` → `pulse:workflow execute`
- `reviewing` → `pulse:workflow review`
- `compounding` → `pulse:workflow compound`
- `architecture-rescue` → `pulse:architecture-rescue`
- `systematic-debug-fix` → `pulse:systematic-debug-fix`
- `dev-note` → `pulse:dev-note`
- `dev-note-distil` → `pulse:dev-note-distil`
- `bootstrap-project-context` → removed
- `prompt-leverage` → `pulse:prompt-leverage`
- `gitnexus` → `pulse:gitnexus`
- `dream` → removed
- `refresh-project-docs` → removed
- `writing-pulse-skills` → removed

---

## 8. Terminology

Use these names consistently.

| Context | Canonical term |
| --- | --- |
| packaged workflow router surface | `pulse:workflow` |
| standalone public utility surface | `pulse:<standalone-skill>` |
| user-facing workflow subcommand | `workflow command` |
| runtime CLI | `pulse-work` |
| generic human-facing noun | `work item` |
| schema/code short noun | `item` |
| metadata system name | `workgraph` |
| harness contract source | `HARNESS.md` reference |
| harness backlog seed | `HARNESS_BACKLOG.md` template |

Never use these as the generic noun for the v2 model:

- `bead`
- `pulse-skill`
- `pulse-work` as the object name

`pulse-work` is the CLI, not the work item.

---

## 9. Canonical metadata model

### 9.1 Source of truth

The only canonical writable metadata source is:

```text
.pulse/workgraph/items.jsonl
```

Each line is a full snapshot record for one item.

`items.jsonl` is a snapshot file, not an append-only event log.

### 9.2 Required v1 schema file

The repo must contain:

```text
.pulse/workgraph/schema.json
```

`schema.json` is the machine-readable contract. The runtime validator may be handwritten in Node v1, but it must enforce the same rules encoded in `schema.json`.

### 9.3 Item kinds

Supported v1 kinds:

- `EPIC`
- `STORY`
- `TASK`
- `BUG`

### 9.4 Statuses

Supported v1 statuses:

- `OPEN`
- `IN_PROGRESS`
- `BLOCKED`
- `CLOSED`

### 9.5 Canonical item record

Example:

```json
{
  "id": "T-0V9K4H",
  "kind": "TASK",
  "title": "Implement session store",
  "slug": "session-store",
  "status": "OPEN",
  "parent_id": "S-0V9K4G",
  "epic_id": "E-0V9K4F",
  "depends_on": [],
  "priority": 2,
  "owner": null,
  "labels": ["auth", "session"],
  "risk_flags": ["AUTH", "EXISTING_BEHAVIOR"],
  "blocked_reason": null,
  "content_path": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/tasks/T-0V9K4H-session-store/README.md",
  "verification_path": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/tasks/T-0V9K4H-session-store/verification.md",
  "created_at": "2026-05-14T10:00:00Z",
  "updated_at": "2026-05-14T10:00:00Z",
  "closed_at": null
}
```

### 9.6 Required fields

Required on every record unless explicitly nullable:

- `id`
- `kind`
- `title`
- `slug`
- `status`
- `parent_id`
- `epic_id`
- `depends_on`
- `content_path`
- `created_at`
- `updated_at`

### 9.7 Early-required fields

These are part of v1 and should be present from the start:

- `priority`
- `owner`
- `labels`
- `risk_flags`
- `verification_path`
- `blocked_reason`
- `closed_at`

### 9.8 Field rules

#### `priority`

- integer
- `0` is highest priority
- lower number means higher priority

#### `owner`

- nullable durable responsibility field
- may name a person, role, or stable agent identity
- is **not** the ephemeral execution lease

#### `assignee`

- does not exist in v1
- the current active actor belongs in runtime reservation state only

#### `labels`

- free-form string array
- duplicates not allowed

#### `risk_flags`

Strict enum v1:

- `AUTH`
- `DATA`
- `SECURITY`
- `MIGRATION`
- `EXISTING_BEHAVIOR`
- `EXTERNAL_API`
- `PERFORMANCE`
- `UX`
- `CI`
- `UNKNOWN`

#### Timestamps

- ISO 8601 UTC strings
- `created_at` is set once
- `updated_at` changes on every metadata mutation
- `closed_at` is required when `status = CLOSED`, otherwise `null`

### 9.9 Hierarchy rules

Parent-child structure and dependency graph are distinct.

Allowed parents:

- `EPIC.parent_id = null`
- `STORY.parent_id -> EPIC`
- `TASK.parent_id -> STORY`
- `BUG.parent_id -> STORY`

Additional rules:

- every item must have `epic_id`
- for `EPIC`, `epic_id` must equal `id`
- for all descendants, `epic_id` must match the ancestor epic

### 9.10 Dependency rules

- dependencies may cross epic boundaries
- dependency IDs must exist
- no self-dependency
- no duplicate IDs in `depends_on`
- no cycles
- cycle creation must be blocked at mutation time
- `pulse-work doctor` must still detect cycles if files are edited manually

### 9.11 Status transition rules

Allowed transitions:

```text
OPEN        -> IN_PROGRESS | BLOCKED | CLOSED
IN_PROGRESS -> BLOCKED | CLOSED | OPEN
BLOCKED     -> OPEN | IN_PROGRESS | CLOSED
CLOSED      -> OPEN   # only through reopen
```

Rules:

- `BLOCKED` is only for external blockers, not unresolved dependencies
- dependency blocking is derived state
- a dependency-blocked item can remain `OPEN`
- `pulse-work reopen` is the only supported reopen path

### 9.12 Close rules

- parent items cannot close while any child is not `CLOSED`
- `TASK` and `BUG` cannot close without a valid `verification_path`
- closing sets `closed_at`
- reopening clears `closed_at`
- `blocked_reason` is required when `status = BLOCKED` and must otherwise be `null`

---

## 10. ID strategy

### 10.1 Canonical ID format

```text
<KIND>-<TIMESECOND>[-<SEQ>]
```

Examples:

- `E-0V9K4F`
- `S-0V9K4G`
- `T-0V9K4H`
- `T-0V9K4H-1`
- `B-0V9K4J`

### 10.2 ID generation rules

- kind prefix map: `EPIC -> E`, `STORY -> S`, `TASK -> T`, `BUG -> B`
- `<TIMESECOND>` is UTC Unix seconds encoded with uppercase Crockford-style Base32 to avoid visually ambiguous characters
- `-1`, `-2`, ... suffixes are only used for same-kind, same-second collisions
- IDs are immutable after creation
- persisted IDs are uppercase
- CLI lookup may normalize input case

### 10.3 Collision handling

`pulse-work create` must:

1. generate the base ID
2. check existing IDs in `items.jsonl`
3. append the first unused numeric suffix when needed
4. write only the final canonical ID

### 10.4 Prefix resolution

CLI item lookup must support unique prefixes.

Rules:

- resolution is case-insensitive
- exact canonical match wins
- otherwise, prefix match is allowed only when exactly one record matches
- ambiguous prefixes are hard errors and must list candidates

---

## 11. Slug and path strategy

### 11.1 Slug generation

Slugs must be generated by the CLI and stored on the item record.

Rules:

- normalize Unicode to ASCII where possible
- lowercase
- kebab-case
- remove unsafe characters
- collapse repeated separators
- reject path traversal and absolute paths
- fallback to `item` only when the sanitized title would otherwise be empty

### 11.2 Canonical content paths

#### `EPIC`

```text
works/epics/<epic-id>-<epic-slug>/README.md
```

#### `STORY`

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/README.md
```

The story directory may also contain:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/SPEC.md
```

`SPEC.md` is the story-level brainstorming artifact. It is not the canonical `content_path` for the story item; `README.md` remains the story description entry file.

#### `TASK` or `BUG` under a `STORY`

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/tasks/<item-id>-<item-slug>/README.md
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/tasks/<item-id>-<item-slug>/verification.md
```

### 11.3 Path safety rules

The CLI must:

- reject writes outside `works/`
- reject `..`, absolute paths, encoded traversal, and symlink escapes
- use repo-relative POSIX-style stored paths
- refuse to overwrite unrelated existing files or folders

### 11.4 Rename and move rules

Identity comes from ID, not path.

However, path segments include parent slugs, so parent slug changes must cascade.

Required behavior:

- renaming an `EPIC` slug must move the epic folder and update all descendant `content_path` and `verification_path` values in one transaction
- renaming a `STORY` slug must move the story folder and update all descendant task / bug paths in one transaction
- changing a `TASK` or `BUG` slug only updates its own directory and paths
- manual moves are unsupported and must be reported by `pulse-work doctor`

---

## 12. Markdown content rules

### 12.1 Canonical entry file

Every item directory uses `README.md` as the canonical entry file.

Do not use kind-specific filenames such as `story.md` or `task.md`.

A story directory may additionally contain `SPEC.md` as the brainstorming artifact, but that file does not replace the canonical story entry file.

### 12.2 Allowed frontmatter

Frontmatter may contain only the identity trace:

```yaml
---
id: T-0V9K4H
---
```

Do not persist item metadata in markdown frontmatter.

Forbidden in frontmatter and markdown mirrors:

- `status`
- `depends_on`
- `priority`
- `owner`
- `blocked_reason`
- `created_at`
- `updated_at`
- `closed_at`

### 12.3 Required templates

#### Epic `README.md`

Must include sections for:

- title
- objective
- boundary
- related stories
- open questions

#### Story `README.md`

Must include sections for:

- title
- request summary
- scope
- non-goals
- acceptance criteria
- related product docs
- related decisions
- important task / bug evidence links when relevant

`README.md` is the durable story description. It is not the brainstorming output artifact.

#### Story `SPEC.md`

`SPEC.md` is the required output of `pulse:workflow brainstorm` for a story-scoped design pass.

Must include sections for:

- problem statement and goals
- approved direction summary
- key components and behavior
- data or control-flow expectations when relevant
- error handling and fallback posture
- testing or verification intent
- out-of-scope and deferred items

#### Task / Bug `README.md`

Must include sections for:

- scope
- implementation notes
- related files
- caveats

#### Task / Bug `verification.md`

Must include these headings in v1:

- `## Evidence Summary`
- `## Commands Run`
- `## Observed Outputs`
- `## Attempts`
- `## Artifacts`
- `## Unresolved Gaps`

The sections may be short, but they must exist.

### 12.4 Verification minimum for close

`pulse-work close` for `TASK` and `BUG` must verify that:

1. `verification.md` exists
2. it is not empty
3. it contains all required headings
4. `## Unresolved Gaps` explicitly states either remaining gaps or `None.`

This is the minimum mechanical proof gate for v1.

---

## 13. Generated views

### 13.1 Location

```text
.pulse/workgraph/views/
```

### 13.2 Files

- `active.json`
- `closed.json`
- `ready.json`
- `graph.json`

### 13.3 Rules

- views are derived data only
- views are rebuilt after every successful mutation
- views must be written atomically
- in this plugin repo they live under ignored local `.pulse/` state
- onboarding and `pulse-work doctor --fix` must respect repo-local ignore policy and must not assume this plugin repo rewrites `.gitignore`

### 13.4 View content

Views may enrich canonical records with derived fields.

Allowed derived fields include:

- `ready`
- `blocked_by_dependencies`
- `children`
- `reverse_dependencies`
- `descendant_count`

Derived fields must never flow back into `items.jsonl` unless they are canonical record fields.

### 13.5 Sorting

`ready.json` and `pulse-work ready` must sort by:

1. `priority` ascending
2. `created_at` ascending
3. `id` ascending

---

## 14. Runtime write model

### 14.1 Queue and lock

All metadata mutations must flow through the `pulse-work` runtime queue.

The queue model is:

- in-memory, process-local queue inside the current CLI process
- filesystem lock shared across concurrent processes on the same checkout
- no durable pending-event log

Canonical lock path:

```text
.pulse/workgraph/write.lock
```

Lock contents should be JSON and include at least:

- `pid`
- `hostname`
- `started_at`
- `command`

### 14.2 Mutation flow

Every mutating command must:

1. acquire the lock
2. read and parse `items.jsonl`
3. apply the mutation in memory
4. validate the entire graph
5. write `items.jsonl` via temp file + atomic rename
6. rebuild views via temp file + atomic rename
7. release the lock

### 14.3 Stale lock behavior

If a lock exists:

- if the owning process is still alive, fail with a clear coordination error
- if the owning process is dead, the lock is stale
- stale lock cleanup is allowed through `pulse-work doctor --fix`
- normal mutation commands must not silently delete a lock they did not create

---

## 15. `pulse-work` CLI contract

### 15.1 Delivery form

`pulse-work` is the canonical runtime CLI name.

Implementation form in the plugin repo:

- canonical runtime source scripts live in `skills/workflow/scripts/runtime/`
- `skills/workflow/scripts/runtime/pulse_work.mjs` is the source entrypoint for the CLI
- workflow command-local scripts live under `skills/workflow/commands/<command>/scripts/`
- standalone utility scripts remain under their own skill directories as needed
- onboarding installs or syncs a repo-local executable surface under `.pulse/scripts/`
- v1 should ship a thin executable wrapper named `pulse-work` so the human-facing runtime command remains `pulse-work`

`pulse-work` is not the main conversational surface. The primary workflow surface remains `pulse:workflow`.

### 15.2 Minimum command set

Required v1 commands:

- `pulse-work create`
- `pulse-work show <id>`
- `pulse-work list`
- `pulse-work ready`
- `pulse-work update <id>`
- `pulse-work close <id>`
- `pulse-work reopen <id>`
- `pulse-work dep add <id> <depends-on>`
- `pulse-work dep rm <id> <depends-on>`
- `pulse-work children <id>`
- `pulse-work graph`
- `pulse-work doctor`

### 15.3 Output behavior

- default output is human-readable
- every command used by automation must support `--json`
- JSON output is the stable automation surface

### 15.4 Command semantics

#### `create`

Responsibilities:

- validate kind, parent, and title
- generate ID and slug
- derive canonical paths
- create item record
- create content files in `works/`
- rebuild views

Minimum supported create inputs:

- `--kind`
- `--title`
- `--parent`
- optional `--priority`
- optional `--owner`
- optional `--label`
- optional `--risk`

#### `show`

Returns one record plus useful derived state.

Derived response may include:

- readiness
- unresolved dependencies
- children
- reverse dependencies
- resolved content paths

#### `list`

Lists items with filters.

Minimum filters:

- `--kind`
- `--status`
- `--epic`
- `--parent`
- `--owner`
- `--label`

#### `ready`

Returns items where:

- `status = OPEN`
- `blocked_reason = null`
- every dependency item is `CLOSED`

#### `update`

Supports metadata updates and safe content-path updates.

Minimum update fields:

- `--title`
- `--slug`
- `--status`
- `--priority`
- `--owner`
- `--add-label`
- `--rm-label`
- `--add-risk`
- `--rm-risk`
- `--blocked-reason`

Rules:

- `CLOSED` cannot be set directly through generic status update
- renames that change paths must update item records transactionally

#### `close`

Closes one item.

Rules:

- enforces child-close rules
- enforces verification rules for `TASK` and `BUG`
- sets `closed_at`
- rebuilds views

#### `reopen`

- only supported path out of `CLOSED`
- resets status to `OPEN`
- clears `closed_at`

#### `dep add` / `dep rm`

- mutate dependency edges
- validate target existence
- block cycles on add
- rebuild views

#### `children`

- returns direct children by parent ID
- supports `--json`

#### `graph`

Returns a materialized graph view.

Minimum JSON shape:

```json
{
  "nodes": [],
  "edges": {
    "hierarchy": [],
    "dependencies": []
  }
}
```

#### `doctor`

Must detect:

- schema violations
- duplicate IDs
- broken parent references
- inconsistent `epic_id`
- missing dependency IDs
- dependency cycles
- broken `content_path`
- broken `verification_path`
- manual move / rename drift
- stale or missing generated views
- stale lock file
- markdown frontmatter metadata leaks beyond `id`

Safe fixes allowed in `doctor --fix`:

- rebuild views
- normalize deterministic ordering of `items.jsonl`
- recreate missing directories or empty template files when the path is expected and no unrelated file would be overwritten
- remove stale lock when no owning process exists

Safe fixes not allowed:

- close or reopen items
- mutate lifecycle metadata to hide issues
- overwrite human-authored content
- infer missing semantic data without user intent

---

## 16. Runtime state model

### 16.1 Canonical runtime paths

Canonical runtime files live under `.pulse/runtime/`:

- `.pulse/runtime/tooling-status.json`
- `.pulse/runtime/state.json`
- `.pulse/runtime/STATE.md`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/checkpoints/`
- `.pulse/runtime/reservations.json`

### 16.2 Simplification rules

Pulse v2 reduces runtime artifact duplication.

Persisted runtime files are:

- machine state: `state.json`
- human state: `STATE.md`
- handoffs
- checkpoints
- reservations

Pulse v2 does **not** persist separate derived mirrors such as:

- `current-feature.json`
- `runtime-snapshot.json`

Those may be derived in memory by `pulse_status` when needed.

### 16.3 `state.json` expectations

`state.json` should preserve the current gating model while replacing the old workflow-skill assumptions with router-aware fields.

Minimum fields:

- `schema_version`
- `phase`
- `active_command`
- `active_epic_id`
- `active_story_id`
- `active_item_id`
- `gate`
- `gate_status`
- `requested_mode`
- `recommended_mode`
- `next_action`
- `next_command_recommended`
- `handoff_manifest`
- `last_updated`

If a transitional adapter still carries an `active_skill` field, it must always resolve to `pulse:workflow` for workflow phases and must not reintroduce multi-skill workflow routing logic.

### 16.4 Reservations

Canonical runtime reservation file:

```text
.pulse/runtime/reservations.json
```

Reservation semantics:

- short-lived execution lease only
- used to prevent double-work
- does not change item identity
- does not replace durable `owner`

---

## 17. Harness references and templates

### 17.1 Canonical source locations

The plugin source repo must contain:

- `skills/workflow/references/HARNESS.md`
- `skills/workflow/templates/HARNESS_BACKLOG.md`

### 17.2 Runtime materialization rule

Onboarding must materialize:

- `.pulse/harness/HARNESS_BACKLOG.md`

Onboarding must **not** create a second canonical `HARNESS.md` inside `.pulse/harness/`.

The harness operating contract belongs to the workflow reference tree, while the runtime backlog belongs to the installed runtime plane.

### 17.3 Backlog entry shape

Minimum backlog entry shape:

- title
- discovered while
- current pain
- suggested improvement
- risk
- status

### 17.4 High-level docs relationship

Repo-root docs such as `AGENTS.md` remain the high-level operator entrypoint and should point to the workflow router contract and harness reference rather than duplicating the full runtime contract.

---

## 18. Onboarding and script-source rules

### 18.1 Canonical script sources

Because this repo is both the Pulse source repo and a self-hosted testbed, script duplication must be controlled.

Rules:

- `skills/workflow/scripts/runtime/` is the canonical source for runtime and workgraph scripts
- `skills/workflow/scripts/lib/` is the canonical source for shared workflow router helpers
- `skills/workflow/commands/<command>/scripts/` is the canonical source for workflow-command-specific helpers and assets
- standalone public utility scripts stay under their own skill directories unless the runtime must invoke them directly
- `.pulse/scripts/` is the installed mirror in a self-hosted or downstream repo for runtime-facing executables and helpers
- command-local workflow scripts do not need a second mirror under `.pulse/scripts/` unless the repo-local runtime must invoke them directly
- if this repo keeps a checked-in `.pulse/scripts/` mirror for dogfooding, tests must ensure it stays synced from the canonical runtime source

### 18.2 `pulse:workflow onboard` authority

`pulse:workflow onboard` replaces the combined authority previously split across `preflight` and `using-pulse`.

It is responsible for bootstrap, readiness checks, and installing the local Pulse runtime surface.

### 18.3 Onboarding responsibilities

`skills/workflow/commands/onboard/scripts/onboard_pulse.mjs` must:

- create the v2 directory structure under `.pulse/`
- install `pulse-work` and helper scripts under `.pulse/scripts/`
- create `.pulse/workgraph/schema.json`
- create or initialize `.pulse/workgraph/items.jsonl`
- create `.pulse/runtime/*` files
- materialize `.pulse/harness/HARNESS_BACKLOG.md` from the template source
- move or remove legacy top-level runtime files
- respect the existing `.gitignore` policy of this plugin repo rather than treating `.gitignore` edits as a migration deliverable here
- stop requiring `br` and `bv` for a healthy v2 repo

### 18.4 Readiness behavior

Pulse v2 no longer has a standalone packaged `preflight` surface.

Readiness and bootstrap logic must:

- stop treating `br` and `bv` as core prerequisites
- treat Node and the repo-local Pulse runtime as the core prerequisites
- continue reporting Git and other truly required dependencies
- surface legacy artifacts such as `.beads/` or `history/` as migration warnings instead of runtime blockers

---

## 19. Docs, hooks, eval, and manifest rewiring

Pulse v2 implementation must rewire the public and internal contracts that currently point at the legacy system.

### 19.1 Required updates

These surfaces must be updated to reference `pulse:workflow`, standalone utility skills, `pulse-work`, `.pulse/workgraph/`, and `.pulse/runtime/` instead of legacy workflow skills, `br`, `bv`, and `.beads/`:

- `AGENTS.md`
- `AGENTS.template.md`
- `CLAUDE.md`
- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/examples/golden-path.md`
- `CONTRIBUTING.md`
- `hooks/*`
- `.codex/hooks/*`
- `scripts/pulse-plugin-eval.mjs`
- `.plugin-eval/benchmark.json`
- `pulse-eval-workspace/evals.json`
- plugin manifests and marketplace metadata

### 19.2 Replacement behavior

Current behavior should map as follows:

| Legacy | Pulse v2 replacement |
| --- | --- |
| `pulse:preflight` | `pulse:workflow onboard` |
| `pulse:using-pulse` | `pulse:workflow onboard` |
| `pulse:exploring` | `pulse:workflow explore` |
| `pulse:planning` | `pulse:workflow plan` |
| `pulse:validating` | `pulse:workflow validate` |
| `pulse:swarming` | `pulse:workflow swarm` |
| `pulse:executing` | `pulse:workflow execute` |
| `pulse:reviewing` | `pulse:workflow review` |
| `pulse:compounding` | `pulse:workflow compound` |
| `pulse:brainstorming` | `pulse:workflow brainstorm` |
| `pulse:architecture-rescue` | `pulse:architecture-rescue` |
| `pulse:systematic-debug-fix` | `pulse:systematic-debug-fix` |
| `pulse:dev-note` | `pulse:dev-note` |
| `pulse:dev-note-distil` | `pulse:dev-note-distil` |
| `pulse:dream` | removed |
| `br ready` | `pulse-work ready` |
| `br show <id>` | `pulse-work show <id>` |
| `br create/update/close` | `pulse-work create/update/close` |
| `bv --robot-triage` | `pulse-work ready --json` or `pulse-work graph --json` depending on need |
| `.beads/` truth | `.pulse/workgraph/items.jsonl` |
| `history/<feature>/...` active work | `works/epics/**` + `.pulse/runtime/*` |

### 19.3 Manifest and metadata rules

Plugin manifests and marketplace metadata must describe Pulse as:

- one workflow router skill: `pulse:workflow`
- a small set of standalone public utility skills
- a repo-local runtime CLI: `pulse-work`

They must **not** describe Pulse as a bundle of many workflow-phase skills, blur the boundary between the workflow router and standalone skills, or keep removed legacy utilities in the shipped public surface.

Structured workflow command metadata must live in `skills/workflow/scripts/command-metadata.json`, not in a second repo-root catalog file.

---

## 20. Migration strategy

Pulse v2 migration must happen in phases.

### Phase 0 — architecture baseline and repo cleanup

Deliver:

- updated `PLAN.md` and `SPEC.md`
- plugin-vs-target-repo boundary clarified, especially around `.pulse/` and `.gitignore`
- plugin manifests aligned with workflow-router + standalone-utility architecture
- explicit removal of `skill-catalog.json` from the target design
- explicit classification of workflow router, standalone public utilities, and removed legacy utilities
- explicit lock that runtime source lives under `skills/workflow/scripts/runtime/`

### Phase 1 — workflow router `pulse:workflow`

Deliver:

- `skills/workflow/SKILL.md`
- workflow command modules under `skills/workflow/commands/<command>/`
- shared references under `skills/workflow/references/shared/`
- `skills/workflow/references/HARNESS.md`
- `skills/workflow/templates/HARNESS_BACKLOG.md`
- `skills/workflow/scripts/command-metadata.json`

### Phase 2 — `pulse-work` engine v1

Deliver:

- `pulse-work` CLI implementation under `skills/workflow/scripts/runtime/`
- `.pulse/workgraph/items.jsonl`
- `.pulse/workgraph/schema.json`
- lock handling
- derived views
- core validator
- content scaffolding into `works/`

### Phase 3 — runtime relocation and onboarding

Deliver:

- `.pulse/runtime/*` canonical paths
- onboarding changes under `skills/workflow/commands/onboard/scripts/onboard_pulse.mjs`
- `pulse_state`, `pulse_status`, `pulse_session_context`, and `pulse_reservations` moved under `skills/workflow/scripts/runtime/`
- removal of `current-feature.json` and `runtime-snapshot.json`
- `pulse:workflow onboard` replacing the old bootstrap authority

### Phase 4 — collapse legacy workflow skill surface

Deliver:

- migration of legacy workflow skill content into `skills/workflow/commands/`
- update of standalone utility skills so they remain outside the workflow router
- removal of legacy packaged workflow skills
- removal of `dream`
- removal of `skill-catalog.json`
- retention of `prompt-leverage` and `gitnexus` as standalone public skills
- removal of `refresh-project-docs` and `writing-pulse-skills` from the packaged public surface

### Phase 5 — docs, hooks, evals, and tests

Deliver:

- removal of `bv`-specific hook assumptions
- session-start and readiness language updated to `pulse:workflow onboard`
- eval corpus updated to `pulse:workflow <command>` for workflow paths
- standalone-utility docs/tests updated to their direct invocation surfaces
- router, runtime, workgraph, and harness backlog materialization tests

### Phase 6 — migration docs, cleanup, and final audit

Deliver:

- final docs pass across `README.md`, `docs/ARCHITECTURE.md`, and examples
- manual migration blueprint for brownfield repos
- final repo-wide audit for forbidden legacy assumptions

### 20.1 Brownfield migration safety

Before restructuring legacy docs in a brownfield repo:

- create a backup under `.pulse/migrations/docs-backups/<timestamp-or-migration-id>/`
- include a short manifest explaining when and why the snapshot was taken
- never treat backup content as current truth after migration

### 20.2 Legacy import policy

Do not start with a one-shot automatic migration script.

The first required migration artifact is a manual blueprint that explains how to map:

- `.beads/` items → `.pulse/workgraph/items.jsonl`
- `history/<feature>/...` → `works/epics/**`
- legacy verification artifacts → task / bug `verification.md`
- story closeout → `lifecycle-summary.md`

Automation can follow after the mapping rules are stable.

---

## 21. Testing strategy

Pulse v2 must ship with tests for both the standalone workgraph engine and the public-surface split.

### 21.1 Workflow router coverage

Required workflow router tests:

- `pulse:workflow` with no args renders the workflow command menu correctly
- `pulse:workflow onboard` routes to the correct command reference
- `pulse:workflow explore` routes correctly
- `pulse:workflow brainstorm` routes correctly
- `pulse:workflow plan` routes correctly
- `pulse:workflow validate` routes correctly
- `pulse:workflow execute` routes correctly
- `pulse:workflow swarm` routes correctly
- `pulse:workflow review` routes correctly
- `pulse:workflow compound` routes correctly
- unknown first token falls back to supported help behavior

### 21.2 Standalone utility coverage

Required public-surface tests:

- `pulse:architecture-rescue` remains packaged as a standalone skill, not a workflow command
- `pulse:systematic-debug-fix` remains packaged as a standalone skill, not a workflow command
- `pulse:dev-note` remains packaged as a standalone skill, not a workflow command
- `pulse:dev-note-distil` remains packaged as a standalone skill, not a workflow command
- `pulse:prompt-leverage` remains packaged as a standalone skill, not a workflow command
- `pulse:gitnexus` remains packaged as a standalone skill, not a workflow command
- `refresh-project-docs` and `writing-pulse-skills` are not packaged as public skills

### 21.3 Unit coverage

Required unit tests:

- ID generation and collision suffixing
- slug sanitization
- path derivation
- hierarchy validation
- dependency cycle detection
- status transition rules
- close / reopen rules
- verification heading checks
- unique prefix resolution
- lock-file parsing and stale-lock detection

### 21.4 Integration coverage

Required integration tests:

- bootstrap repo with `pulse:workflow onboard`
- onboarding creates `.pulse/runtime/*`, `.pulse/workgraph/*`, and `.pulse/harness/HARNESS_BACKLOG.md`
- create epic / story / task / bug
- task / bug creation only under a story
- dependency add / remove
- ready list sorting and filtering
- close blocked by open child
- close blocked by missing verification
- reopen resets `closed_at`
- parent / story slug rename cascades descendant paths
- doctor detects manual path drift
- doctor rebuilds views without assuming this plugin repo rewrites `.gitignore`
- `pulse_status` works in a repo with only v2 artifacts and no `history/` or `.beads/`

### 21.5 Golden fixtures

Required golden fixtures:

- `items.jsonl` ordering
- `active.json`
- `closed.json`
- `ready.json`
- `graph.json`
- generated markdown templates

### 21.6 Repo audit coverage

Final audits must grep for and fail on unexpected active references to:

- `pulse:preflight`
- `pulse:using-pulse`
- `pulse:dream`
- `skill-catalog.json`
- `br`
- `bv`
- `.beads`
- `history/`
- `.pulse/current-feature.json`
- `.pulse/runtime-snapshot.json`
- top-level `.pulse/reservations.json`

---

## 22. Acceptance criteria

Pulse v2 v1 is acceptable only when all of the following are true.

### 22.1 Public surface

- the only packaged workflow router skill is `pulse:workflow`
- all supported workflow phases route through `pulse:workflow <command>`
- `architecture-rescue`, `systematic-debug-fix`, `dev-note`, `dev-note-distil`, `prompt-leverage`, and `gitnexus` remain standalone public utility skills outside the workflow router
- `preflight`, `bootstrap-project-context`, `dream`, `refresh-project-docs`, and `writing-pulse-skills` are not shipped as packaged public surfaces
- `skill-catalog.json` does not exist

### 22.2 Core workgraph

- items can be created, updated, closed, reopened, and linked by dependency
- `items.jsonl` is the only writable metadata truth
- cycles are blocked on write
- `ready` output matches the spec exactly
- generated views rebuild deterministically

### 22.3 Content routing

- human-facing work content is created under `works/`
- story `README.md` remains the story description, while story `SPEC.md` is the brainstorming output artifact
- epics contain stories, and tasks / bugs live only under story directories
- markdown contains only `id` frontmatter for identity
- `TASK` and `BUG` close is impossible without valid verification evidence

### 22.4 Runtime plane

- runtime state lives under `.pulse/runtime/`
- no `current-feature.json` or `runtime-snapshot.json` persistence remains
- reservations are runtime-only and separate from canonical item metadata

### 22.5 Harness source / runtime split

- `HARNESS.md` lives at `skills/workflow/references/HARNESS.md`
- `HARNESS_BACKLOG.md` lives at `skills/workflow/templates/HARNESS_BACKLOG.md`
- onboarding materializes `.pulse/harness/HARNESS_BACKLOG.md`
- the runtime does not rely on a second canonical `.pulse/harness/HARNESS.md`

### 22.6 Dependency removal

- a greenfield Pulse v2 repo can operate without `br`, `bv`, or `.beads/`
- readiness no longer marks missing `br` / `bv` as blocking for v2
- hooks no longer depend on `bv`-specific behavior

### 22.7 Migration safety

- brownfield docs are backed up before restructure
- legacy drift is surfaced by `doctor`
- v2 writers do not silently update legacy sources

### 22.8 Repo self-hosting

- canonical runtime source scripts live in `skills/workflow/scripts/runtime/`
- workflow command-specific helper scripts live in `skills/workflow/commands/<command>/scripts/`
- installed runtime scripts under `.pulse/scripts/` have a defined source-of-truth relationship to the canonical runtime source
- onboarding tests cover the self-hosted v2 installed layout

---

## 23. Immediate implementation order in this repo

Recommended implementation order:

1. update `SPEC.md` and manifests for the workflow-router + standalone-utility architecture, and clarify plugin-repo vs target-repo scope
2. create `skills/workflow/SKILL.md`, workflow command module directories, shared references, and `command-metadata.json`
3. implement `pulse-work` core create / show / list / ready / update / close / reopen / dep / doctor flow under `skills/workflow/scripts/runtime/`
4. implement lock handling, atomic writes, and view rebuilds
5. implement `works/` scaffolding and markdown templates
6. move runtime state to `.pulse/runtime/` and collapse duplicated runtime mirrors
7. update onboarding and readiness into `pulse:workflow onboard`
8. update standalone utility surfaces so they remain separate from the workflow router, with `prompt-leverage` and `gitnexus` retained while `bootstrap-project-context`, `refresh-project-docs`, and `writing-pulse-skills` are removed
9. rewire docs, hooks, examples, manifests, and evals away from the legacy workflow-skill surface
10. write the manual migration blueprint
11. add router, golden, unit, and integration tests, then remove remaining legacy assumptions

---

## 24. Final locked decisions captured by this spec

This spec locks these implementation decisions for Pulse v2 v1:

- `pulse:workflow` is the only packaged workflow router surface
- `pulse:architecture-rescue`, `pulse:systematic-debug-fix`, `pulse:dev-note`, `pulse:dev-note-distil`, `pulse:prompt-leverage`, and `pulse:gitnexus` remain standalone public utility skills
- `pulse-work` is the runtime CLI name
- `skills/workflow/SKILL.md` is the workflow router source of truth
- workflow-command behavior lives under `skills/workflow/commands/<command>/`
- `skills/workflow/scripts/command-metadata.json` is the structured workflow command metadata source of truth
- canonical runtime source scripts live under `skills/workflow/scripts/runtime/`
- `items.jsonl` is the only canonical writable metadata source
- generated views are derived and local-only; in this plugin repo they remain under ignored `.pulse/` state
- `work item` is the human-facing generic noun
- `owner` is durable metadata and reservations track the active execution lease
- no `assignee` field exists in canonical v1 item metadata
- `README.md` is the canonical item entry file
- story directories may also contain `SPEC.md` as the brainstorming output, while story `README.md` remains the story description
- tasks and bugs live only under story directories, never directly under an epic
- task / bug close requires verification evidence
- runtime state moves under `.pulse/runtime/`
- `current-feature.json` and `runtime-snapshot.json` are removed as persisted artifacts
- `HARNESS.md` is a workflow reference source, not a runtime template
- `HARNESS_BACKLOG.md` is a template source materialized into `.pulse/harness/`
- `preflight`, `dream`, and `skill-catalog.json` are removed from the target architecture
- `prompt-leverage` and `gitnexus` remain standalone public skills outside the workflow router
- `bootstrap-project-context`, `refresh-project-docs`, and `writing-pulse-skills` are removed from the target packaged surface
- Pulse v2 remains zero-service and repo-local

This document is the baseline for implementation work.
