# Pulse Tool Readiness Matrix

Use this matrix during `pulse:preflight` to decide whether the run should `PASS`, `DEGRADED`, or `FAIL`.

## Required vs Optional

| Tool / Capability | Required For | Suggested Minimal Check | If Missing |
|---|---|---|---|
| `git` | Every Pulse run | `git rev-parse --show-toplevel` | `FAIL` |
| `br` | Every Pulse run that uses bead-backed planning/execution | `br --help` | `FAIL` |
| `bv` | Every Pulse run that prioritizes or validates bead graphs | `bv --help` | `FAIL` |
| Native swarm capability for the active runtime | Swarm execution | smallest real runtime capability check available | `DEGRADED` to `single-worker`, or `FAIL` if the user explicitly requires swarm |
| `.pulse/scripts/pulse_reservations.mjs` | Swarm execution | `test -f .pulse/scripts/pulse_reservations.mjs` | `DEGRADED` to `single-worker`, or `FAIL` if the user explicitly requires swarm |
| `gitnexus` | Accelerated graph-backed discovery only | verify configured MCP server in the scout/dependency report | continue with fallback |
| `gh` | PR automation only | `gh --version` | continue without PR automation |
| docs / web MCP | External research acceleration | runtime-specific health check | continue with local/manual research |

## Install References

If a required Pulse tool is missing, include the relevant install reference in the preflight report:

| Tool | Reference |
|---|---|
| `br` | [beads_rust](https://github.com/Dicklesworthstone/beads_rust) |
| `bv` | [beads_viewer](https://github.com/Dicklesworthstone/beads_viewer) |

## Outcome Rules

### PASS

Use `PASS` when all are true:

- `git` is ready
- `br` is ready
- `bv` is ready
- all tools required for the requested mode are ready

### DEGRADED

Use `DEGRADED` when all are true:

- core tools are ready
- the requested mode can still proceed safely
- one or more capabilities are missing and Pulse must adjust behavior

Common degraded examples:

- native swarm capability unavailable -> use `single-worker`
- reservation helper unavailable -> use `single-worker`
- `gitnexus` unavailable -> use grep/find/manual discovery
- `gh` unavailable -> skip PR automation

### FAIL

Use `FAIL` when any are true:

- `git`, `br`, or `bv` is unavailable
- project root cannot be resolved as a git repo
- the user explicitly requested a mode whose required runtime is unavailable and no downgrade is acceptable

## Fallback Rules

### No Native Swarm Capability

If the runtime cannot safely support swarming:

- do not launch swarm workers
- recommend `single-worker`
- surface the downgrade before continuing

### No Reservation Helper

If `.pulse/scripts/pulse_reservations.mjs` is unavailable:

- do not launch swarm workers
- recommend `single-worker`
- repair onboarding before retrying swarm mode

### No `gitnexus`

Use local fallback:

```bash
find . -type f | head -100
rg "<keyword>"
```

### No `gh`

Continue normally and report:

`PR automation unavailable; use manual branch or PR workflow later.`

## Output Contract

`.pulse/tooling-status.json` should record, at minimum:

```json
{
  "timestamp": "<ISO-8601>",
  "project_root": "<absolute path>",
  "requested_mode": "<mode>",
  "recommended_mode": "<mode>",
  "status": "pass|degraded|fail",
  "tools": {
    "<tool-name>": {
      "status": "ready|unavailable|unknown",
      "check": "<command or health check used>"
    }
  },
  "blockers": [],
  "degradations": [],
  "next_skill": "pulse:using-pulse"
}
```

## Notes

- Prefer `unknown` only when a real health check does not exist.
- Never silently upgrade `unknown` to `ready`.
- A degraded run is valid only when the degraded mode is explicitly surfaced.
