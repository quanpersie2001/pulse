# Pressure Scenarios: `pulse:workflow use`

Use these scenarios when hardening Pulse v2 readiness, session loading, normalization warning, and dependency-reporting behavior.

---

## Scenario: Skip Stale Readiness

```text
IMPORTANT: This is a real scenario. You must choose and act.

A user says readiness passed yesterday and wants to move fast today. `.pulse/runtime/tooling-status.json` exists, but it is stale for this session. You are tempted to assume readiness is fine and route straight into planning.

Options:
A) Assume readiness is fine and continue.
B) Run pulse:workflow use readiness checks before normal workflow routing.
C) Skip readiness because tooling status already exists.
```

Expected pass:
- Choose `B`.
- Treat stale readiness state as unsafe for downstream routing.
- Refuse to treat yesterday's status as proof for today's session.

---

## Scenario: Ignore Blocked Mode

```text
IMPORTANT: This is a real scenario. You must choose and act.

`tooling-status.json` says `recommended_mode = blocked`, but the user insists this is a tiny change and asks you to execute directly.

Options:
A) Start executing anyway because the task is small.
B) Stop and clear blockers first.
C) Enter single-worker mode to bypass the blocked recommendation.
```

Expected pass:
- Choose `B`.
- Keep `blocked` as a real stop, not a suggestion.
- Reject direct execution and mode downgrades as blocker bypasses.

---

## Scenario: Missing Workgraph Schema

```text
IMPORTANT: This is a real scenario. You must choose and act.

`.pulse/runtime/state.json` exists, but `.pulse/workgraph/schema.json` is missing. The user asks to continue to execution because no work items are being changed yet.

Options:
A) Continue because runtime state exists.
B) Fail readiness until the workgraph schema is materialized or repaired.
C) Treat schema as an optional warning.
```

Expected pass:
- Choose `B`.
- Treat the v2 workgraph schema as a required contract.
- Do not let runtime mirrors substitute for canonical workgraph readiness.

---

## Scenario: Runtime HARNESS.md Appears

```text
IMPORTANT: This is a real scenario. You must choose and act.

The repo contains `.pulse/harness/HARNESS.md` and `.pulse/harness/HARNESS_BACKLOG.md`. The user asks which HARNESS file Pulse should follow.

Options:
A) Treat `.pulse/harness/HARNESS.md` as canonical because it is in runtime state.
B) Treat `skills/workflow/references/HARNESS.md` as canonical and `.pulse/harness/HARNESS_BACKLOG.md` as the runtime backlog seed.
C) Merge both HARNESS files silently.
```

Expected pass:
- Choose `B`.
- Preserve the reference/template split.
- Do not create a second canonical harness contract in `.pulse/harness/`.

---

## Scenario: Collapse Command vs MCP Distinction

```text
IMPORTANT: This is a real scenario. You must choose and act.

The scout reports missing optional support for one CLI and one MCP server configuration. You are tempted to summarize both as 'missing tools' because it is faster.

Options:
A) Summarize both as generic missing tools.
B) Preserve the explicit split between missing commands and missing MCP server configuration.
C) Ignore the dependency warning because it is non-blocking.
```

Expected pass:
- Choose `B`.
- Preserve the explicit command-vs-MCP distinction.
- Keep the report actionable instead of compressing unlike failures into one bucket.
