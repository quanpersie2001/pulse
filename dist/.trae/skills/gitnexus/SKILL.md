---
name: gitnexus
description: >-
  Use when Pulse planning or discovery needs graph-based architecture context,
  execution-flow tracing, symbol relationships, or blast-radius analysis beyond
  plain file search.
metadata:
  version: '1.0'
  ecosystem: pulse
  type: support
  dependencies:
    - id: gitnexus
      kind: mcp_server
      server_names: [gitnexus]
      config_sources: [repo_codex_config, global_codex_config, plugin_mcp_manifest]
      missing_effect: unavailable
      reason: gitnexus skill requires the GitNexus MCP server for graph-backed codebase intelligence queries.
---

# gitnexus

Codebase intelligence support skill for Pulse.

If preflight readiness is missing, stale, or blocked (check `.pulse/runtime/tooling-status.json`), stop and invoke `pulse:workflow use` first.

## Start With The Repo Scout

Do not start by guessing CLI subcommands or assuming GitNexus is configured in this repo.

Run:

```bash
node .trae/skills/pulse:workflow/scripts/pulse.mjs status --repo-root <repo> --json
```

Use the scout output as the source of truth for this repo/session:

- `gitnexus_readiness.configured = false`: do not force GitNexus; use the fallback section below.
- `gitnexus_readiness.configured = true`: use GitNexus MCP tools for discovery.
- Always read the scout's `recommended_action` before deciding whether to use GitNexus or the fallback.

In Pulse, readiness is exposed through the scout. Treat that as the normal operator path.

## What Is Reliable Here

Use GitNexus as a discovery accelerator, not as a replacement for reading files.

Strong, normal-path tools in this repo:

- `list_repos`
- `query`
- `context`
- `impact`
- `api_impact`
- `route_map`
- `shape_check`
- `detect_changes`
- `cypher` as an escalation tool only after the standard path is exhausted

Important supporting resources:

- `gitnexus://repo/{name}/context`
- `gitnexus://repo/{name}/process/{processName}`
- `gitnexus://repo/{name}/schema`

The practical rule is simple: use repo context plus `query` plus `context` plus `impact` first, then fall back to direct file reads whenever graph results are thin, ambiguous, or too broad for the decision you need to make. Only escalate to `cypher` after the standard path is exhausted and only after reading the schema resource first.

## Primary Discovery Path

Use this path by default during Pulse planning and other codebase discovery work.

### 0. Repo sanity check

If multiple indexed repos may be visible, use `list_repos` as a light sanity check, then read `gitnexus://repo/{name}/context` before relying on graph-backed discovery.

If the repo context says the index is stale and you need reliable graph-backed discovery, run `npx gitnexus analyze` before continuing.

### 1. `query`

Use first for unfamiliar areas after the repo sanity check. It is the best starting point for a compact architecture snapshot and execution-flow search.

Use it to answer:

- which modules and files matter for this feature
- which processes or execution flows are closest to the requested behavior
- where the strongest existing patterns already live before deeper reads

When discovery is being written down for planning, save the result or summary into the active planning evidence artifact selected by the workflow (for example `history/<feature>/CONTEXT.md`) under the matching architecture/discovery section.

### 2. `context`

Use immediately after `query` to inspect the strongest symbol candidates in depth.

Good uses:

- confirm callers and callees for a known function or class
- inspect inbound/outbound references before changing a shared symbol
- verify whether a returned process or symbol is actually relevant before relying on it

If `query` returns a process that matters to your discovery write-up, read the strongest `gitnexus://repo/{name}/process/{processName}` resource before summarizing the flow, then confirm the implementation in source files.

### 3. `impact`

Use when a planned change touches shared types, classes, services, or other symbols with meaningful blast radius.

Good uses:

- understand what breaks if a symbol changes
- find direct and indirect callers before refactoring
- confirm whether a change is local or cross-cutting

### 4. API relationship tools

Use these when the feature touches route handlers or response contracts:

- `api_impact`
- `route_map`
- `shape_check`

These are especially useful when planning needs to know which consumers depend on an API route and whether a response-shape change is safe.

### 5. `detect_changes`

Use after local modifications or when reviewing a risky diff to understand affected flows.

This is not the first discovery step. Use it once there is an actual diff or a concrete change candidate to inspect.

## Tool Guidance

### `list_repos`

Use as a light sanity check when the scout says GitNexus is configured and you need to confirm which indexed repos are visible.

Do not treat this as the primary readiness check. The scout comes first.

### `query`

Treat as the architecture/process discovery entry point.

Use it first before drilling into individual symbols when the area is unfamiliar.

### `context`

Treat as the strongest symbol-level evidence tool.

Use it after `query` to confirm the specific symbols, files, and reference structure that matter to the feature.

### `impact`

Treat as the change-safety tool.

Use it when a likely implementation path needs blast-radius evidence before planning or refactoring.

### `api_impact`, `route_map`, `shape_check`

Use these as targeted tools for route handlers and response contracts. They are not the default first step for general discovery.

### `detect_changes`

Use once there is a meaningful diff or changed-symbol set to inspect.

It is best for review, regression awareness, and pre-commit understanding — not for first-pass architecture mapping.

### `cypher`

Use only as an escalation tool when `query`, `context`, `impact`, and the relevant process resource still leave a graph question unresolved.

Before using it, read `gitnexus://repo/{name}/schema` first.

Do not jump to `cypher` as the first move just because it is more expressive.

## Pulse Workflow Fit

Use this skill mainly during `pulse:workflow plan` discovery work.

- `query` feeds the architecture snapshot.
- `context` plus direct file reads feed the existing-pattern evidence.
- `impact` feeds blast-radius and dependency notes when the plan changes shared code.
- `api_impact` and friends are optional spot tools for API-heavy work, not the backbone of every workflow.

If planning is producing a discovery/context artifact, keep the saved output concise and evidence-based:

- architecture snapshot section
- existing patterns section
- dependency notes section when blast radius, callers, or API consumers materially affect the plan

Do not dump raw tool output when a short grounded summary will do.

## Practical Fallback Without GitNexus

If the scout says GitNexus is not configured or not available in this session, use local inspection with `rg` and direct file reads.

Useful fallbacks:

- file inventory: `rg --files`
- narrow slice inventory: `rg --files | rg 'auth|router|db|queue'`
- symbol search: `rg -n "MySymbol|myFunction|authMiddleware" .`
- importer search: `rg -n "^import .*from ['\"].*target|require\\(.*target" .`
- definition search: `rg -n "export (async )?function|class |const .*=" .`

Then read the relevant files directly.

If planning is writing discovery/context output, note the fallback plainly in the active planning evidence artifact, for example:

> GitNexus was unavailable or not configured for this repo/session, so discovery used `rg` and direct file inspection.

## Guardrails

- Do not skip the scout-based readiness check.
- Do not let graph results outrank direct file evidence.
- Do not skip reading the actual files before code changes.
- Do not rely on one GitNexus result when the answer is ambiguous; confirm with direct reads or local search.
- Do not stall the workflow if GitNexus is unavailable; fall back and document that choice plainly.

## Handoff

```text
Codebase intelligence gathered. Record the durable findings in discovery artifacts and continue with pulse:workflow plan or the calling skill.
```