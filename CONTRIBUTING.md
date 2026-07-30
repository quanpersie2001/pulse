# Contributing

This repo packages one plugin, `pulse`, with a single public workflow surface: `pulse:workflow`.
Use this guide when editing router commands, runtime scripts, manifests, or public docs.

## Repository Truth

These paths matter most:

- [`skills/workflow/`](skills/workflow) is the canonical source of public workflow behavior
- [`skills/workflow/references/`](skills/workflow/references) owns command-level behavior docs
- [`skills/workflow/scripts/`](skills/workflow/scripts) owns canonical runtime CLI logic
- [`.codex-plugin/plugin.json`](.codex-plugin/plugin.json) is the Codex package manifest
- [`.claude-plugin/plugin.json`](.claude-plugin/plugin.json) is the Claude plugin manifest
- [`.mcp.json`](.mcp.json) is the packaged MCP manifest for shared runtime servers
- [`.agents/plugins/marketplace.json`](.agents/plugins/marketplace.json) exposes the packaged plugin to Codex
- [`AGENTS.md`](AGENTS.md), [`README.md`](README.md), and this file are contract docs and must stay consistent

## Plugin Packaging Overview

This repository is a root-scoped packaged plugin repo.

- Codex manifest: [`.codex-plugin/plugin.json`](.codex-plugin/plugin.json)
- Claude manifest: [`.claude-plugin/plugin.json`](.claude-plugin/plugin.json)
- Marketplace metadata: [`.agents/plugins/marketplace.json`](.agents/plugins/marketplace.json)

Packaged public-surface discovery is rooted at `skills/workflow/` as declared in manifests.

## Where Workflow Behavior Lives

Public workflow behavior is routed through `skills/workflow/`:

```text
skills/workflow/
├── SKILL.md
├── commands/
├── references/
├── templates/
└── scripts/
```

Runtime status, readiness, and reservation operations use the rendered `{{pulse_command}}` command from the installed workflow skill. Canonical state lives in:

- `.pulse/runtime/`
- `.pulse/workgraph/items.jsonl`

## SKILL.md Format

Every skill needs a `SKILL.md` with YAML frontmatter and markdown body.

```yaml
---
name: my-skill
description: >-
  Use when this skill clearly applies. State trigger scenarios and expected outcomes.
metadata:
  version: '1.0'
  ecosystem: pulse
---

# My Skill

Operational instructions.
```

### Required Fields

| Field | Purpose |
|-------|---------|
| `name` | Bare skill identifier in frontmatter |
| `description` | Trigger text for skill matching |

## Pulse Workflow Conventions

### Public command surface

Pulse is documented and operated as one router with subcommands:

- `pulse:workflow onboard`
- `pulse:workflow explore`
- `pulse:workflow brainstorm`
- `pulse:workflow plan`
- `pulse:workflow validate`
- `pulse:workflow swarm`
- `pulse:workflow execute`
- `pulse:workflow review`
- `pulse:workflow compound`


### Standalone utility skills (packaged outside `pulse:workflow`)

- `architecture-rescue`
- `systematic-debug-fix`
- `dev-note`
- `dev-note-distil`
- `prompt-leverage`

### Runtime command

Use `{{pulse_command}}` for status, readiness, and reservations, for example:

```bash
{{pulse_command}} ready --repo-root <repo> --json
{{pulse_command}} reservation list --repo-root <repo> --active-only --json
```

## Adding or Changing Router Commands

1. Update `skills/workflow/SKILL.md` if routing/help tables change.
2. Update `skills/workflow/references/<command>/command.md`.
3. Keep shared contracts under `skills/workflow/references/shared/`.
4. Update public docs when contract language changes:
   - [`README.md`](README.md)
   - [`CONTRIBUTING.md`](CONTRIBUTING.md)
   - [`AGENTS.md`](AGENTS.md)
   - [`PULSE_REBOOT.md`](PULSE_REBOOT.md)
   - [`pulse-reboot/`](pulse-reboot/)
5. Run checks and tests.

## Testing Changes

Minimum verification:

1. Install/update plugin in runtime.
2. Start a fresh session.
3. Trigger the command(s) you changed.
4. Confirm routing and behavior match command docs.

For runtime changes, verify:

- `pulse:workflow use` initializes expected `.pulse/runtime` and `.pulse/workgraph` layout when needed and restores session context.
- `pulse:workflow onboard` still performs explicit bootstrap/remediation correctly when invoked directly.
- `{{pulse_command}} status --repo-root <repo> --json` returns valid scout state.
- `{{pulse_command}} ready --repo-root <repo> --json` and reservation commands produce expected JSON outputs.

## Documentation Rules

- use repository-relative links for repo files
- external links are allowed for upstream/public references
- never commit absolute local filesystem paths
- verify links resolve
- treat docs/contract drift as a bug

Run:

```bash
bash scripts/check-markdown-links.sh
```
