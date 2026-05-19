# Pulse Handoff Contract

Pulse uses owner-scoped handoffs plus a small manifest. It does not use one global handoff file.

## Why

The handoff system has to support multiple paused actors safely:

- workflow command owners
- swarm coordinator
- worker agents
- single-worker degraded execution
- standalone utility runs

One shared JSON file creates race conditions and schema drift. Pulse avoids that by separating:

- `manifest.json` -> discovery and resume index
- `<owner>.json` -> checkpoint for exactly one owner

## Directory Layout

```text
.pulse/
  runtime/
    handoffs/
      manifest.json
      workflow-plan.json
      coordinator.json
      worker-<agent>.json
      single-worker.json
      utility-<name>.json
      archive/
```

## Manifest Schema

`manifest.json` is intentionally small:

```json
{
  "schema_version": "1.0",
  "updated_at": "<ISO-8601>",
  "active": [
    {
      "owner_id": "workflow-plan",
      "owner_type": "workflow_command",
      "surface": "pulse:workflow",
      "active_command": "plan",
      "active_epic_id": "E-0V9K4F",
      "active_story_id": "S-0V9K4G",
      "active_item_id": "T-0V9K4H",
      "path": ".pulse/runtime/handoffs/workflow-plan.json",
      "phase": "plan/draft-execplan",
      "next_action": "Finish the task breakdown for the active story",
      "summary": "Planning is paused with the story scope approved and task breakdown in progress."
    }
  ]
}
```

For standalone utilities, use the utility surface directly:

```json
{
  "owner_id": "utility-architecture-rescue",
  "owner_type": "utility",
  "surface": "pulse:architecture-rescue",
  "active_command": null,
  "active_epic_id": null,
  "active_story_id": null,
  "active_item_id": null,
  "path": ".pulse/runtime/handoffs/utility-architecture-rescue.json",
  "phase": "analysis",
  "next_action": "Review the unresolved architecture findings",
  "summary": "Architecture rescue is paused after initial system boundary analysis."
}
```

## Owner File Envelope

Every owner file must use the same outer envelope:

```json
{
  "schema_version": "2.0",
  "handoff_id": "workflow-plan-2026-05-20T10:15:00Z",
  "owner_type": "workflow_command|coordinator|worker|utility",
  "owner_id": "workflow-plan|coordinator|worker-blue-lake|single-worker|utility-architecture-rescue",
  "surface": "pulse:workflow",
  "active_command": "plan",
  "active_epic_id": "E-0V9K4F",
  "active_story_id": "S-0V9K4G",
  "active_item_id": "T-0V9K4H",
  "phase": "plan/draft-execplan",
  "status": "paused|ready_to_resume|consumed|archived",
  "paused_at": "<ISO-8601>",
  "reason": "context_critical",
  "next_action": "Finish the task breakdown for the active story",
  "read_first": [
    ".pulse/runtime/STATE.md",
    ".pulse/workgraph/items.jsonl",
    "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md"
  ],
  "summary": "Planning is paused with story scope approved and task breakdown in progress.",
  "payload": {}
}
```

## Standard Pause/Resume Contract

The shared envelope carries the same three handoff-facing blocks across workflow commands, coordinator, worker, single-worker, and standalone utility owners:

1. `summary` — one short plain-language handoff summary of what is happening now
2. `next_action` + `read_first` — the resume briefing for the next agent turn
3. `payload.transfer` — the detailed transfer block with owner-specific state needed to continue safely

Treat these blocks as complementary, not interchangeable:

- `summary` is the one-read headline for the manifest and resume chooser.
- `next_action` says the first concrete move after resume.
- `read_first` is the ordered file list to reload before acting.
- `payload.transfer` holds state that is too detailed for the top-level envelope.

### Writing Rules

- Keep `summary` to 1-2 sentences in plain language.
- Keep `next_action` to a single concrete step.
- Keep `read_first` ordered from most critical reload to least.
- Always include `payload.transfer.status`, `payload.transfer.completed`, `payload.transfer.in_flight`, `payload.transfer.blockers`, and `payload.transfer.resume_notes`.
- Use empty arrays when a transfer section has nothing to report; do not omit the field.
- Use `surface: "pulse:workflow"` plus `active_command` for workflow phases.
- Use `surface: "pulse:<standalone-skill>"` and `active_command: null` for standalone utility handoffs.

### Transfer Block Shape

```json
{
  "payload": {
    "transfer": {
      "status": "What is true right now in plain language",
      "completed": [
        "Concrete things finished before pause"
      ],
      "in_flight": [
        "Exactly one active work item or the next item to pick up"
      ],
      "blockers": [
        "Anything blocking safe resume; empty array if none"
      ],
      "resume_notes": [
        "Checks, commands, or coordination notes the next turn must honor"
      ]
    }
  }
}
```

## Payload Expectations

Keep owner-specific data inside `payload`.

Examples:

- `workflow-plan.json`
  - `completed_through`
  - `artifacts_written`
  - `items_created`
  - `open_questions`
- `coordinator.json`
  - `active_epic_id`
  - `graph_status`
  - `active_workers`
  - `blockers`
- `worker-<agent>.json`
  - `active_item_id`
  - `reserved_files`
  - `verification_state`
- `single-worker.json`
  - `active_item_id`
  - `completed_items`
  - `blocked_items`
- `utility-<name>.json`
  - `utility_surface`
  - `analysis_scope`
  - `artifacts_written`
  - `next_operator_decision`

## Lifecycle

1. Owner writes or updates its own handoff file.
2. Owner registers or updates its entry in `manifest.json`.
3. Resume starts from the manifest, never by scanning arbitrary files.
4. After successful resume, mark the owner file `consumed` or move it to `archive/`.
5. Remove the manifest entry only after the resume is confirmed.

## Human-Readable Companion Formats

The JSON files are the source of truth for machine-readable resume state. The formats below are canonical rendered outputs for presenting that state to humans.

If a Pulse command presents a human-facing handoff note, it should render from the authoritative JSON handoff/manifest values instead of improvising new prose fields.

### 1. Handoff Summary Format

Use this when pausing work and explaining what the next person or next session should know at a glance.

```markdown
## Handoff Summary
- Owner: workflow-plan
- Surface: pulse:workflow
- Active command: plan
- Active epic: E-0V9K4F
- Active story: S-0V9K4G
- Active item: T-0V9K4H
- Phase: plan/draft-execplan
- Status: ready_to_resume
- Paused at: 2026-05-20T10:15:00Z
- Reason: context_critical
- Next action: Finish the task breakdown for the active story
- Read first:
  - .pulse/runtime/STATE.md
  - .pulse/workgraph/items.jsonl
  - works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md
- Summary: Planning is paused with story scope approved and task breakdown in progress.
```

Guidance:

- Keep it short enough to scan in one screen.
- Reuse the same values already present in the owner envelope.
- Do not invent fields that are not represented in the JSON handoff.
- `Summary` should explain the current state in plain language, not just restate the phase name.

### 2. Resume Briefing Format

Use this after a user chooses a manifest entry to resume. This is for conversation output, not a replacement for the owner file.

```markdown
## Resume Briefing
- Resuming: workflow-plan via pulse:workflow plan
- Active epic: E-0V9K4F
- Active story: S-0V9K4G
- Active item: T-0V9K4H
- Phase: plan/draft-execplan
- Current state: Story scope is approved and task breakdown is in progress.
- Next action: Finish the task breakdown for the active story.
- Required reads:
  - .pulse/runtime/STATE.md
  - .pulse/workgraph/items.jsonl
  - works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md
- Resume check: wait for explicit user confirmation before continuing.
```

Guidance:

- Frame the briefing around what will happen next in practical terms.
- Translate technical status into plain language when possible.
- Keep the `Next action` aligned with the manifest and owner file.
- Always preserve the explicit confirmation rule before continuing work.

### 3. Paste-Ready Transfer Block Format

Use this when one agent, owner, or session needs to hand off context to another chat or tool. It should be easy to copy and paste without extra cleanup.

````markdown
```text
PULSE TRANSFER
owner=workflow-plan
surface=pulse:workflow
active_command=plan
active_epic_id=E-0V9K4F
active_story_id=S-0V9K4G
active_item_id=T-0V9K4H
phase=plan/draft-execplan
status=ready_to_resume
paused_at=2026-05-20T10:15:00Z
reason=context_critical
next_action=Finish the task breakdown for the active story
read_first=.pulse/runtime/STATE.md | .pulse/workgraph/items.jsonl | works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md
summary=Planning is paused with story scope approved and task breakdown in progress.
handoff_path=.pulse/runtime/handoffs/workflow-plan.json
manifest_path=.pulse/runtime/handoffs/manifest.json
```
````

Guidance:

- Keep it single-purpose and copyable.
- Prefer one field per line.
- Use repo-relative paths exactly as they appear in the repo.
- Include both the owner handoff path and the manifest path.
- Do not claim the block was auto-generated unless a real command produced it.

## Checkpoint Companion Contract (v1)

Pulse checkpoints are advisory operator aids under `.pulse/runtime/checkpoints/<scope>/...`.
They are not a replacement for owner handoffs, and they are not a second workflow state machine.

Recommended v1 actions:

- `save` -> capture a scoped checkpoint artifact from known current state
- `list` -> enumerate checkpoints for a scope with compact summaries
- `show` -> render one checkpoint in human-readable and JSON-friendly form
- `diff` -> compare two checkpoints at the summary level
- `resume-brief` -> synthesize a restart packet from a checkpoint plus current runtime/handoff state

Recommended checkpoint record shape:

```json
{
  "schema_version": "1.0",
  "checkpoint_id": "2026-05-20T10-30-00Z-plan",
  "created_at": "2026-05-20T10:30:00.000Z",
  "summary": "Planning is complete and validation is next.",
  "next_action": "Run pulse:workflow validate for the active story.",
  "captured": {
    "surface": "pulse:workflow",
    "active_command": "plan",
    "phase": "plan/finalized",
    "gate": "GATE 2",
    "requested_mode": "standard_feature",
    "active_epic_id": "E-0V9K4F",
    "active_story_id": "S-0V9K4G",
    "active_item_id": null
  },
  "links": {
    "workgraph": ".pulse/workgraph/items.jsonl",
    "state": ".pulse/runtime/state.json",
    "handoff": ".pulse/runtime/handoffs/workflow-plan.json",
    "story": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md",
    "story_spec": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/SPEC.md"
  },
  "blockers": [],
  "memory_hooks": {
    "critical_patterns": ".pulse/memory/critical-patterns.md",
    "learnings": [".pulse/memory/learnings/20260520-auth-refresh.md"],
    "corrections": [],
    "ratchet": []
  }
}
```

Checkpoint rules:

- Checkpoints are advisory snapshots. If checkpoint content disagrees with current handoff, runtime state, or workgraph artifacts, current handoff/runtime/workgraph artifacts win.
- A checkpoint scope directory must contain only `manifest.json` and valid checkpoint `*.json` records.
- Debugging or verification evidence belongs in the relevant work item `verification.md` or a clearly linked artifact path, not in checkpoint state.
- `resume-brief` can point to relevant memory files, but it must not create a second durable memory store.
- `diff` should stay summary-level: surface, active command, phase, gate, mode, next action, blockers, links, and memory hooks.
- `save` may write a checkpoint record, but read-only status/scout flows must never mutate runtime state.
- Productized checkpoint trigger points should be phase/gate transitions, pause-handoff boundaries, and pre-review freeze-frames — not arbitrary heartbeat logging.

## Rules

1. One owner file = one writer.
2. `pulse:workflow use` relies on the manifest plus the common envelope for resume discovery.
3. Workers do not overwrite coordinator state.
4. Coordinator does not overwrite worker state.
5. Resume flows always require user confirmation.
6. Human-readable summaries must stay consistent with the JSON handoff; if they drift, the JSON handoff wins.
7. Human-readable handoff notes are rendered companions from authoritative JSON, not a second source of truth.
8. Checkpoints are advisory artifacts, not authoritative pause/resume ownership records.
