# Pulse Runtime and Workflow Script Refactor Proposal

## Summary

Pulse should be redesigned around one clear architectural boundary:

```text
Pulse = router skill + plugin-owned runtime scripts + repo-local data plane
```

The packaged workflow skill owns routing, command references, and executable runtime logic. Target repositories own only Pulse data and artifacts: `.pulse/`, `works/`, `docs/`, and `AGENTS.md`.

The previous repo-local runtime-kernel model copied canonical scripts into `.pulse/scripts/`. That solved one problem, making commands easy to run from a target repo, but created a worse one: copied scripts become stale when the plugin updates. It also mixed executable code into `.pulse/`, which should be a data/control plane.

The target model is closer to Impeccable:

```text
script location != target repo
script location = skill/plugin package
repo root       = --repo-root || git root from process.cwd() || process.cwd()
```

`pulse:workflow use` remains the idempotent entrypoint: repair/onboard only when needed, then load session context and recommend the next command.

## Core Product Contract

`pulse:workflow use` is the normal Pulse session entrypoint.

It should do exactly this:

```text
pulse:workflow use
  Phase A: check readiness
  Phase B: repair/materialize repo data if needed
  Phase C: load session context
  Phase D: recommend the next workflow command
```

In Vietnamese terms:

```text
/pulse:workflow use là entrypoint chuẩn để bắt đầu hoặc resume một Pulse session.

Nếu repo chưa onboard hoặc data layout bị thiếu/stale, nó onboard/repair trước.
Nếu repo đã onboard đúng, nó không cài lại.
Sau đó nó luôn load context hiện tại để agent biết đang ở đâu, cần đọc gì trước,
có handoff/reservation nào không, và command tiếp theo là gì.
```

`use` is not “always install Pulse”. It is also not “only read status”. It is:

```text
ensure Pulse repo data plane is ready + load current Pulse session
```

## Inspiration From Impeccable

The Impeccable skill under `references/impeccable/skill/` provides the design pattern Pulse should follow.

```text
skill/SKILL.md             = router, shared rules, command table
skill/reference/<command>/ = command-specific reference bundle
skill/scripts/*.mjs        = skill-owned executable utilities
project cwd                = target project/repo
```

Important lessons:

1. **One user-facing router skill** keeps the command surface clean.
2. **Each command owns a reference bundle**, with `command.md` as the entrypoint and supporting references beside it when needed.
3. **Scripts stay in the skill package** and update with the plugin.
4. **Target project state is resolved from `process.cwd()` or explicit override**, not from script location.
5. **Local durable state is not skill source code**.

Impeccable source references use:

```bash
node {{scripts_path}}/load-context.mjs
```

`{{scripts_path}}` is not a Node.js feature. It is a build/provider placeholder for the skill scripts directory. After skill build/install, it becomes a real path to the installed skill’s `scripts/` directory.

Inside the script, Impeccable uses:

```js
loadContext(process.cwd())
```

So the final separation is:

```text
{{scripts_path}} = where the skill's scripts live
process.cwd()    = target project root
```

Pulse should adopt this pattern, with an explicit `--repo-root` option because Pulse performs more mutations than Impeccable.

## Target Architecture

Pulse should be split into five explicit planes.

```text
1. Skill Plane
2. Plugin Runtime Script Plane
3. Installer/Onboarding Plane
4. Repo Data Plane
5. Optional Compatibility Shim Plane
```

## 1. Skill Plane

The skill plane is packaged with the plugin and is responsible for routing and instruction loading.

```text
skills/workflow/
  SKILL.md
  references/
    use/
      command.md
      readiness.md
      handoff-contract.md
      migration-warnings.md
      pressure-scenarios.md
    explore/command.md
    plan/command.md
    validate/command.md
    execute/command.md
    swarm/command.md
    review/command.md
    compound/command.md
    shared/
  scripts/
    command-metadata.json
    onboard_pulse.mjs
    pulse_work.mjs
    pulse_status.mjs
    pulse_state.mjs
    pulse_session_load.mjs
    pulse_reservations.mjs
    workgraph_*.mjs
  templates/
```

Responsibilities:

- route `pulse:workflow <command>`
- load the correct command `command.md` entrypoint
- load command-local supporting references only when the entrypoint requires them
- explain human gates
- call plugin-owned runtime scripts when needed
- avoid duplicating readiness/session logic inside command docs

`skills/workflow/SKILL.md` should resemble Impeccable’s router structure:

```markdown
## Setup

Before any Pulse command:
1. If command is `use`, run the use reference directly.
2. For all other commands, ensure runtime context is loaded first.
3. If `.pulse/runtime/onboarding.json` is missing or stale, route to `pulse:workflow use`.
4. Run plugin-owned status/session scripts via `{{scripts_path}}`, not `.pulse/scripts/`.
5. Load the invoked command's `command.md` entrypoint, then load command-local supporting references as directed by that entrypoint.

## Commands

| Command | Purpose | Entrypoint |
| --- | --- | --- |
| `use` | Ensure repo data plane exists, then load session | references/use/command.md |
| `explore` | Build decision context, stop at Gate 1 | references/explore/command.md |
| `plan` | Shape approved work, stop at Gate 2 | references/plan/command.md |
| `validate` | Validate feasibility, stop at Gate 3 | references/validate/command.md |
| `execute` | Single-worker implementation | references/execute/command.md |
| `swarm` | Multi-agent implementation | references/swarm/command.md |
| `review` | Review and stop at Gate 4 | references/review/command.md |
| `compound` | Promote learnings and durable memory | references/compound/command.md |
```

Each command directory should treat `command.md` as the public entrypoint. Supporting files such as readiness contracts, pressure scenarios, artifact contracts, handoff contracts, or migration notes should live beside it and be loaded only when the entrypoint says they are relevant.

The `command.md` entrypoint should clearly define:

- command mission
- input/runtime preconditions
- command-local supporting references and when to read them
- files/artifacts it reads
- files/artifacts it may mutate
- required human gate
- output contract
- fallback route when context is missing

## 2. Plugin Runtime Script Plane

The plugin runtime script plane is executable code that stays inside the workflow skill package.

```text
skills/workflow/scripts/
  command-metadata.json
  onboard_pulse.mjs
  pulse_work.mjs
  pulse_status.mjs
  pulse_state.mjs
  pulse_session_load.mjs
  pulse_reservations.mjs
  workgraph_*.mjs
  ...
```

Scripts should be flat under `skills/workflow/scripts/`. The script set is small enough that `runtime/` and `onboard/` subdirectories add ceremony without meaningful separation. Responsibility should be expressed by file names and module boundaries, not directory hierarchy.

Canonical runtime logic should not be copied into `.pulse/scripts/` by default.

Each script should start with an Impeccable-style header docstring. The goal is not generic documentation; it is to make the script's operational contract obvious before any code is read.

Required header content:

```text
- one-line script purpose
- the Pulse command or runtime flow that calls it
- repo-local files it reads/writes
- CLI entrypoints and arguments
- what the script intentionally does not own
- path-resolution rule: --repo-root/PULSE_REPO_ROOT/git root/cwd
```

Example shape:

```js
#!/usr/bin/env node
/**
 * Pulse session-load helper.
 *
 * /pulse:workflow use calls this after readiness is established to restore
 * the current Pulse session from repo-local runtime pointers.
 *
 * Reads:
 *   .pulse/runtime/state.json
 *   .pulse/runtime/tooling-status.json
 *   .pulse/runtime/handoffs/manifest.json
 *   .pulse/runtime/reservations.json
 *   .pulse/workgraph/items.jsonl
 *
 * Writes nothing. The loader reports posture, read-first paths, selected
 * handoff, conflicts, and the recommended next workflow command. It never
 * executes that next command.
 *
 * CLI entry points:
 *   node pulse_session_load.mjs [--repo-root <repo>] [--resume-owner <owner>] [--json]
 *
 * Repo root resolution:
 *   --repo-root, then PULSE_REPO_ROOT, then git root from process.cwd(), then process.cwd().
 *
 * Note: this helper is intentionally pointer-driven. It must not recursively
 * scan works/, docs/, .pulse/memory/, or .pulse/runtime/.
 */
```

This mirrors Impeccable's script style: the header records why the helper exists, how skill instructions call it, which artifacts it touches, and which responsibilities are deliberately outside its scope.

Important exported functions and non-obvious internal functions should also have short header docstrings.

Function docstring rules:

```text
- document why the function exists, not obvious syntax
- name the invariant or contract it enforces
- mention filesystem side effects when present
- mention safety boundaries for path validation, locks, backups, or migrations
- keep it short; avoid tutorial-style comments
```

Example shape:

```js
/**
 * Resolve the target repository for a Pulse runtime operation.
 *
 * The script path belongs to the installed skill package; the repo root is the
 * project being operated on. Prefer explicit input for automation, then fall
 * back to the agent's current git checkout.
 */
export function resolveRepoRoot({ explicitRoot, env = process.env, cwd = process.cwd() } = {}) {
  ...
}
```

Inline comments inside function bodies should be used only when they explain a non-obvious constraint, not what the next line does. Good examples are path traversal rejection, lock ownership rules, brownfield backup ordering, or why generated workgraph views must not flow back into `items.jsonl`.

Every runtime script should support this root resolution contract:

```text
repoRoot = --repo-root || PULSE_REPO_ROOT || git root from process.cwd() || process.cwd()
```

Meaning:

```text
agent standing in target repo:
  node {{scripts_path}}/pulse_status.mjs --json

agent standing elsewhere:
  node {{scripts_path}}/pulse_status.mjs --repo-root /path/to/repo --json
```

The script path and repo path are separate concerns:

```text
{{scripts_path}} = provider-aware path to the installed workflow skill's scripts directory
repoRoot         = target repository being operated on
```

This preserves plugin update behavior: when the plugin updates, agents immediately use the updated scripts.

## 3. Installer/Onboarding Plane

The installer/onboarding plane is skill-local and plugin-aware.

Current file:

```text
skills/workflow/scripts/onboard_pulse.mjs
```

Target role:

```text
skill-owned readiness checker and repo data materializer
```

It should not be copied into `.pulse/scripts/` because it depends on packaged source paths such as:

```text
skills/workflow/templates/
skills/workflow/scripts/
AGENTS.template.md
plugin manifests
```

The installer should own:

- Node/runtime checks
- repo root resolution for install operations
- `.pulse/`, `docs/`, and `works/` domain classification
- brownfield backup and migration briefs
- materializing repo-local data files
- materializing `.pulse/harness/HARNESS_BACKLOG.md`
- initializing `.pulse/workgraph/`
- writing `.pulse/runtime/tooling-status.json`
- writing `.pulse/runtime/state.json`
- writing `.pulse/runtime/STATE.md`
- writing `.pulse/runtime/onboarding.json`
- rebuilding generated workgraph views
- session-load handoff into runtime status

It should not own:

- canonical runtime command logic
- long-term status rendering
- reservation mutation internals
- workgraph mutation internals beyond initialization/repair
- copied runtime script lifecycle as a readiness blocker

It should expose two public modes:

```text
check = inspect readiness without mutation
apply = repair/materialize repo data plane
```

Example output:

```json
{
  "status": "PASS",
  "action": "check",
  "applied": false,
  "blockers": [],
  "warnings": [],
  "next": "session_load"
}
```

When repair is needed:

```json
{
  "status": "PASS",
  "action": "apply",
  "applied": true,
  "materialized": [".pulse/runtime/state.json", ".pulse/workgraph/items.jsonl"],
  "repaired": [".pulse/runtime/STATE.md"],
  "warnings": [],
  "next": "session_load"
}
```

## 4. Repo Data Plane

The repo data plane is mutable target-repo state.

```text
.pulse/runtime/
  onboarding.json
  tooling-status.json
  state.json
  STATE.md
  handoffs/manifest.json
  reservations.json
  onboarding-migration/

.pulse/workgraph/
  schema.json
  items.jsonl
  views/
    active.json
    closed.json
    ready.json
    graph.json

.pulse/harness/
  HARNESS_BACKLOG.md

.pulse/memory/
  critical-patterns.md
  learnings/
  corrections/
  ratchet/

works/
docs/
AGENTS.md
```

Ownership rules:

```text
.pulse/runtime/*      = session/runtime mirror and leases
.pulse/workgraph/*    = canonical work metadata plus derived views
.pulse/harness/*      = materialized harness backlog only
.pulse/memory/*       = durable reusable Pulse memory output
works/*               = human-facing work content
docs/*                = project knowledge
AGENTS.md             = operator workflow rules
```

`.pulse/` should be data-only by default. Runtime code does not belong there as a canonical source of truth.

## 5. Optional Compatibility Shim Plane

`.pulse/scripts/` may exist only as a compatibility shim plane, not as the canonical runtime.

Allowed use:

```text
.pulse/scripts/pulse_status.mjs      = tiny shim that forwards to plugin runtime when possible
.pulse/scripts/pulse-work            = tiny shim for legacy operator muscle memory
```

Rules:

```text
1. Shims are optional, not required readiness files.
2. Shims must not contain canonical runtime logic.
3. Missing shims must not block a greenfield v2 repo.
4. Stale shims should be warnings, not blockers, unless they are being used as current truth.
5. Readiness must never depend on copied full runtime scripts.
```

If the project chooses to keep no shims, that is the cleanest model.

## Current Problems

## Problem 1: `.pulse/scripts/` creates stale runtime code

Copying full scripts into target repos means:

```text
plugin update -> skill scripts update
existing target repo -> copied .pulse/scripts stay old
```

This creates version skew between the installed plugin and the target repo. It also forces onboarding to become a script installer/updater instead of a repo data materializer.

Target fix:

```text
Canonical scripts stay under skills/workflow/scripts/*.mjs.
.pulse/scripts/ is removed from required readiness.
If kept, .pulse/scripts/ contains only optional shims.
```

## Problem 2: `onboard_pulse.mjs` is doing too much

Current file:

```text
skills/workflow/scripts/onboard_pulse.mjs
```

It currently handles:

- runtime checks
- repo root resolution
- readiness payload construction
- state markdown rendering
- domain classification
- brownfield backup
- migration brief writing
- AGENTS block management
- support script copying
- workgraph initialization
- onboarding check/apply APIs
- CLI argument parsing

This is acceptable only if it remains a skill-owned installer. It becomes problematic when copied or when it owns runtime domains that should be separate.

Target fix:

```text
onboard_pulse.mjs = readiness + data materialization + repair orchestration
runtime modules   = session load, status, reservations, workgraph mutation
```

## Problem 3: `load_context.mjs` is in the wrong layer

Current file:

```text
skills/workflow/scripts/load_context.mjs
```

It is not onboarding logic. It is session loading logic.

It reads:

- `.pulse/runtime/state.json`
- `.pulse/runtime/tooling-status.json`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/reservations.json`
- `.pulse/workgraph/items.jsonl`

It computes:

- session posture
- selected handoff
- resume options
- active work item IDs
- read-first paths
- missing/rejected paths
- next command

Target rename:

```text
skills/workflow/scripts/pulse_session_load.mjs
```

It should remain skill-owned and run against a target repo by `--repo-root` or `process.cwd()`.

## Problem 4: `pulse_state.mjs` is a status engine, not just state

Current file:

```text
skills/workflow/scripts/pulse_state.mjs
```

It currently combines:

- state schema/defaults
- path registry
- GitNexus readiness
- project-doc summary
- reservation summary
- gate/next-command inference
- runtime artifact derivation
- memory recall ranking
- memory hygiene warnings
- history lifecycle summary
- handoff rendering
- status object construction
- human-readable status rendering

This should be split. The name `pulse_state.mjs` should mean state primitives only.

## Problem 5: Reservation logic is duplicated

Current files:

```text
skills/workflow/scripts/pulse_state.mjs
skills/workflow/scripts/pulse_reservations.mjs
```

`pulse_reservations.mjs` has the authoritative reservation CLI and locking/mutation behavior. `pulse_state.mjs` also carries its own reservation store normalization and summary logic.

Target:

```text
pulse_reservation_store.mjs = store, locking, normalize, summarize, reserve/release/list/sweep
pulse_reservations.mjs     = CLI wrapper only
```

Then status code imports reservation summary from the shared store.

## Problem 6: Runtime snapshot/current feature persistence is unclear

The v2 runtime docs list canonical runtime files as:

```text
.pulse/runtime/tooling-status.json
.pulse/runtime/state.json
.pulse/runtime/STATE.md
.pulse/runtime/handoffs/manifest.json
.pulse/runtime/reservations.json
```

But existing code still has concepts like `current_feature` and `runtime_snapshot`.

Recommendation:

```text
current_feature and runtime_snapshot should be derived status sections, not persisted files,
unless they are explicitly added to the v2 runtime file contract.
```

Prefer derived sections for now. The canonical state should remain:

```text
.pulse/runtime/state.json
.pulse/runtime/STATE.md
.pulse/workgraph/items.jsonl
```

## Target Script Responsibilities

## `onboard_pulse.mjs`

Location:

```text
skills/workflow/scripts/onboard_pulse.mjs
```

Role:

```text
skill-owned readiness checker and repo data materializer
```

Responsibilities:

- readiness check
- apply repair/materialization
- write onboarding receipt
- initialize runtime/workgraph/harness data layout
- preserve brownfield docs/works/.pulse with backups
- rebuild generated workgraph views
- call session loader after readiness is established

Should not be copied to `.pulse/scripts/`.

## `pulse_paths.mjs`

Location:

```text
skills/workflow/scripts/pulse_paths.mjs
```

Role:

```text
shared path registry and repo-root resolver
```

Exports:

```js
resolveRepoRoot({ explicitRoot, env, cwd })
getPulsePaths(repoRoot)
relativePosix(repoRoot, filePath)
```

Root resolution:

```text
1. explicit `--repo-root`
2. `PULSE_REPO_ROOT`
3. `git rev-parse --show-toplevel` from process.cwd()
4. process.cwd()
```

This removes path registry responsibility from `pulse_state.mjs` and makes every runtime script consistent.

## `pulse_state.mjs`

Location:

```text
skills/workflow/scripts/pulse_state.mjs
```

Role:

```text
state primitives only
```

Exports:

```js
buildDefaultState()
normalizePulseState()
readPulseState()
writePulseState()
parseStateMarkdown()
writeStateMarkdown()
syncPulseRuntimeArtifacts()
```

Should not include:

- memory recall
- history lifecycle
- project docs detection
- GitNexus readiness
- handoff rendering
- full status rendering
- reservation mutation

## `pulse_session_load.mjs`

Location:

```text
skills/workflow/scripts/pulse_session_load.mjs
```

Role:

```text
session context loader
```

Renamed from:

```text
skills/workflow/scripts/load_context.mjs
```

Responsibilities:

- read runtime pointers
- select handoff when unambiguous or by owner
- validate safe read-first paths
- load active workgraph item context
- report posture
- compute recommended next command

Output shape:

```json
{
  "posture": "fresh|active|resumable|conflicted",
  "in_progress_items": 0,
  "open_reservations": 0,
  "requires_selection": false,
  "selected_handoff": null,
  "resume_options": [],
  "active_context": {
    "active_command": "pulse:workflow explore",
    "active_epic_id": null,
    "active_story_id": null,
    "active_item_id": null
  },
  "workgraph_items": [],
  "read_first": [],
  "missing_files": [],
  "rejected_paths": [],
  "conflicts": [],
  "summary": "No active Pulse session was found.",
  "next_action": "",
  "next_command": "pulse:workflow explore"
}
```

## `pulse_status_model.mjs`

Location:

```text
skills/workflow/scripts/pulse_status_model.mjs
```

Role:

```text
compose complete Pulse status JSON
```

Responsibilities:

- call `buildSessionLoad`
- summarize onboarding
- summarize tooling status
- summarize state
- summarize reservations
- summarize handoffs
- summarize project docs
- summarize history lifecycle
- summarize memory recall
- summarize GitNexus readiness
- build next reads
- build recommended actions

It should compose submodules, not implement every domain inline.

## `pulse_status_render.mjs`

Location:

```text
skills/workflow/scripts/pulse_status_render.mjs
```

Role:

```text
render status JSON into human-readable text
```

Exports:

```js
renderPulseStatus(status)
```

No file reads. No state mutation.

## `pulse_status.mjs`

Location:

```text
skills/workflow/scripts/pulse_status.mjs
```

Role:

```text
CLI wrapper for Pulse status
```

Responsibilities:

- parse CLI args
- resolve repo root using the shared resolver
- optional `--sync`
- call `readPulseStatus`
- call `renderPulseStatus` for text output
- print JSON for `--json`

Canonical invocation:

```bash
node {{scripts_path}}/pulse_status.mjs --json
node {{scripts_path}}/pulse_status.mjs --repo-root /path/to/repo --json
```

## `pulse_reservation_store.mjs`

Location:

```text
skills/workflow/scripts/pulse_reservation_store.mjs
```

Role:

```text
reservation storage and locking authority
```

Responsibilities:

- normalize reservation records
- read/write reservation store
- lock reservation file
- reserve paths
- release reservations
- list reservations
- sweep expired reservations
- summarize reservation status
- detect path overlaps/conflicts

## `pulse_reservations.mjs`

Location:

```text
skills/workflow/scripts/pulse_reservations.mjs
```

Role:

```text
CLI wrapper for reservations
```

Commands:

```text
reserve
release
list
sweep
```

It should import all core logic from `pulse_reservation_store.mjs` and resolve the target repo through the shared root resolver.

## Runtime Manifest

A copied-runtime manifest is no longer required for the default architecture.

Instead, if Pulse needs script identity reporting, it should report plugin runtime identity:

```json
{
  "schema_version": "1.0",
  "generated_at": "2026-05-20T00:00:00.000Z",
  "plugin_runtime": {
    "scripts_path": "<rendered skill scripts path>",
    "source": "workflow skill package",
    "version": "<plugin version when available>"
  },
  "repo_data_plane": {
    "onboarding_receipt": ".pulse/runtime/onboarding.json"
  }
}
```

If optional `.pulse/scripts/` shims exist, they may have their own compatibility receipt, but shim freshness must not be a baseline readiness blocker.

## Status Submodules

The following domains should be split out of `pulse_state.mjs` into narrow modules.

```text
pulse_project_docs.mjs
  summarizeProjectDocs(...)

pulse_memory_recall.mjs
  summarizeMemoryRecall(...)

pulse_handoffs.mjs
  summarizeHandoffManifest(...)
  renderHandoffSummary(...)
  renderResumeBriefing(...)
  renderTransferBlock(...)

pulse_history_lifecycle.mjs
  summarizeHistoryLifecycle(...)

pulse_gitnexus_readiness.mjs
  readGitNexusReadiness(...)

pulse_recommendations.mjs
  buildNextReads(...)
  buildRecommendedActions(...)
  normalizeNextCommandSurface(...)
```

These modules remain skill-owned runtime modules under `skills/workflow/scripts/`.

## Command Metadata

Add a command metadata source similar to Impeccable:

```text
skills/workflow/scripts/command-metadata.json
```

Example:

```json
{
  "use": {
    "description": "Ensure Pulse repo data plane exists, then load session context.",
    "category": "entrypoint",
    "next": ["explore"]
  },
  "explore": {
    "description": "Build decision context and stop at Gate 1.",
    "category": "discovery",
    "gate": "GATE 1",
    "next": ["plan"]
  },
  "plan": {
    "description": "Shape current work and stop at Gate 2.",
    "category": "planning",
    "gate": "GATE 2",
    "next": ["validate"]
  },
  "validate": {
    "description": "Validate feasibility and stop at Gate 3.",
    "category": "validation",
    "gate": "GATE 3",
    "next": ["execute", "swarm"]
  },
  "execute": {
    "description": "Run single-worker implementation against approved work.",
    "category": "execution",
    "next": ["review"]
  },
  "swarm": {
    "description": "Run coordinated multi-agent implementation against approved work.",
    "category": "execution",
    "next": ["review"]
  },
  "review": {
    "description": "Review execution output and stop at Gate 4.",
    "category": "review",
    "gate": "GATE 4",
    "next": ["compound"]
  },
  "compound": {
    "description": "Promote durable learnings, corrections, and lifecycle evidence.",
    "category": "memory",
    "next": []
  }
}
```

This can help keep router docs, next-command recommendations, and command menus aligned.

## `pulse:workflow use` Detailed Flow

## Phase A: Readiness Check

Run skill-owned installer in check mode:

```bash
node {{scripts_path}}/onboard_pulse.mjs --repo-root <repo>
```

If the agent is already standing in the target repo, `<repo>` can be omitted:

```bash
node {{scripts_path}}/onboard_pulse.mjs
```

Checks:

- Node version
- repo root
- plugin runtime script availability through rendered `{{scripts_path}}`
- onboarding receipt
- managed `AGENTS.md` block
- `.pulse/runtime/*` data files
- `.pulse/workgraph/schema.json`
- `.pulse/workgraph/items.jsonl`
- `.pulse/workgraph/views/*`
- `.pulse/harness/HARNESS_BACKLOG.md`
- `.pulse/`, `docs/`, and `works/` domain compliance
- optional shim warnings for `.pulse/scripts/*`, if present

No mutation in this phase.

## Phase B: Repair/Materialize If Needed

If check reports missing/stale repo data assets:

```bash
node {{scripts_path}}/onboard_pulse.mjs --repo-root <repo> --apply
```

Actions:

- backup non-compliant `.pulse/`, `docs/`, or `works/`
- rebuild v2 `.pulse/` data layout
- migrate known-safe runtime state/memory/workgraph data
- write migration briefs
- scaffold workgraph files/views
- scaffold harness backlog
- write tooling status
- write runtime state
- write human state mirror
- write onboarding receipt

It should not copy canonical runtime scripts into `.pulse/scripts/`.

## Phase C: Session Load

After readiness is established:

```bash
node {{scripts_path}}/pulse_session_load.mjs --repo-root <repo> --json
```

Or:

```bash
node {{scripts_path}}/pulse_status.mjs --repo-root <repo> --json
```

where `pulse_status` includes `session_load` in its status output.

If the agent is already standing in the target repo:

```bash
node {{scripts_path}}/pulse_status.mjs --json
```

Session load must be pointer-driven. It must not recursively scan all of `works/`, `docs/`, `.pulse/memory/`, or `.pulse/runtime/`.

Allowed read roots:

```text
AGENTS.md
.pulse/runtime/handoffs/
.pulse/memory/
works/
docs/
```

## Phase D: Next Command Recommendation

The session output should recommend the next workflow command:

```text
pulse:workflow explore
pulse:workflow plan
pulse:workflow validate
pulse:workflow execute
pulse:workflow swarm
pulse:workflow review
pulse:workflow compound
```

Recommendation should prefer explicit runtime state and handoff pointers, then fall back to safe defaults.

## Refactor Phases

## Phase 1: Fix the Code/Data Boundary

Goals:

- stop copying canonical runtime scripts into `.pulse/scripts/`
- keep `.pulse/` data-only by default
- introduce a shared repo-root resolver
- correct documentation around plugin-owned scripts

Actions:

1. Remove full runtime scripts from `MANAGED_SUPPORT_FILES`.
2. Remove `onboard_pulse.mjs` and `load_context.mjs` from any copied support surface.
3. Decide whether `.pulse/scripts/` shims are kept at all.
4. If shims are kept, make them tiny compatibility wrappers only.
5. Update readiness docs to remove required installed `.pulse/scripts/*`.
6. Update tests asserting copied files.

## Phase 2: Rename Session Load In the Flat Script Directory

Goals:

- make session loading a runtime concern, not onboarding
- keep session loading skill-owned, not copied
- keep the script directory flat

Actions:

1. Rename:

```text
skills/workflow/scripts/load_context.mjs
→ skills/workflow/scripts/pulse_session_load.mjs
```

2. Update `onboard_pulse.mjs` import:

```js
import { buildSessionLoad } from "./pulse_session_load.mjs";
```

3. Ensure `pulse_session_load.mjs` accepts `--repo-root` and otherwise uses shared root resolution.
4. Update status composition to consume `buildSessionLoad`.

## Phase 3: Split State From Status

Goals:

- make `pulse_state.mjs` a true state primitive module
- move status composition/rendering into separate modules

Actions:

1. Create `pulse_paths.mjs`.
2. Create `pulse_status_model.mjs`.
3. Create `pulse_status_render.mjs`.
4. Move `readPulseStatus` into `pulse_status_model.mjs`.
5. Move `renderPulseStatus` into `pulse_status_render.mjs`.
6. Update `pulse_status.mjs` imports.
7. Leave compatibility exports temporarily if tests require incremental migration.

## Phase 4: Establish Reservation Authority

Goals:

- one reservation store implementation
- no duplicate reservation normalization/summary logic

Actions:

1. Create `pulse_reservation_store.mjs`.
2. Move store read/write/lock/list/reserve/release/sweep logic from `pulse_reservations.mjs`.
3. Make `pulse_reservations.mjs` a CLI wrapper.
4. Update status code to use reservation summary from `pulse_reservation_store.mjs`.
5. Remove duplicate reservation functions from `pulse_state.mjs`.

## Phase 5: Extract Status Domains

Goals:

- reduce `pulse_status_model.mjs` into composition logic
- isolate independent status domains

Actions:

Create and wire:

```text
pulse_project_docs.mjs
pulse_memory_recall.mjs
pulse_handoffs.mjs
pulse_history_lifecycle.mjs
pulse_gitnexus_readiness.mjs
pulse_recommendations.mjs
```

Each module should have narrow exports and no CLI behavior.

## Phase 6: Rewrite Router/Reference Docs

Goals:

- align docs with router + plugin-runtime + repo-data model
- avoid command docs recreating session/readiness logic

Actions:

1. Update `skills/workflow/SKILL.md` as a thin router.
2. Update `references/use/command.md` to define check/apply/load contract.
3. Update `references/use/readiness.md` to remove `.pulse/scripts/*` as required runtime files.
4. Update command `command.md` entrypoints to consume runtime status/session context and point to command-local supporting references when needed.
5. Keep cross-command invariants under `references/shared/`; keep command-specific contracts inside the relevant command reference directory.
6. Keep `HARNESS.md` canonical in skill references and materialize only `HARNESS_BACKLOG.md`.

## Testing Plan

Add or update tests for:

1. `checkRepo` detects missing repo data files without mutating.
2. `applyRepo` materializes repo data files.
3. `applyRepo` does not copy canonical runtime scripts into `.pulse/scripts/`.
4. `applyRepo` does not copy skill-only `onboard_pulse.mjs`.
5. `node skills/workflow/scripts/pulse_status.mjs --repo-root <tmp-repo> --json` works.
6. `node skills/workflow/scripts/pulse_session_load.mjs --repo-root <tmp-repo> --json` works.
7. `node skills/workflow/scripts/pulse_reservations.mjs --repo-root <tmp-repo> reserve/list/release --json` works.
8. Running those scripts from inside the temp repo without `--repo-root` resolves the repo via git root/cwd.
9. Session load rejects unsafe read-first paths.
10. Brownfield `.pulse`, `docs`, and `works` backup behavior remains intact.
11. Workgraph views still rebuild after onboarding.
12. Status output includes session load instead of recalculating posture independently.
13. Optional `.pulse/scripts/` shims, if present, are warnings/compatibility only.

## Acceptance Criteria

The refactor is complete when these are true:

```text
1. pulse:workflow use is idempotent.
2. Already-onboarded repos do not get structurally rewritten during use.
3. Missing/stale repo data gets repaired through skill-owned installer logic.
4. .pulse/ is data-only by default.
5. .pulse/scripts/ is not required for readiness.
6. Canonical runtime scripts live under skills/workflow/scripts/*.mjs.
7. Every runtime CLI accepts --repo-root and otherwise resolves from git root/cwd.
8. Session loading lives in runtime as pulse_session_load.mjs.
9. pulse_state.mjs contains only state primitives.
10. pulse_status.mjs is a thin CLI wrapper.
11. Reservation store logic has one authority.
12. Runtime status includes session_load as a first-class section.
13. Command references consume runtime status instead of recreating readiness logic.
14. Tests prove plugin-owned scripts work against target repos by --repo-root and cwd.
```

## Key Invariants

```text
Packaged skill scripts own executable runtime behavior.
Target repo .pulse/ owns data, not canonical code.
{{scripts_path}} is a provider/build concern, not runtime script logic.
repoRoot is a runtime concern resolved from --repo-root, PULSE_REPO_ROOT, git root, or process.cwd().
Downstream workflow commands must consume runtime context, not recreate onboarding/session logic.
.pulse/workgraph/items.jsonl remains the canonical work metadata source.
works/ remains human-facing work content.
.pulse/runtime/state.json remains the canonical machine-readable runtime mirror.
.pulse/runtime/STATE.md remains the human-readable runtime mirror.
```

## Final Recommendation

Do not treat this refactor as a “make files shorter” cleanup. The real goal is authority separation.

Recommended final ownership:

```text
onboard_pulse.mjs
  owns readiness and repo data materialization from packaged skill source

pulse_paths.mjs
  owns repo-root resolution and path registry

pulse_session_load.mjs
  owns loading current session context

pulse_status_model.mjs
  owns composing status JSON

pulse_status_render.mjs
  owns text output

pulse_state.mjs
  owns state read/write only

pulse_reservation_store.mjs
  owns reservation storage and locking

pulse_reservations.mjs
  owns reservation CLI dispatch

pulse_work.mjs
  owns workgraph mutation
```

Pulse should become a router skill with plugin-owned runtime scripts and a repo-local data plane. That keeps the workflow surface as clear as Impeccable, preserves plugin update behavior, and still supports Pulse’s stronger requirements: durable runtime state, resumable sessions, workgraph mutation, and safe multi-agent coordination inside target repositories.
