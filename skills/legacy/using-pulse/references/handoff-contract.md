# Pulse Handoff Contract

Pulse uses owner-scoped handoffs plus a small manifest. It does not use one global handoff file.

## Why

The handoff system has to support multiple paused actors safely:

- planning
- swarm coordinator
- worker agents
- single-worker degraded execution

One shared JSON file creates race conditions and schema drift. Pulse avoids that by separating:

- `manifest.json` -> discovery and resume index
- `<owner>.json` -> checkpoint for exactly one owner

## Directory Layout

```text
.pulse/
  runtime/
    handoffs/
      manifest.json
      planning.json
      coordinator.json
      worker-<agent>.json
      single-worker.json
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
      "owner_id": "planning",
      "owner_type": "phase",
      "skill": "pulse:planning",
      "feature": "auth-refresh",
      "path": ".pulse/runtime/handoffs/planning.json",
      "phase": "planning/phase-4",
      "next_action": "Create remaining task beads",
      "summary": "Discovery and approach are complete"
    }
  ]
}
```

## Owner File Envelope

Every owner file must use the same outer envelope:

```json
{
  "schema_version": "2.0",
  "handoff_id": "planning-2026-03-27T10:15:00Z",
  "owner_type": "phase|coordinator|worker",
  "owner_id": "planning|GreenCastle|worker-blue-lake|single-worker",
  "skill": "pulse:planning",
  "feature": "auth-refresh",
  "phase": "planning/phase-4",
  "status": "paused|ready_to_resume|consumed|archived",
  "paused_at": "<ISO-8601>",
  "reason": "context_critical",
  "next_action": "Create remaining task beads",
  "read_first": [
    ".pulse/runtime/STATE.md",
    "history/auth-refresh/CONTEXT.md"
  ],
  "summary": "Discovery and approach are complete; bead creation is next.",
  "payload": {}
}
```

## Standard Pause/Resume Contract

The shared envelope carries the same three handoff-facing blocks across planning, coordinator, worker, and single-worker owners:

1. `summary` — one short plain-language handoff summary of what is happening now
2. `next_action` + `read_first` — the resume briefing for the next agent turn
3. `payload.transfer` — the detailed transfer block with owner-specific state needed to continue safely

Treat these blocks as complementary, not interchangeable:

- `summary` is the one-read headline for the manifest and resume chooser.
- `next_action` says the first concrete move after resume.
- `read_first` is the ordered file list to reload before acting.
- `payload.transfer` holds the state that is too detailed for the top-level envelope.

### Writing Rules

- Keep `summary` to 1-2 sentences in plain language.
- Keep `next_action` to a single concrete step.
- Keep `read_first` ordered from most critical reload to least.
- Always include `payload.transfer.status`, `payload.transfer.completed`, `payload.transfer.in_flight`, `payload.transfer.blockers`, and `payload.transfer.resume_notes`.
- Use empty arrays when a transfer section has nothing to report; do not omit the field.

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
        "Exactly one active item or the next item to pick up"
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

- `planning.json`
  - `completed_through`
  - `artifacts_written`
  - `beads_created`
  - `open_questions`
- `coordinator.json`
  - `epic_id`
  - `graph_status`
  - `active_workers`
  - `blockers`
- `worker-<agent>.json`
  - `current_bead`
  - `reserved_files`
  - `verification_state`
- `single-worker.json`
  - `current_bead`
  - `completed_beads`
  - `blocked_beads`

## Lifecycle

1. Owner writes or updates its own handoff file.
2. Owner registers or updates its entry in `manifest.json`.
3. Resume starts from the manifest, never by scanning arbitrary files.
4. After successful resume, mark the owner file `consumed` or move it to `archive/`.
5. Remove the manifest entry only after the resume is confirmed.

## Human-Readable Companion Formats

The JSON files are the source of truth for machine-readable resume state. The formats below are the canonical rendered outputs for presenting that state to humans.

If a Pulse skill presents a human-facing handoff note, it should render from the authoritative JSON handoff/manifest values instead of improvising new prose fields.

### 1. Handoff Summary Format

Use this when pausing work and explaining what the next person or next session should know at a glance.

```markdown
## Handoff Summary
- Owner: planning
- Skill: pulse:planning
- Feature: auth-refresh
- Phase: planning/phase-4
- Status: ready_to_resume
- Paused at: 2026-03-27T10:15:00Z
- Reason: context_critical
- Next action: Create remaining task beads
- Read first:
  - .pulse/runtime/STATE.md
  - history/auth-refresh/CONTEXT.md
- Summary: Discovery and approach are complete; bead creation is next.
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
- Resuming: planning via pulse:planning
- Feature: auth-refresh
- Phase: planning/phase-4
- Current state: Discovery and approach are complete.
- Next action: Create remaining task beads.
- Required reads:
  - .pulse/runtime/STATE.md
  - history/auth-refresh/CONTEXT.md
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
owner=planning
skill=pulse:planning
feature=auth-refresh
phase=planning/phase-4
status=ready_to_resume
paused_at=2026-03-27T10:15:00Z
reason=context_critical
next_action=Create remaining task beads
read_first=.pulse/runtime/STATE.md | history/auth-refresh/CONTEXT.md
summary=Discovery and approach are complete; bead creation is next.
handoff_path=.pulse/runtime/handoffs/planning.json
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

Pulse checkpoints are feature-scoped operator aids under `.pulse/runtime/checkpoints/<feature>/...`.
They are not a replacement for owner handoffs, and they are not a second workflow state machine.

Recommended v1 actions:

- `save` -> capture a feature-scoped checkpoint artifact from known current state
- `list` -> enumerate checkpoints for a feature with compact summaries
- `show` -> render one checkpoint in human-readable and JSON-friendly form
- `diff` -> compare two checkpoints at the summary level
- `resume-brief` -> synthesize a restart packet from a checkpoint plus current runtime/handoff state

Recommended checkpoint record shape:

```json
{
  "schema_version": "1.0",
  "checkpoint_id": "2026-04-16T10-30-00Z-planning",
  "feature": "auth-refresh",
  "created_at": "2026-04-16T10:30:00.000Z",
  "summary": "Planning is complete and validation is next.",
  "next_action": "Re-open the current phase contract and run validating.",
  "captured": {
    "phase": "planning/phase-2",
    "gate": "GATE 2",
    "mode": "standard_feature",
    "story": "Story 2",
    "bead": "BEAD-014"
  },
  "links": {
    "context": "history/auth-refresh/CONTEXT.md",
    "handoff": ".pulse/runtime/handoffs/planning.json",
    "runtime_snapshot": ".pulse/runtime/state.json",
    "verification": "history/auth-refresh/verification/"
  },
  "blockers": [],
  "memory_hooks": {
    "critical_patterns": ".pulse/memory/critical-patterns.md",
    "learnings": [".pulse/memory/learnings/20260415-auth-refresh.md"],
    "corrections": [],
    "ratchet": []
  }
}
```

Checkpoint rules:

- Checkpoints are advisory snapshots. If checkpoint content disagrees with current handoff or state artifacts, current handoff/state artifacts win.
- A checkpoint feature directory must contain only `manifest.json` and valid checkpoint `*.json` records. Beads repair backups, `beads.db*`, `issues.jsonl`, and `config.yaml` are foreign artifacts in this namespace.
- Beads recovery artifacts belong under `.beads/.br_recovery/`; debugging or verification evidence belongs under `.spikes/` or `history/<feature>/verification/`, not `.pulse/runtime/checkpoints/<feature>/`.
- `resume-brief` can point to relevant memory files and `history/<feature>/lifecycle-summary.md` when present, but it must not create a second durable memory store.
- `diff` should stay summary-level: phase, gate, mode, next action, blockers, links, and memory hooks.
- `save` may write a checkpoint record, but read-only status/scout flows must never mutate runtime state.
- Productized checkpoint trigger points should be phase/gate transitions, pause-handoff boundaries, and pre-review freeze-frames — not arbitrary heartbeat logging.

## Rules

1. One owner file = one writer.
2. `using-pulse` only relies on the manifest plus the common envelope.
3. Workers do not overwrite coordinator state.
4. Coordinator does not overwrite worker state.
5. Resume flows always require user confirmation.
6. Human-readable summaries must stay consistent with the JSON handoff; if they drift, the JSON handoff wins.
7. Human-readable handoff notes are rendered companions from authoritative JSON, not a second source of truth.
8. Checkpoints are advisory artifacts, not authoritative pause/resume ownership records.
