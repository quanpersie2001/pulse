# `pulse:workflow use`

Session entrypoint, readiness repair, runtime materialization, Pulse session loading, and next-command recommendation for Pulse v2.

## Mission

`pulse:workflow use` is the normal command an agent runs when it wants to use Pulse in a target repo.

It combines two responsibilities behind one public command:

1. **Readiness/onboarding phase** — if the repo is missing or stale, materialize and repair the v2 runtime surface.
2. **Session-load phase** — once readiness is established, load the current Pulse session from runtime pointers, handoffs, workgraph metadata, and selected work artifacts.

## Authority

`pulse:workflow use` is the workflow session authority.

Downstream workflow commands may consume the result, but they must not recreate their own readiness or session-restoration system. If they cannot prove readiness or current context, route back to `pulse:workflow use`.

## When to run

Run `pulse:workflow use` when:

- starting any Pulse session in a repo
- resuming after environment, branch, dependency, or runtime drift
- `.pulse/runtime/*`, `.pulse/workgraph/*`, or `.pulse/harness/*` is missing or stale
- the current workflow command, active work item, or next safe action is unclear
- a handoff exists and the operator needs to resume from it
- a workflow command cannot prove current runtime readiness or session context

## Phase 1: readiness and materialization

Use first checks whether the repo can safely run Pulse. If the repo is already onboarded and current, this phase is effectively a fast no-op.

Use verifies the v2 runtime plane:

```text
.pulse/runtime/tooling-status.json
.pulse/runtime/state.json
.pulse/runtime/STATE.md
.pulse/runtime/handoffs/manifest.json
.pulse/runtime/reservations.json
```

Use verifies the v2 workgraph plane:

```text
.pulse/workgraph/items.jsonl
.pulse/workgraph/schema.json
.pulse/workgraph/views/active.json
.pulse/workgraph/views/closed.json
.pulse/workgraph/views/ready.json
.pulse/workgraph/views/graph.json
.pulse/workgraph/write.lock
```

Use verifies that the installed workflow skill exposes the canonical `scripts/pulse.mjs` runtime entrypoint. Canonical runtime code is not copied into the target repo.

If `.pulse/scripts/` exists, treat it as non-authoritative historical data. Missing or stale shims are not baseline readiness blockers.

Use verifies the harness seed artifact:

```text
.pulse/harness/HARNESS_BACKLOG.md
```

`skills/workflow/references/HARNESS.md` remains the canonical harness reference. Use must not create or rely on a second canonical `.pulse/harness/HARNESS.md`.

## Source-of-truth rules

- `.pulse/workgraph/items.jsonl` is the only writable work item metadata truth.
- `.pulse/runtime/state.json` is the machine-readable runtime routing mirror.
- `.pulse/runtime/STATE.md` is the human-readable runtime mirror.
- `.pulse/runtime/reservations.json` stores execution leases only; durable responsibility belongs in work item `owner`.
- Generated workgraph views are derived data and must never flow back into `items.jsonl` as canonical fields.
- `works/` stores human-facing work content; it does not replace the workgraph metadata source.

## Outcome contract

Exactly one readiness outcome is valid:

| Outcome | Meaning |
| --- | --- |
| `PASS` | The requested workflow posture is safe. |
| `DEGRADED` | The workflow can continue only with a stated reduced capability. |
| `FAIL` | Required prerequisites or runtime posture are unsafe; downstream workflow routing must stop. |

A complete use result must report:

- repo root
- readiness outcome
- requested mode, when known
- recommended mode
- missing blockers
- degraded capabilities
- domain status for `.pulse/`, `docs/`, and `works/`
- workgraph health
- runtime materialization status
- plugin runtime availability and optional shim warnings
- harness backlog status
- session-load posture
- selected handoff, when one is selected
- resumable handoffs, if present
- active runtime command and active work item IDs, if present
- read-first paths
- loaded files, missing files, and rejected unsafe paths
- recommended next workflow command

## Core blockers

Treat these as blockers for v2 readiness:

- missing `git` when repo identity or normal source-control operations are required
- missing `node` for plugin-owned runtime helpers
- missing or unreadable Pulse workflow source tree
- unavailable installed workflow runtime entrypoint after use has been asked to materialize repo data
- invalid `.pulse/workgraph/items.jsonl`
- missing or invalid `.pulse/workgraph/schema.json`
- active workgraph write lock owned by a live process
- runtime state that conflicts with canonical workgraph metadata
- handoff manifest entries that point to missing or conflicting owner files
- work content paths that escape `works/`
- unsafe `read_first` paths that try to escape the repo
- missing required verification evidence when a requested close operation is being prepared

## Degraded capabilities

Use `DEGRADED` only when the workflow remains safe but reduced. Examples:

- swarm coordination is unavailable, but single-worker execution is safe
- optional GitNexus discovery is not configured, but file-based discovery is available
- a domain is non-compliant, but use can back it up and rebuild the v2 shape safely

Do not use `DEGRADED` to bypass a blocker.

## First-run materialization model

The readiness/onboarding phase should use this sequence:

1. Detect current target repo posture.
2. Classify `.pulse/`, `docs/`, and `works/` as `missing`, `compliant`, or `non_compliant`.
3. If a domain is missing, create the required v2 structure.
4. If `.pulse/` exists but is non-compliant, move active contents into `.pulse/backup-<date>/`, rebuild the v2 `.pulse/` layout, then preserve known-safe data such as memory and runtime state in the new layout.
5. If `docs/` exists but is non-compliant, move active contents into `docs/backup-<date>/`, scaffold the v2 docs shape, and emit a docs regeneration brief for AI-assisted reconstruction from backed-up docs plus the codebase.
6. If `works/` exists but is non-compliant, move active contents into `works/backup-<date>/`, scaffold the v2 works shape, and emit a works reconstruction brief for AI-assisted reconstruction into `works/epics/**` plus `.pulse/workgraph/items.jsonl`.
7. Materialize `.pulse/harness/HARNESS_BACKLOG.md` from `skills/workflow/templates/HARNESS_BACKLOG.md`.
8. Rebuild generated workgraph views.
9. Write `tooling-status.json`, `state.json`, `STATE.md`, and the onboarding install receipt.
10. Continue into session-load.

Use must not silently overwrite human-authored docs or work artifacts. Brownfield normalization backs up in-place first and writes reconstruction briefs under `.pulse/runtime/onboarding/` for semantic follow-up.

## Phase 2: session load

Session load starts after readiness is known or repaired.

It must read from pointers, not by scanning the whole repo:

1. Read `.pulse/runtime/state.json`.
2. Read `.pulse/runtime/tooling-status.json`.
3. Read `.pulse/runtime/handoffs/manifest.json`.
4. Read `.pulse/runtime/reservations.json` for posture only.
5. If exactly one active handoff exists, read that owner file automatically.
6. If multiple active handoffs exist, present resume options and require operator selection before reading an owner file.
7. Resolve active epic/story/item IDs through `.pulse/workgraph/items.jsonl`.
8. Read only safe `read_first`, workgraph `content_path`, workgraph `verification_path`, and memory-hook paths.
9. Report loaded, missing, rejected, and conflicted paths.
10. Recommend the next supported workflow command without executing it.

Session load must never auto-execute the next workflow step.

## Session-load output

Use writes a `session_load` object into runtime status and state:

```json
{
  "posture": "fresh | resumable | active | conflicted",
  "requires_selection": false,
  "selected_handoff": null,
  "resume_options": [],
  "active_context": {
    "active_command": "explore",
    "active_epic_id": "E-...",
    "active_story_id": "S-...",
    "active_item_id": "T-..."
  },
  "workgraph_items": [],
  "read_first": [],
  "loaded_files": [],
  "missing_files": [],
  "rejected_paths": [],
  "conflicts": [],
  "summary": "...",
  "next_action": "...",
  "next_command": "pulse:workflow explore"
}
```

Allowed session-read paths are repo-relative and must stay inside one of:

```text
AGENTS.md
.pulse/runtime/handoffs/
.pulse/memory/
works/
docs/
```

Absolute paths, path traversal, non-normalized paths, and unknown roots must be rejected.

## Resume handling

Resume starts from `.pulse/runtime/handoffs/manifest.json`.

1. Read the manifest.
2. Present active handoffs by owner, surface, active command, active work item, summary, and next action.
3. If one entry exists, read only that owner file.
4. If multiple entries exist, ask the user which one to resume.
5. Use the selected handoff's `summary`, `next_action`, `read_first`, `payload.transfer`, and memory hooks.
6. Ask for explicit confirmation before continuing work.

See [Handoff contract](handoff-contract.md) for the router-aware handoff schema.

## Target repo scope

Use runs against the repo where Pulse is installed.

Rules:

- treat `.pulse/` as the repo-local Pulse data plane
- treat `works/` as the installed work-content plane
- run canonical runtime helpers from the installed workflow skill package, not from copied repo-local code
- materialize only data/template artifacts that belong in the target repo
- do not require the target repo to contain the plugin source tree

## Next-command recommendation

Use recommends supported v2 workflow surfaces only.

| Situation | Recommendation |
| --- | --- |
| Repo is not bootstrapped or readiness is stale | `pulse:workflow use` |
| Request is vague or design intent is not formed | `pulse:workflow brainstorm` |
| Feature intent exists but implementation context is unresolved | `pulse:workflow explore` |
| Decisions are approved and implementation planning is next | `pulse:workflow plan` |
| Plan exists but feasibility and readiness need proof | `pulse:workflow validate` |
| Execution is approved and swarm capability is available | `pulse:workflow swarm` |
| Execution is approved and single-worker mode is recommended | `pulse:workflow execute` |
| Implementation is complete and quality gate is next | `pulse:workflow review` |
| Cycle is complete and durable learnings should be captured | `pulse:workflow compound` |

## Gate posture

Use reports gate state; it does not approve gates.

- Gate approval remains an explicit human decision.
- Use must not mark a gate approved because files exist.
- Use must not skip `validate` before execution-capable workflow.
- `review` findings with blocking severity remain blockers for merge readiness.

## Red flags

Return `FAIL` or pause for human decision when:

- execution is requested while recommended mode is `blocked`
- runtime state and workgraph metadata disagree about active work
- a live write lock is present
- generated views are stale and cannot be rebuilt safely
- a handoff manifest points to missing or conflicting owner files
- multiple active handoffs exist and no owner has been selected
- work content paths escape `works/`
- read-first paths escape the repo or point outside allowed roots

## Command-local references

- [Readiness](readiness.md)
- [Handoff contract](handoff-contract.md)
- [Pressure scenarios](pressure-scenarios.md)
