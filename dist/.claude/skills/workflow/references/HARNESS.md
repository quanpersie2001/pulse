# Pulse Harness Reference

This is the canonical reference document for the Pulse harness in the single-router model.

## What the harness is

The Pulse harness is the repo-local operating environment that lets `pulse:workflow` coordinate workflow decisions with runtime state, workgraph metadata, and human work artifacts.

It is not a separate public skill.
It is the support system around the `pulse:workflow` router.

## Layer relationship

Pulse has four important layers:

1. **Router layer** — `skills/workflow/`
   - user-facing workflow contract
   - command docs and shared references
2. **Runtime layer** — `.pulse/runtime/`
   - session state, gate state, handoffs, reservations
3. **Workgraph layer** — `.pulse/workgraph/`
   - canonical item metadata and derived views
4. **Work content layer** — `works/`
   - human-authored epics, stories, tasks, bugs, and verification files

The harness coordinates these layers without collapsing them into one file tree.

## Operator expectations

An operator using the harness should expect Pulse to:

- route work through `pulse:workflow <command>`
- keep approvals explicit
- keep runtime status inspectable
- keep canonical work metadata in one place
- keep implementation and verification evidence attached to human-readable work artifacts
- support pause, resume, and swarm coordination without inventing a second public surface

## Canonical source locations in the plugin repo

- router source: `skills/workflow/SKILL.md`
- shared router references: `skills/workflow/references/shared/`
- harness reference source: `skills/workflow/references/HARNESS.md`
- harness backlog template source: `skills/workflow/templates/HARNESS_BACKLOG.md`
- structured command metadata source: `skills/workflow/scripts/command-metadata.json`

## Canonical runtime locations in an installed or self-hosted repo

- runtime state: `.pulse/runtime/`
- workgraph metadata: `.pulse/workgraph/`
- harness backlog materialization: `.pulse/harness/HARNESS_BACKLOG.md`
- work content: `works/`

## What does not belong here

- `HARNESS.md` is not a seed file to materialize into `.pulse/harness/`
- `HARNESS.md` is not the canonical location for mutable runtime state
- `HARNESS_BACKLOG.md` is not the full harness contract

## Router and runtime boundary

- `pulse:workflow ...` chooses the workflow move
- `node .claude/skills/workflow/scripts/pulse.mjs ...` reads readiness and coordinates runtime reservations

The harness must make that boundary obvious to operators.

## Phase 1 note

This phase establishes the reference source and the backlog template split.
Runtime relocation and materialization behavior will follow in later phases.
