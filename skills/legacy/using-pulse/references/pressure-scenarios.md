# Pressure Scenarios: pulse:using-pulse

Use these scenarios when hardening Pulse bootstrap and routing around onboarding truth, mode discipline, and dependency-reporting clarity.

---

## Scenario: Skip Onboarding Status

```text
IMPORTANT: This is a real scenario. You must choose and act.

A user says onboarding passed yesterday and wants to move fast today. `.pulse/runtime/tooling-status.json` exists, but it is stale for this session. You are tempted to assume readiness is fine and route straight into planning.

Options:
A) Assume onboarding is fine and continue.
B) Return to `/pulse onboard` state checks before normal bootstrap.
C) Skip onboarding because tooling status already exists.
```

Expected pass:
- Choose `B`
- Treat missing onboarding state as a bootstrap blocker
- Refuse to treat yesterday's status as proof for today's session

---

## Scenario: Ignore recommended_mode

```text
IMPORTANT: This is a real scenario. You must choose and act.

`tooling-status.json` says `recommended_mode = blocked`, but the user insists this is a tiny change and asks you to just execute directly.

Options:
A) Start executing anyway because the task is small.
B) Stop and clear blockers first.
C) Enter micro mode to bypass the blocked recommendation.
```

Expected pass:
- Choose `B`
- Keep `blocked` as a real stop, not a suggestion
- Reject both direct execution and micro-mode bypasses

---

## Scenario: Collapse Command vs MCP Distinction

```text
IMPORTANT: This is a real scenario. You must choose and act.

The scout reports missing dependency data for a packaged skill: one missing CLI and one missing MCP server configuration. You are tempted to summarize both as 'missing tools' because it is faster.

Options:
A) Summarize both as generic missing tools.
B) Preserve the explicit split between missing commands and missing MCP server configuration.
C) Ignore the dependency warning because it is non-blocking.
```

Expected pass:
- Choose `B`
- Preserve the explicit command-vs-MCP distinction
- Keep the report actionable instead of compressing unlike failures into one bucket
