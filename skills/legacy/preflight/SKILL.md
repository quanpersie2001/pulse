---
name: preflight
description: Use when starting any Pulse workflow, resuming a Pulse session, or before planning or execution when tool readiness or onboarding state may block the flow.
metadata:
  version: '1.2'
  ecosystem: pulse
  position: '0 - runs before using-pulse or any execution-capable phase'
  type: bootstrap
  dependencies: []
---

# Preflight

Validate runtime readiness before Pulse spends planning or execution effort.
This skill is the sole authority for onboarding, tool-health, and readiness decisions in a Pulse session.
`pulse:using-pulse` consumes the artifacts written here and routes next steps, but it does not rerun onboarding or readiness checks.
Do not explore, plan, validate, swarm, or execute until this skill finishes.

## When to Use

- User says: "start Pulse", "run Pulse", "/go", "resume Pulse"
- Before any full Pulse workflow
- Before execution if tool availability is uncertain
- After environment changes: new machine, new shell, new repo clone, broken tooling

## What This Skill Produces

- `.pulse/tooling-status.json`
- `.pulse/state.json`
- `.pulse/STATE.md` update with the latest preflight result
- One of three outcomes: `PASS`, `DEGRADED`, or `FAIL`

Load `references/tool-readiness-matrix.md` before Phase 3.

## Hard Rules

1. Never assume a tool exists because a repo or prior session mentioned it.
2. Never report `PASS` without running a real minimal check for each required tool.
3. If a tool is optional, record it as unavailable but do not fail the run.
4. If a tool is required for the requested mode, block that mode explicitly.
5. If the result is `FAIL`, do not continue into planning or execution.
6. If the result is `DEGRADED`, present the downgrade clearly before proceeding.

## Phase 1: Establish Pulse State

1. Resolve the project root.
2. Ensure `.pulse/` exists. Create it if missing.
3. Ensure `.pulse/handoffs/` exists. Create it if missing.
4. Ensure `.pulse/STATE.md` exists. If missing, create:

```markdown
# STATE
focus: (none)
phase: preflight
last_updated: <timestamp>
```

5. Ensure `.pulse/handoffs/manifest.json` exists. If missing, create:

```json
{
  "schema_version": "1.0",
  "updated_at": "<ISO-8601>",
  "active": []
}
```

6. Detect whether `.pulse/handoffs/manifest.json` contains active resume entries. Record that resume data exists, but do not auto-resume.
7. Infer the requested mode:
   - `full-pipeline`
   - `planning-only`
   - `execution-only`
   - `resume`

## Phase 2: Check Onboarding

Verify that the Pulse plugin is properly onboarded in the current repo.

Run `node --version` first.

- If `node` is missing or reports a version below 18: set `onboarding` to `NEEDS_SETUP` and include a blocker entry: "Node.js 18+ required for onboarding — install or upgrade before continuing." Do not proceed to the onboarding script.

If Node.js 18+ is available, run the onboarding status check from this skill's directory. `needs_onboarding` is a remediation path for stale installs, not the normal Pulse bootstrap result:

```bash
node skills/using-pulse/scripts/onboard_pulse.mjs --repo-root <project_root>
```

Interpret the result:

- `status = "up_to_date"` — Pulse onboarding is current. Set `onboarding = PASS`.
- `status = "needs_onboarding"` — the repo needs Pulse onboarding or remediation updates before normal bootstrap can continue. Use `actions` plus any relevant `details` to explain whether this is missing repo-local assets, stale `.codex/hooks.json` Pulse entries, legacy `.codex/hooks/*` or `.codex/pulse_*.mjs` cleanup, legacy Python hook files, legacy verification/memory layout, or a partial Pulse install.
- `status = "missing_runtime"` — Node.js 18+ is not available, so onboarding cannot run. Set `onboarding = NEEDS_SETUP`.

For `needs_onboarding`:

1. Summarize what the script wants to create or update from `actions`.
2. If `requires_confirmation = true`, explain that an existing `compact_prompt` was found and Pulse will preserve it unless the user explicitly approves replacement.
3. Ask the user before making any repo changes.
4. After approval, run: `node skills/using-pulse/scripts/onboard_pulse.mjs --repo-root <project_root> --apply`
   - Only pass `--allow-compact-prompt-replace` when the user explicitly approved replacing the existing compaction prompt.
5. If the apply run succeeds, update `onboarding` to `PASS` and continue.
6. If the apply run fails, keep `onboarding` blocked and add a blocker entry.

An unresolved onboarding issue or missing runtime sets preflight result to `FAIL`.

## Phase 3: Validate Core Tooling

Check the tools that Pulse depends on structurally:

- `git`
- `br`
- `bv`

Use the smallest real checks that prove the tool is callable. Recommended checks:

```bash
command -v git
command -v br
command -v bv
git rev-parse --show-toplevel
br --help
bv --help
```

Rules:

- If `git` is missing or the project is not a git repo, result is `FAIL`
- If `br` is missing, result is `FAIL`
- If `bv` is missing, result is `FAIL`
- If a command exists but the minimal call errors, mark it unavailable
- If `br` or `bv` is unavailable, include the install references from `references/tool-readiness-matrix.md` in the failure report

## Phase 4: Validate Execution Runtime

If the requested mode can lead to execution, validate the active runtime path.

Primary concerns:

- native swarm capability for the current CLI runtime
- shared reservation helper readiness for swarm execution

Run the smallest real health check available in the current environment.
Examples vary by runtime:

- Claude Code: confirm teammate primitives needed for swarming are available in this session
- Codex: confirm native subagent flow is available in this session
- reservation helper: confirm onboarding installed `.pulse/scripts/pulse_reservations.mjs`

If no real health-check primitive exists, mark swarm capability as unavailable.
Do not infer readiness from environment variables alone.

Decision rules:

- If requested mode is `full-pipeline` and swarm execution is expected:
  - native swarm + reservations ready -> keep `swarm`
  - unavailable -> set result to `DEGRADED` and recommend `single-worker`
- If requested mode is `execution-only` and the user explicitly asked for swarm mode:
  - unavailable -> stop and present the downgrade decision

## Phase 5: Validate Optional Accelerators

Check optional helpers if relevant:

- `gitnexus`
- PR tooling such as `gh`
- docs or web research MCPs

Optional tools never fail preflight on their own.
Record impact instead:

- `gitnexus` missing -> discovery falls back to grep/find/manual reads
- `gh` missing -> no automated PR creation
- docs or web MCP missing -> research becomes manual or local-only

## Phase 6: Decide Outcome

Choose exactly one result label:

- `PASS`
  - all required tools for the requested mode are ready
- `DEGRADED`
  - core tools are ready
  - at least one optional capability is missing, or swarm must downgrade to single-worker
- `FAIL`
  - any required tool for the requested mode is unavailable
  - onboarding is `NEEDS_SETUP` or `NEEDS_ONBOARDING` and could not be resolved in Phase 2

Normalize status casing explicitly:
- `.pulse/tooling-status.json` uses lowercase machine values: `pass | degraded | fail`
- `.pulse/state.json`, `.pulse/STATE.md`, and user-facing summaries may use uppercase labels: `PASS | DEGRADED | FAIL`

Also choose `recommended_mode`:

- `swarm`
- `single-worker`
- `planning-only`
- `blocked`

## Phase 7: Write Artifacts

Write `.pulse/tooling-status.json` with this shape:

```json
{
  "timestamp": "<ISO-8601>",
  "project_root": "<absolute path>",
  "requested_mode": "full-pipeline",
  "recommended_mode": "single-worker",
  "status": "degraded",
  "onboarding": "PASS",
  "tools": {
    "git": { "status": "ready", "check": "git rev-parse --show-toplevel" },
    "br": { "status": "ready", "check": "br --help" },
    "bv": { "status": "ready", "check": "bv --help" },
    "swarm_runtime": { "status": "unavailable", "check": "<actual runtime capability probe or 'none available'>" },
    "reservations": { "status": "ready", "check": "test -f .pulse/scripts/pulse_reservations.mjs" }
  },
  "blockers": [],
  "degradations": [
    "Native swarm path unavailable; execution downgraded to single-worker"
  ],
  "next_skill": "pulse:using-pulse"
}
```

Also refresh `.pulse/state.json` as the lightweight routing mirror:

```json
{
  "phase": "preflight",
  "status": "PASS | DEGRADED | FAIL",
  "requested_mode": "<mode>",
  "recommended_mode": "<mode>",
  "tooling_status": ".pulse/tooling-status.json"
}
```

Update `.pulse/STATE.md` with:

```markdown
phase: preflight
preflight_status: PASS | DEGRADED | FAIL
requested_mode: <mode>
recommended_mode: <mode>
tooling_status: .pulse/tooling-status.json
resume_manifest: .pulse/handoffs/manifest.json
last_updated: <timestamp>
```

## Phase 8: Present Result

Use this response shape:

```text
PREFLIGHT COMPLETE

Status: PASS | DEGRADED | FAIL
Requested mode: <mode>
Recommended mode: <mode>
Onboarding: PASS | NEEDS_SETUP | NEEDS_ONBOARDING

Ready:
- <tool>
- <tool>

Missing or degraded:
- <tool> -> <impact>

Next:
- If PASS or DEGRADED: invoke pulse:using-pulse
- If FAIL: fix blockers before continuing
```

## Handoff

- `PASS` -> "Preflight complete. Pulse is ready. Invoke `pulse:using-pulse`."
- `DEGRADED` -> "Preflight complete with downgrade. Confirm the downgraded mode, then invoke `pulse:using-pulse`."
- `FAIL` -> "Preflight failed. Do not continue into Pulse flow until blockers are cleared."

## Red Flags

- Marking `PASS` without running actual checks
- Treating swarm runtime as optional in swarm mode
- Continuing after `FAIL`
- Hiding degraded mode from the user
- Reusing stale `.pulse/tooling-status.json` without refreshing it
- Using environment hints instead of real tool invocations

## References

- `references/tool-readiness-matrix.md`
- `../using-pulse/references/handoff-contract.md`
