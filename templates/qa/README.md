# QA templates (plan 0022 §8.6)

`ui.mjs` and `api.mjs` are the `qa-ui`/`qa-api` lane scripts `pulse init
--with-qa-templates` copies into `scripts/qa/` of the target repo. Node >=
20, no bundled dependency: `ui.mjs` loads `playwright` with a dynamic
`import('playwright')` from the target repo's own `node_modules` and prints
a clear error if it is missing, rather than shipping its own copy.

## `docs/operations/run.md`'s `pulse-run` blocks

Both scripts start/stop the app themselves, reading how from two fenced
code blocks in the target repo's `docs/operations/run.md`, each with the
info string `pulse-run` and one YAML object with exactly these keys:

```pulse-run
id: api
start: ["docker", "compose", "up", "-d", "api"]
ready_url: "http://127.0.0.1:8000/health"
stop: ["docker", "compose", "stop", "api"]
log: ".pulse/runtime/logs/api.log"
```

```pulse-run
id: ui
start: ["pnpm", "dev"]
ready_url: "http://127.0.0.1:3000"
stop: ["pkill", "-f", "next dev"]
log: ".pulse/runtime/logs/ui.log"
```

`id` picks which block belongs to which script (`ui.mjs` reads `id: ui`,
`api.mjs` reads `id: api`). `start`/`stop` are argv arrays (JSON-array
syntax, which is also valid YAML flow-sequence syntax — the scripts parse
this restricted subset with a few lines of hand-rolled parsing, not a real
YAML library, to stay dependency-free); `ready_url` is polled with a plain
`fetch` until it returns HTTP 200; `log` is a path (relative to the repo
root) the running app writes to, tailed into each case's evidence.

`start` is spawned detached into its own process group with stdout/stderr
redirected to `log`, and the script does not wait for it to exit — many
`start` commands (the `pnpm dev` example above included) are long-running
dev servers that never exit on their own, so waiting would hang until
Pulse's lane timeout. `ready_url` returning 200 is the only readiness
signal. `stop` is expected to actually terminate the app; if it exits
non-zero the script falls back to sending `SIGTERM` to the process group
`start` was spawned into, best-effort. `commands_run[]` records this
truthfully: the detached `start` with `exit: null` and `detached: true`
(it has no exit code — it never exits during the run), then `stop` with
its real exit code.

A missing `docs/operations/run.md`, a missing block for the script's `id`,
or a block missing any of the four keys, is not a crash: the script writes
`<evidence_dir>/qa-{ui,api}.json` with `verdict: "inconclusive"` and one
finding naming exactly what is missing, then still prints
`{"status":"done"}` — a QA lane's job is to report, not to decide the run
was a Pulse bug.

## `qa_cases[]` step conventions

- `ui.mjs`: `steps[0]` is the URL to navigate to; every other step is free
  text recorded verbatim into the case's `observation` — the script never
  interprets it as an instruction.
- `api.mjs`: every step is one `METHOD /path [json-body]` line, sent in
  order against the app started from the `id: api` block.

## `check` and never self-grading `pass`

A `qa_cases[]` entry may carry a `check: {argv: [...], assert: [{"exit_code": N}]}`
(plan §4.5). When present, the script runs `argv` and compares its exit
code against `assert[].exit_code`: `pass` on a match, `fail` otherwise. When
absent, the case is always `inconclusive` with its artifacts attached —
neither script ever marks a case `pass` on its own judgment of a screenshot,
console log or HTTP response; that call is left to whoever reads the
evidence next (a review lane, or a human).

## Output

Both scripts write `<evidence_dir>/qa-{ui,api}.json` matching the lane §8.4
shape (`verdict`, `cases[]`, `findings[]`, `commands_run[]`,
`environment.commit` = the repo's current HEAD) before printing
`{"status":"done"}` as their last stdout line. The app they started is
always stopped before the process exits, success or failure.
