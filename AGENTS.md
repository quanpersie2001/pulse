# AGENTS.md — Pulse Skill Ecosystem

Read this file at every session start. Re-read after any context compaction.

## What is Pulse?

A validate-first, docs-first skill ecosystem for agentic software delivery, built on the Flywheel toolchain (`br`, `bv`, runtime-native swarm adapters, and local reservations). Skills chain together to move from vague requirements to shipped, reviewed, compounded code.

## What Pulse Is / Is Not

Pulse is:

- a gated workflow with explicit human approvals and repo artifacts
- a skill plugin for Claude Code and Codex, not a separate compiled runtime
- a delivery system that can run in swarm or single-worker mode

Pulse is not:

- a license for agents to skip `CONTEXT.md`, validating, or review gates
- a promise that every task needs the heaviest possible process path
- a replacement for human gate approval or operator judgment

## One-Line Glossary

- `CONTEXT.md` — locked decisions that downstream work must honor.
- `work-shape.md` / `phase-plan.md` / `epic-map.md` — approved planning shape artifact.
- current-work artifact — execution-ready slice (`work-shape.md` section, `current-story-pack.md`, or `phase-<n>-contract.md` + `phase-<n>-story-map.md`).
- story map — the reason beads are sequenced the way they are.
- bead — one worker-sized unit of work with exact files and checks.
- handoff — the pause/resume contract for the next actor.
- `pulse_status` — the read-only scout for current workflow state.

## Skill Catalog (core chain + selected support)

| Skill | Purpose | Invoke When |
|-------|---------|-------------|
| `pulse:using-pulse` | Bootstrap/meta — routing, go mode, state bootstrap | Session start, "build feature X" |
| `pulse:preflight` | Tool readiness, mode selection, project tooling status | Before planning or execution |
| `pulse:exploring` | Extract decisions via Socratic dialogue → CONTEXT.md | New feature, unclear requirements |
| `pulse:planning` | Route mode → shape → current work; synthesize artifacts and bead timing | After exploring, with CONTEXT.md |
| `pulse:validating` | Feasibility/readiness verification + spikes + constrained routing | After planning, before execution |
| `pulse:swarming` | Launch + tend parallel worker agents | After validating approves beads |
| `pulse:executing` | Per-agent worker loop (report online → implement → close) | Loaded by workers spawned by swarming |
| `pulse:reviewing` | Specialist reviewers + 3-level verification + UAT + finishing | After swarming completes all beads |
| `pulse:compounding` | Capture learnings → .pulse/memory/learnings/ | After reviewing, always |
| `pulse:writing-pulse-skills` | TDD-for-skills meta-skill | Creating/improving pulse skills |

### Support Skills

| Skill | Purpose |
|-------|---------|
| `pulse:systematic-debug-fix` | Root-cause-first bug fixing when workers hit blockers |
| `pulse:gitnexus` | Codebase intelligence via GitNexus graph tools or local fallback |
| `pulse:architecture-rescue` | Repo-wide or subsystem-wide architecture rescue report on shallow modules, leaky seams, ownership drift, and deepening opportunities |
| `pulse:bootstrap-project-context` | Repo bootstrap and project-doc discovery/mapping before implementation in an unfamiliar repo |
| `pulse:refresh-project-docs` | Regenerate evergreen project docs to match the current repo state |
| `pulse:dev-note` | Capture one structured developer learning from the current coding-with-AI session |
| `pulse:dev-note-distil` | Distill pending raw dev notes into stable topic knowledge and rebuild the global topic index |
| `pulse:dream` | Manual dream consolidation over Claude Code or Codex runtime artifacts |

## The Chain

```
pulse:preflight → pulse:using-pulse → pulse:exploring → pulse:planning → pulse:validating → pulse:swarming → pulse:executing(×N) → pulse:reviewing → pulse:compounding
```

## Go Mode Gates

- **GATE 1** (after exploring): approve `CONTEXT.md` before planning
- **GATE 2** (after planning): approve the selected shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`)
- **GATE 3** (after validating): approve feasibility-validated current work before swarming or single-worker execution
- **GATE 4** (after reviewing): approve merge; P1 findings block this gate

## Core Tools

- `br` — beads CLI (create/update/close work items)
- `bv` — beads viewer (graph analytics, priority routing)
- native swarm adapters — Claude Code teammates or Codex subagents in swarm mode
- `.pulse/scripts/pulse_reservations.mjs` — shared local file reservations for swarm safety
- `gitnexus` — graph-backed codebase intelligence (optional)
- CASS/CM — session search, cognitive memory (optional)

## 3-Plane Model

Use this mental model when deciding where to read next:

1. **Control plane — `.pulse/`**: live workflow state, routing, handoffs, and active operator surfaces.
2. **Memory plane — `.pulse/memory/`**: shared root for reusable cross-feature memory, including critical patterns, learnings, corrections, and ratchet artifacts.
3. **Feature record plane — `history/`**: feature-specific decisions, plans, contracts, story maps, canonical verification evidence, and durable audit narrative.

Downstream repos may also expose a separate project-docs plane through repo-owned `CONTEXT.md`, `CONTEXT-MAP.md`, and `docs/adr/`; Pulse maps those inputs via `.pulse/project-docs.json` and keeps them distinct from feature history.

## File Conventions

```
.pulse/STATE.md                 ← Working memory
.pulse/config.json              ← Feature toggles (absent=enabled)
.pulse/tooling-status.json      ← Preflight result + recommended mode
.pulse/project-docs.json        ← Mapping artifact for repo-owned project docs
.pulse/handoffs/manifest.json   ← Owner-scoped handoff index
.pulse/handoffs/*.json          ← Per-owner pause/resume state
history/<feature>/verification/      ← Canonical verification evidence for the current feature
.pulse/memory/                  ← Shared reusable memory root
history/<feature>/              ← Per-feature artifacts, including lifecycle-summary.md and canonical verification evidence
.beads/                         ← Bead files
.spikes/                        ← Spike verification results
```

## Critical Rules

1. **Never execute without validating.** GATE 3 is non-negotiable.
2. **`CONTEXT.md` is the source of truth.** All downstream agents honor locked decisions.
3. **Context budget: >65% → write an owner-scoped handoff and register it in `.pulse/handoffs/manifest.json`.**
4. **Scout first, drill down second.** Prefer `pulse_status`, then open the specific artifact it points to.
5. **After compaction: re-read this file + `CONTEXT.md` immediately.**
6. **P1 findings always block merge.** Even in go mode.

---

## Native Swarm Coordination

Pulse keeps one swarm contract and adapts it to the active runtime.

### Runtime adapters

- **Claude Code** — use teammate primitives such as `TeamCreate`, `Agent`, and `SendMessage` when swarm mode is active.
- **Codex** — use native subagents and parent-thread follow-ups.
- **Shared reservation layer** — every runtime uses `.pulse/scripts/pulse_reservations.mjs` and `.pulse/reservations.json` to prevent overlapping edits.

### Same Repository Workflow

When multiple agents operate in the same repository:

1. **Spawn with bounded context** — workers receive runtime identity, coordinator identity, epic ID, feature name, and an optional startup hint.
2. **Reserve files** — before editing, a worker claims the files listed in its bead's `files` field through the local reservation helper.
3. **Report on the active coordination surface** — workers post `[ONLINE]`, `[DONE]`, `[BLOCKED]`, `[FILE CONFLICT]`, and `[HANDOFF]` as state changes happen.

### Shared rules

- beads plus `bv` remain the source of truth for work selection
- do not edit without reservations
- do not invent extra registration, inbox, or topic mechanics when the runtime does not use them
- release reservations before advertising completion or pause

### Quick Reads

- Scout: `node .pulse/scripts/pulse_status.mjs --json`
- Reservations: `node .pulse/scripts/pulse_reservations.mjs list --active-only --json`
- Graph: `bv --robot-triage --graph-root <EPIC_ID>`

### Macros vs Granular Tools

Some runtimes expose high-level worker-spawn macros. Others expose a smaller set of teammate or subagent primitives. Pulse skills reference the behavior contract; keep the behavior, not the spelling.

### Common Pitfalls

- **Forgetting to release files.** Always release reservations after closing a bead or pausing.
- **Skipping `[ONLINE]`.** Startup is not complete until the worker reports in.
- **Treating tasks or chat threads as the work graph.** Beads plus `bv` stay authoritative.
- **Editing around a conflict.** Report `[FILE CONFLICT]` and wait for resolution.

---

<!-- bv-agent-instructions-v1 -->

## Beads Workflow Integration

This project uses [beads_rust](https://github.com/Dicklesworthstone/beads_rust) for issue tracking. Durable issue state lives in `.beads/issues.jsonl` and `.beads/config.yaml`.

**Important:** `br` is non-invasive — it NEVER runs git commands automatically. After `br sync --flush-only`, stage durable Beads files explicitly before committing. Do not stage SQLite cache/sidecar or recovery artifacts such as `.beads/beads.db`, `.beads/beads.db-wal`, `.beads/beads.db-shm`, or `.beads/.br_recovery/`.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
br ready              # Show issues ready to work (no blockers)
br list --status=open # All open issues
br show <id>          # Full issue details with dependencies
br create --title="..." --type=task --priority=2
br update <id> --status=in_progress
br close <id> --reason="Completed"
br close <id1> <id2>  # Close multiple issues at once
br sync --flush-only  # Flush bead changes to disk (does NOT run git)
```

### Workflow Pattern

1. **Scout**: Run `node .pulse/scripts/pulse_status.mjs --json` when available, then `br ready` to find actionable work
2. **Claim**: Use `br update <id> --status=in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `br close <id>`
5. **Sync**: Always run `br sync --flush-only` then manually commit at session end

### Key Concepts

- **Dependencies**: Issues can block other issues. `br ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `br dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status                                      # Check what changed
git add <files>                                 # Stage code changes
br sync --flush-only                            # Flush bead changes to disk
git add .beads/issues.jsonl .beads/config.yaml # Stage durable bead state only
git commit -m "..."                             # Commit code + beads together
git push                                        # Push to remote
```

### Best Practices

- Check `br ready` at session start to find available work
- Update status as you work (in_progress → closed)
- Create new issues with `br create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `br sync --flush-only`, stage `.beads/issues.jsonl` and `.beads/config.yaml` explicitly, then commit before ending session

<!-- end-bv-agent-instructions -->

---

## Beads Viewer (bv) — Graph-Aware Triage Engine

`bv` is the graph-aware viewer and analytics engine for the bead dependency graph. It provides TUI mode for humans and structured robot-output mode for agents.

> **Scope boundary:** `bv` reads and analyzes the bead graph. It does not create, update, or close beads — use `br` for mutations.

### CRITICAL: Use ONLY `--robot-*` Flags

In automated or agent sessions, never launch the TUI. Always use robot-output flags that return structured JSON:

```bash
bv --robot-triage              # Prioritized triage of actionable work
bv --robot-priority            # Priority-ranked list of all beads
bv --robot-suggest             # Dependency suggestions and fixes
bv --robot-insights            # Graph health analysis
bv --robot-triage --graph-root <ID>  # Scoped to a specific epic/subtree
```

### The Workflow: Start With Triage

Every planning, validation, or execution session should begin with:

```bash
bv --robot-triage
```

This returns the most actionable view: beads sorted by priority with dependency status, blockers surfaced, and ready work highlighted.

### Command Reference

#### Planning Commands

| Command | Purpose |
|---------|---------|
| `bv --robot-triage` | Actionable work, sorted by priority, blockers surfaced |
| `bv --robot-priority` | All beads ranked by priority score |
| `bv --robot-suggest` | Suggested dependency additions or fixes |

#### Graph Analysis Commands

| Command | Purpose |
|---------|---------|
| `bv --robot-insights` | Graph health: cycles, bottlenecks, orphans, disconnected clusters |
| `bv --robot-triage --graph-root <ID>` | Triage scoped to a specific epic subtree |

#### History Commands

| Command | Purpose |
|---------|---------|
| `bv --robot-triage --include-closed` | Include closed beads in triage output |
| `bv --robot-priority --include-closed` | Full priority list including closed work |

#### Other Commands

| Command | Purpose |
|---------|---------|
| `bv --help` | Full flag reference |
| `bv` | Launch TUI (humans only, never in agent sessions) |

### Scoping & Filtering

Scope triage to a specific epic or subtree:

```bash
# Triage only beads under a specific epic
bv --robot-triage --graph-root EPIC-001

# Priority ranking within a subtree
bv --robot-priority --graph-root EPIC-001

# Insights for a specific subgraph
bv --robot-insights --graph-root EPIC-001
```

### Understanding Robot Output

All `--robot-*` commands return JSON. Key fields:

- `id` — bead identifier
- `title` — human-readable summary
- `status` — open, in_progress, closed, blocked
- `priority` — numeric (P0-P4)
- `dependencies` — list of upstream bead IDs
- `blockers` — unresolved blocking beads
- `files` — file paths in scope

### jq Quick Reference

```bash
# List IDs of all ready beads
bv --robot-triage | jq -r '.[] | select(.status == "open" and (.blockers | length == 0)) | .id'

# Count open beads by priority
bv --robot-priority | jq 'group_by(.priority) | map({priority: .[0].priority, count: length})'

# Find beads touching a specific file
bv --robot-priority | jq -r '.[] | select(.files[]? | contains("src/auth")) | .id'

# List all blocked beads and what blocks them
bv --robot-triage | jq '.[] | select(.blockers | length > 0) | {id, blockers}'
```

---

## Operator Cookbook

### Start a fresh Pulse session

1. Run `pulse:preflight`.
2. Run `pulse:using-pulse`.
3. Use `node .pulse/scripts/pulse_status.mjs --json` when available for a read-only scout pass.
4. Open only the artifacts the scout points at: usually `.pulse/state.json`, `.pulse/STATE.md`, `.pulse/handoffs/manifest.json`, `history/<feature>/CONTEXT.md`, and the selected shape/current-work artifacts.

### Resume safely

- If `.pulse/handoffs/manifest.json` exists, surface it before resuming.
- Do not auto-resume because a handoff exists; wait for confirmation.
- Rehydrate from the handoff, then re-check the current state mirrors before new edits.

### Pick swarm vs single-worker

- Prefer swarm only when approved current work has enough parallelizable beads to justify coordination overhead.
- Prefer single-worker when approved current work still needs Pulse discipline but not multiple active workers.
- In both paths, Gate 3 approval is still required before execution begins.

### Scout guidance

- Treat `pulse_status` as the first read, not the full source of truth.
- Treat it as an orientation surface that tells you where to look next.
- If the scout and deeper artifacts disagree, trust the underlying artifacts and surface the mismatch.

## Landing the Plane (Session Completion)

Every session must end cleanly. Before stopping, run through this mandatory workflow:

### 1. File Issues

Create beads for any discovered work that was not completed:

```bash
br create --title="<description>" --type=task --priority=<N>
```

### 2. Quality Gates

Confirm that no P1 findings remain unresolved:

```bash
bv --robot-triage | jq '.[] | select(.priority == 0 or .priority == 1) | select(.status != "closed")'
```

If P1 items remain open, they block merge. Fix or escalate before ending.

### 3. Update Status

Update `.pulse/STATE.md` with approved current work focus, completed work, and any open blockers.

### 4. Sync Beads

```bash
br sync --flush-only
git add .beads/issues.jsonl .beads/config.yaml
git commit -m "sync beads"
```

### 5. Hand Off

If context budget is near 65%:

1. Write an owner-scoped handoff to `.pulse/handoffs/<owner>.json`
2. Register it in `.pulse/handoffs/manifest.json`
3. Update `.pulse/STATE.md` with handoff reference

If the session is the final session for a feature:

1. Ensure all beads are closed or intentionally deferred
2. Run `pulse:compounding` to capture learnings
3. Push all changes: `git push`

---

## CASS — Cross-Agent Session Search

CASS (Cross-Agent Session Search) lets you find information from previous agent sessions. Use it to recover decisions, find prior approaches, or locate work done in earlier contexts.

### Examples

```bash
# Find sessions that worked on authentication
cass search "authentication login JWT"

# Find sessions that modified a specific file
cass search "src/auth/middleware.ts"

# Find sessions related to a specific bead
cass search "BEAD-042"
```

### Tips

- Use specific terms: file paths, bead IDs, feature names, error messages.
- Combine multiple search terms for narrower results.
- CASS indexes session transcripts, not code. It finds what agents discussed, not what they wrote.

### Usage Rules

- CASS is optional. If unavailable, fall back to reading `history/` and `.pulse/` artifacts directly.
- Do not use CASS as a substitute for reading `CONTEXT.md` or `STATE.md` — those are always the source of truth.
- CASS results are hints, not authoritative. Always verify findings against the current codebase state.

---

## Memory System: cass-memory

`cass-memory` (also called `cm`) is the persistent cognitive memory layer. It stores and retrieves durable knowledge across sessions — patterns, preferences, project-specific rules, and accumulated context.

### Quick Start

```bash
# Store a memory
cm store "br is the beads CLI binary, not bd"

# Recall memories relevant to a topic
cm recall "beads CLI commands"

# List recent memories
cm list --recent
```

### Protocol

1. **Store early.** When you discover a non-obvious fact, project convention, or user preference, store it immediately.
2. **Recall before assuming.** At session start or when entering unfamiliar territory, recall relevant memories before guessing.
3. **Trust but verify.** Memories may be stale. Cross-check recalled facts against the current codebase.

### Key Flags

| Flag | Purpose |
|------|---------|
| `--recent` | Show most recently stored memories |
| `--tag <tag>` | Filter by tag |
| `--context <text>` | Provide context for smarter recall |

### Rules

- `cass-memory` is optional. If unavailable, rely on `.pulse/memory/` and `.pulse/STATE.md`.
- File-based learnings under `.pulse/memory/` remain the canonical source of truth for compounded knowledge. `cass-memory` is a convenience index, not a replacement.
- Do not store secrets, credentials, or tokens in memory.

<!-- PULSE:START -->
# Pulse Workflow

Use `pulse:using-pulse` first in this repo unless you are resuming an already approved Pulse handoff.

## What Pulse Is / Is Not

Pulse is a validate-first, docs-first skill workflow for Claude Code and Codex.
Pulse is not a license to skip `CONTEXT.md`, validating, review gates, or human approval.

## One-Line Glossary

- `CONTEXT.md` — locked decisions downstream work must honor.
- `work-shape.md` / `phase-plan.md` / `epic-map.md` — approved planning shape artifact.
- current-work artifact — execution-ready slice (`work-shape.md` section, `current-story-pack.md`, or `phase-<n>-contract.md` + `phase-<n>-story-map.md`).
- story map — the reason beads are sequenced the way they are.
- bead — one worker-sized unit of work with exact files and checks.
- handoff — the pause/resume contract for the next actor.
- `pulse_status` — the read-only scout for current workflow state.

## Startup

1. Read this file at session start and again after any context compaction.
2. If preflight readiness is missing/stale/blocked in `.pulse/tooling-status.json`, stop and run `pulse:using-pulse` before continuing.
3. If `.pulse/scripts/pulse_status.mjs` exists, use `node .pulse/scripts/pulse_status.mjs --json` for a fast read-only status snapshot.
4. If `.pulse/handoffs/manifest.json` exists, do not auto-resume. Surface the saved state and wait for user confirmation.
5. If `.pulse/memory/critical-patterns.md` exists, read it before planning or execution work.

## Chain

```
pulse:preflight
  → pulse:using-pulse
  → pulse:exploring
  → pulse:planning
  → pulse:validating
  → pulse:swarming
  → pulse:executing
  → pulse:reviewing
  → pulse:compounding
```

## Critical Rules

1. Never execute without validating.
2. `CONTEXT.md` is the source of truth for locked decisions.
3. If context usage passes roughly 65%, write `.pulse/handoffs/manifest.json` and pause cleanly.
4. Treat `.pulse/state.json` as the routing mirror and `.pulse/STATE.md` as the human-readable narrative; keep them aligned.
5. After compaction, re-read `AGENTS.md`, run `node .pulse/scripts/pulse_status.mjs --json` if present, then re-open `.pulse/handoffs/manifest.json`, `.pulse/state.json`, `.pulse/STATE.md`, and the active feature context before more work.
6. P1 review findings block merge.

## 3-Plane Model

1. **Control plane — `.pulse/`**: live workflow state, routing mirrors, handoffs, and operator surfaces.
2. **Memory plane — `.pulse/memory/`**: shared root for reusable cross-feature memory, including critical patterns, learnings, corrections, and ratchet artifacts.
3. **Feature record plane — `history/`**: feature-specific decisions, plans, contracts, story maps, and durable narrative.

## Working Files

```
.pulse/
  onboarding.json     ← onboarding state for the Pulse plugin
  state.json          ← machine-readable routing/status mirror
  STATE.md            ← approved current-work focus
  handoffs/
    manifest.json     ← pause/resume artifact
  memory/             ← shared reusable memory root
    critical-patterns.md ← globally promoted patterns
    learnings/          ← durable cross-feature learning entries
    corrections/        ← durable corrections to prior guidance
    ratchet/            ← durable quality bars and non-regression rules

history/<feature>/
  CONTEXT.md          ← locked decisions
  discovery.md        ← research findings
  approach.md         ← approach + risk map

.beads/               ← bead/task files when beads are in use
.spikes/              ← spike outputs when validation requires them
```

## Operator Cookbook

### Startup scout

1. Run `pulse:using-pulse` if preflight readiness is missing, stale, or blocked.
2. Run `node .pulse/scripts/pulse_status.mjs --json` when available.
3. Use the scout to choose the next artifact instead of opening everything at once.

### Resume scout

- If `.pulse/handoffs/manifest.json` exists, surface it and wait for explicit confirmation.
- Re-open the handoff plus `.pulse/state.json` and `.pulse/STATE.md` before continuing.
- If current state and a handoff disagree, surface the mismatch instead of guessing.

### Swarm vs single-worker

- Use swarm when approved current work has enough parallelizable beads to justify coordination overhead.
- Use single-worker when approved current work still needs Pulse discipline but parallelism is not.
- Gate 3 still blocks both modes until validating approves execution.

## Codex Guardrails

- Repo-local `.codex/` files installed by Pulse are workflow guardrails, not optional decoration.
- Use `node .pulse/scripts/pulse_status.mjs --json` as the preferred quick scout step when it is available.
- Treat `compact_prompt` recovery instructions as mandatory.
- Use `bv` only with `--robot-*` flags. Bare `bv` launches the TUI and should be avoided in agent sessions.
- If the repo is only partially onboarded, stay in bootstrap/planning mode and surface what is missing before implementation.

## Session Finish

Before ending a substantial Pulse work chunk:

1. Update or close the active bead/task if one exists.
2. Leave `.pulse/state.json`, `.pulse/STATE.md`, and `.pulse/handoffs/` consistent with the current pause/resume state.
3. Mention any remaining blockers, open questions, or next actions in the final response.
<!-- PULSE:END -->

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **pulse** (4092 symbols, 6052 relationships, 256 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

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
