# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Repo Is

Pulse is a packaged skill plugin for Claude Code and Codex. Its public workflow surface is a single router skill: `pulse:workflow`. Runtime reads and reservations use the rendered `{{pulse_command}}` command from the installed workflow skill.

## Repository Layout

- `skills/workflow/` — canonical source for the Pulse router skill and subcommands
- `.codex-plugin/plugin.json` — Codex plugin manifest
- `.claude-plugin/plugin.json` — Claude plugin manifest
- `.mcp.json` — packaged MCP manifest for shared runtime servers
- `.agents/plugins/marketplace.json` — Codex marketplace metadata
- `scripts/check-markdown-links.sh` — markdown link verification helper
- `AGENTS.md` — operator workflow rules
- `references/` — upstream/pedagogical material; not part of the shipped plugin contract

## Key Tools

| Tool | CLI | Purpose |
|------|-----|---------|
| Pulse Router | `pulse:workflow` | User-facing workflow entrypoint |
| Pulse Runtime | `{{pulse_command}}` | Runtime status, readiness, and reservation coordination |
| Scout | `{{pulse_command}} status --repo-root <repo> --json` | Read-only runtime orientation |
| Git | `git` | Version control |
| Native swarm adapters | — | Claude Code teammates or Codex subagents |

## Delivery Chain

The core workflow is a gated, linear pipeline:

```
pulse:workflow use → pulse:workflow explore → pulse:workflow plan → pulse:workflow validate → pulse:workflow swarm or pulse:workflow execute → pulse:workflow review → pulse:workflow compound
```

`pulse:workflow use` is the normal session-entry command. It runs onboarding/bootstrap behavior when needed, then restores current Pulse context before downstream workflow work.

Four human gates control progression:
- **GATE 1** (after explore): Approve locked decision context.
- **GATE 2** (after plan): Approve selected shape artifact.
- **GATE 3** (after validate): Approve feasibility-validated current work before execution.
- **GATE 4** (after review): P1 findings must be fixed before merge approval.

## Artifact Locations

```
.pulse/runtime/tooling-status.json      ← onboarding/readiness status
.pulse/runtime/state.json               ← machine-readable routing/runtime mirror
.pulse/runtime/STATE.md                 ← shared human-readable state
.pulse/runtime/handoffs/manifest.json   ← owner-scoped pause/resume index
.pulse/runtime/reservations.json        ← runtime reservations
.pulse/workgraph/items.jsonl            ← canonical workgraph metadata source
.pulse/workgraph/schema.json            ← workgraph schema contract
.pulse/workgraph/views/                 ← derived runtime views
.pulse/harness/HARNESS_BACKLOG.md       ← materialized harness backlog template
works/                                  ← work content artifacts
.pulse/memory/                          ← shared reusable memory output
```

## Editing Skills

The public workflow skill lives at `skills/workflow/SKILL.md`.

When adding or modifying command behavior:

1. Update router contract in `skills/workflow/SKILL.md` as needed.
2. Update the command module under `skills/workflow/references/<command>/command.md`.
3. Keep shared rules in `skills/workflow/references/shared/`.

## Testing

Pulse has automated coverage for onboarding/runtime control-plane behavior in `tests/`. The `references/superpowers/tests/brainstorm-server/` suite is reference material and not part of the shipping plugin.

## Session Protocol

```bash
git status
# edit files
{{pulse_command}} status --repo-root <repo> --json
{{pulse_command}} ready --repo-root <repo> --json
git add <files>
git commit -m "..."
git push
```

