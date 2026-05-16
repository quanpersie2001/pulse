# Executing Runtime Appendix

Use this appendix for detailed checklists and command templates referenced by `../SKILL.md`.

## Non-trivial bead rule

Before reserving files or writing code, read the active-shape current-work artifacts whenever any trigger condition below is true.

Shape artifact requirements:
- work-shape path: `history/<feature>/work-shape.md` (current work is defined directly there)
- epic-map path: `history/<feature>/epic-map.md` and `history/<feature>/current-story-pack.md`
- phase-plan path: `history/<feature>/phase-plan.md`, `history/<feature>/phase-<n>-contract.md`, and `history/<feature>/phase-<n>-story-map.md`

Trigger conditions:
- `testing_mode` is `tdd-required`
- the bead touches multiple files across different modules or ownership boundaries
- the bead has multiple upstream dependencies or explicitly references story coordination, parallelism, shared file/context risk, or boundary preservation
- the `verify` path is multi-step, integration-heavy, or hard to explain in one line from the bead alone
- after reading the bead, more than one plausible implementation path still seems possible

For beads that touch module interfaces, ownership boundaries, or HIGH-risk constraints, also read relevant sections of `history/<feature>/approach.md`.

## Verification evidence contract

Update every artifact listed in the bead `verification_evidence` field. Standard path:

```text
history/<feature>/verification/<bead-id>.md
```

Evidence must include:

- bead ID and feature name
- `testing_mode`
- verification timestamp
- every `verify` command actually run
- exit code for each command
- concise observed result for each command
- links/paths to generated proof artifacts, screenshots, logs, or findings

For `tdd-required`, also include:

- `tdd_steps.red`
- expected failure signal observed
- `tdd_steps.green`
- passing signal observed

## Worker command quick reference

| Action | Call |
|--------|------|
| Get priority bead | `bv --robot-priority` |
| Read bead | `br show <id>` |
| Reserve files | `node .pulse/scripts/pulse_reservations.mjs reserve --agent <runtime_identity> --bead <bead-id> --path "..." --json` |
| Release files | `node .pulse/scripts/pulse_reservations.mjs release --agent <runtime_identity> --json` |
| Inspect reservations | `node .pulse/scripts/pulse_reservations.mjs list --active-only --json` |
| Close bead | `br close <id> --reason "..."` |
| Send `[ONLINE]` / `[DONE]` / `[BLOCKED]` / `[FILE CONFLICT]` / `[HANDOFF]` | active coordination surface |

## Swarm worker bootstrap inputs

- `runtime_identity`
- `coordinator_identity`
- `adapter_name` (`claude-code` or `codex`)
- `epic_id`
- `feature_name`
- optional `startup_hint`

If any required startup input is missing, request clarification on the active coordination surface before proceeding.

## Post-compact recovery sequence

If you detect context compaction, stop and re-read before further implementation:

1. `AGENTS.md`
2. `history/<feature>/CONTEXT.md`
3. `br show <bead-id>`
4. If non-trivial, active-shape current-work artifacts (`work-shape.md`, `current-story-pack.md`, or phase contract/story map) and boundary-relevant `approach.md`
5. `node .pulse/scripts/pulse_reservations.mjs list --active-only --json`
6. latest coordinator updates on the active coordination surface

## Red flags

- executing without reading the bead fully
- executing a bead missing canonical schema fields
- writing outside reserved scope
- skipping verification or redefining success after failures
- closing without substantive `verification_evidence`
- claiming `tdd-required` without a real red failure and green pass
- continuing after compaction without re-reading required context
- implementing stubs/TODOs/empty handlers
- ignoring locked decisions from `CONTEXT.md`
- skipping required current-work artifact reads for non-trivial work
- guessing through ambiguity instead of surfacing it
- bundling multiple beads in one commit
- claiming without reservation checks
- closing/blocking without coordination reporting
- waiting silently while blocked/conflicted/done
- reading all learnings instead of bead-cited learning refs
- writing the retired global handoff file
