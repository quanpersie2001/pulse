# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Repo Is

Pulse is a packaged skill plugin for Claude Code and Codex. Its public workflow surface is a single router skill: `pulse:workflow`. Runtime operations are handled by the `pulse-work` CLI and repo-local Node helpers.

## Repository Layout

- `skills/workflow/` — canonical source for the Pulse router skill and subcommands
- `.codex-plugin/plugin.json` — Codex plugin manifest
- `.claude-plugin/plugin.json` — Claude plugin manifest
- `.mcp.json` — packaged MCP manifest for shared runtime servers
- `.agents/plugins/marketplace.json` — Codex marketplace metadata
- `scripts/sync-skills.sh` — raw skill mirror helper for agents/Claude compatibility
- `AGENTS.md` — operator workflow rules
- `references/` — upstream/pedagogical material; not part of the shipped plugin contract

## Key Tools

| Tool | CLI | Purpose |
|------|-----|---------|
| Pulse Router | `pulse:workflow` | User-facing workflow entrypoint |
| Pulse Runtime CLI | `pulse-work` | Workgraph/runtime metadata mutations |
| Scout | `node {{scripts_path}}/pulse_status.mjs --json` | Read-only runtime orientation |
| Git | `git` | Version control |
| Native swarm adapters | — | Claude Code teammates or Codex subagents |
| GitNexus | `gitnexus` | Optional graph-backed codebase intelligence |

## Delivery Chain

The core workflow is a gated, linear pipeline:

```
pulse:workflow onboard → pulse:workflow explore → pulse:workflow plan → pulse:workflow validate → pulse:workflow swarm or pulse:workflow execute → pulse:workflow review → pulse:workflow compound
```

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

Pulse has automated coverage for onboarding/runtime control-plane behavior in `skills/workflow/tests/`. The `references/superpowers/tests/brainstorm-server/` suite is reference material and not part of the shipping plugin.

## Session Protocol

```bash
git status
# edit files
node {{scripts_path}}/pulse_status.mjs --json
pulse-work ready --json
git add <files>
git commit -m "..."
git push
```

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **pulse** (3504 symbols, 5096 relationships, 208 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/pulse/context` | Codebase overview, check index freshness |
| `gitnexus://repo/pulse/clusters` | All functional areas |
| `gitnexus://repo/pulse/processes` | All execution flows |
| `gitnexus://repo/pulse/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
