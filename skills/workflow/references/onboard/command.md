# `pulse:workflow onboard`

Readiness and bootstrap authority for Pulse v2.

This command owns onboarding, runtime health checks, and mode recommendation before any downstream `pulse:workflow` phase runs.

## Mission

Produce one trustworthy readiness outcome (`PASS`, `DEGRADED`, `FAIL`), persist it under `.pulse/runtime/`, and hand off to the right next command without hidden assumptions.

## When to run

Run `pulse:workflow onboard` when:

- starting a Pulse session in a repo
- resuming after environment/tool changes
- runtime artifacts are missing or stale
- any command reports readiness uncertainty
- execution posture (`swarm` vs `single-worker`) must be revalidated

## Inputs

- repo root
- requested intent when known: `full-pipeline | planning-only | execution-only | resume`
- explicit user requirement for swarm mode (if any)
- known failures since last successful onboard

## Required runtime artifacts

`pulse:workflow onboard` owns these canonical files:

- `.pulse/runtime/tooling-status.json`
- `.pulse/runtime/state.json`
- `.pulse/runtime/STATE.md`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/reservations.json`

It also verifies workgraph presence for runtime integrity:

- `.pulse/workgraph/items.jsonl`
- `.pulse/workgraph/schema.json`

## Outcome contract

Exactly one result:

- `PASS` — required baseline is healthy for requested mode
- `DEGRADED` — baseline is healthy but mode/capability is downgraded
- `FAIL` — required prerequisites are unresolved; stop workflow routing

Downstream commands consume this result. They must not rerun onboarding as a local workaround.

## Phase order (mandatory)

### Phase 1 — Establish canonical runtime plane

Ensure `.pulse/runtime/`, `.pulse/workgraph/`, and `.pulse/harness/` structures exist.

Ensure these files exist or are initialized safely:

- `.pulse/runtime/STATE.md`
- `.pulse/runtime/state.json`
- `.pulse/runtime/handoffs/manifest.json`
- `.pulse/runtime/reservations.json`
- `.pulse/runtime/tooling-status.json` (rewritten later in this run)

If handoff entries exist, surface them as advisory context only. Do not auto-resume.

### Phase 2 — Onboarding status first; apply only with explicit consent

1. Confirm Node runtime is callable and supported for Pulse scripts.
2. Run onboarding status check from `skills/workflow/scripts/onboard/onboard_pulse.mjs`.
3. If remediation is required:
   - summarize what will change
   - ask explicit approval before mutation
   - apply only after approval
4. If user declines required remediation, mark affected modes blocked.

Stopping rule:

- unresolved onboarding/remediation needs => `FAIL`

### Phase 3 — Validate required baseline commands and repo posture

Validate minimum required runtime foundations:

- git availability + valid git repository
- node availability
- repo-local Pulse runtime helper availability

Treat missing required dependencies as blockers.

### Phase 4 — Validate execution posture

Decide execution recommendation:

- `swarm` when runtime supports coordinated multi-agent execution
- `single-worker` when swarm capability is unavailable but execution is still safe
- `planning-only` when execution should not start yet
- `blocked` when prerequisites are unresolved

If the user explicitly requires swarm and swarm is unavailable, stop for explicit downgrade approval.

### Phase 5 — Detect legacy drift as migration warnings

Legacy artifacts are warnings unless they create conflicting active truth:

- `.beads/`
- `history/`
- references to `br`, `bv`, `preflight`, `using-pulse`, or `dream`

Do not treat legacy presence as active runtime authority.

### Phase 6 — Persist normalized status and mirrors

Write authoritative machine status to `.pulse/runtime/tooling-status.json`:

```json
{
  "timestamp": "<ISO-8601>",
  "project_root": "<absolute path>",
  "requested_mode": "full-pipeline",
  "recommended_mode": "single-worker",
  "status": "pass|degraded|fail",
  "onboarding": "PASS|NEEDS_SETUP|NEEDS_REMEDIATION",
  "tools": {},
  "blockers": [],
  "degradations": [],
  "warnings": [],
  "next_command": "explore"
}
```

Refresh routing mirror in `.pulse/runtime/state.json`:

```json
{
  "phase": "onboard",
  "status": "PASS|DEGRADED|FAIL",
  "requested_mode": "<mode>",
  "recommended_mode": "<mode>",
  "tooling_status": ".pulse/runtime/tooling-status.json"
}
```

Refresh `.pulse/runtime/STATE.md` with matching values.

### Phase 7 — Present actionable result

Return concise operator output:

- status
- requested/recommended mode
- blockers
- degradations
- migration warnings
- exact next command

## Stopping rules

Stop immediately when:

- node/runtime prerequisites are missing
- onboarding remediation is required but not completed
- required baseline checks fail
- user-required swarm cannot be honored and downgrade is not approved

## Guardrails

- never declare `PASS` without real checks
- never apply onboarding changes silently
- never continue routing after `FAIL`
- keep command-vs-MCP gaps explicit
- keep warnings separate from blockers

## Handoff guidance

- `PASS` -> route to `pulse:workflow explore` or `pulse:workflow brainstorm` based on request clarity
- `DEGRADED` -> proceed only inside recommended downgraded mode
- `FAIL` -> stop; remediate; rerun `pulse:workflow onboard`

## References

- `readiness.md`
- `migration-warnings.md`
- `../../shared/workflow-contract.md`
- `../../shared/handoff-and-resume.md`
