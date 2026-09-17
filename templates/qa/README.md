# QA templates (plan 0022 §8.6)

`ui.mjs` and `api.mjs` are the `qa-ui`/`qa-api` lane scripts `pulse init
--with-qa-templates` copies into `scripts/qa/` of the target repo. Node >=
20, no bundled dependency: prints
a clear error if it is missing, rather than shipping its own copy. Node
resolves that import walking UP from `scripts/qa/`, so playwright belongs
in the repo's ROOT `node_modules` — installing it inside a sub-package
(`web/`) will not be found (ST-1 dogfood, F21).

## `docs/operations/run.md`'s `pulse-run` blocks

Both scripts start/stop the app themselves, reading how from two fenced
code blocks in the target repo's `docs/operations/run.md`, each with the
info string `pulse-run` and one YAML object with these keys (`await_exit`
optional, default `true` — see below):

```pulse-run
id: api
start: ["docker", "compose", "up", "-d", "api"]
ready_url: "http://127.0.0.1:8000/health"
stop: ["docker", "compose", "stop", "api"]
log: ".pulse/runtime/logs/api.log"
await_exit: true
```

```pulse-run
id: ui
start: ["pnpm", "dev"]
ready_url: "http://127.0.0.1:3000"
stop: ["pkill", "-f", "next dev"]
log: ".pulse/runtime/logs/ui.log"
await_exit: false
```

`id` picks which block belongs to which script (`ui.mjs` reads `id: ui`,
`api.mjs` reads `id: api`). `start`/`stop` are argv arrays (JSON-array
syntax, which is also valid YAML flow-sequence syntax — the scripts parse
this restricted subset with a few lines of hand-rolled parsing, not a real
YAML library, to stay dependency-free); `ready_url` is polled with a plain
`fetch` until it returns HTTP 200; `log` is a path (relative to the repo
root) the running app writes to, tailed into each case's evidence. A block
may also carry an optional `migrate` argv array (see below); every other
key is ignored, so repo-specific notes can live in the block's neighbours,
not inside it.

`start` is spawned detached into its own process group with stdout/stderr
redirected to `log`. With the default `await_exit: true`, the script then
waits (bounded, 120 s) for `start` to exit before polling `ready_url`:
`docker compose up -d --build` recreates the container even on a cached
build, and the old instance — which answers `ready_url` immediately — goes
down for several seconds while the new one comes up. Polling earlier is how
a lane ends up grading a stale or dying instance (dogfood ST-1, F2). After
`start` exits, `ready_url` must answer 200 three times in a row, so a
container blipping during startup does not count as ready. A `start` that
never exits (the `pnpm dev` example above included) must set
`await_exit: false` — the wait is capped and cannot tell a dev server from
a pending compose run, so that mode keeps the old poll-immediately behavior
and cannot fully protect against the stale-instance race; prefer a `start`
command that exits. `stop` is expected to actually terminate the app; if it
exits non-zero the script falls back to sending `SIGTERM` to the process
group `start` was spawned into, best-effort. `commands_run[]` records this
truthfully: the detached `start` with `exit: null` and `detached: true`
(it has no exit code during the run), then `stop` with its real exit code.

The optional `migrate: [argv]` key fills the gap between those two (dogfood
ST-2, F26/F29): it runs after `start` has settled (after the `await_exit`
wait) and before `ready_url` is polled — the slot where
`["docker", "compose", "run", "--rm", "api", "alembic", "upgrade", "head"]`
belongs when nothing else migrates the database and a fresh db volume would
otherwise stay on its old schema forever. Absent or empty means the app
migrates itself. The migrate runs in the foreground, bounded at 120 s; a
non-zero exit or timeout never reaches the QA cases — the report is
`inconclusive` with one finding naming the failure, the command's tail is
kept in `<evidence_dir>/logs/migrate.txt`, and `commands_run[]` records it
between `start` and `stop` with its real exit code.

A missing `docs/operations/run.md`, a missing block for the script's `id`,
or a block missing any of the four keys, is not a crash: the script writes
`<evidence_dir>/qa-{ui,api}.json` with `verdict: "inconclusive"` and one
finding naming exactly what is missing, then still prints
`{"status":"done"}` — a QA lane's job is to report, not to decide the run
was a Pulse bug.

## `qa_cases[]` step conventions

- Story-scope inputs (`pulse run qa-<x> <story-id>`) carry the Story's
  whole `qa_cases[]`; each script runs only the cases whose `surface`
  matches its own (`api.mjs` runs `surface: "api"`, `ui.mjs` runs
  `surface: "ui"`; a case without `surface` is assumed to belong to the
  script's surface) — the other surface's steps are not `METHOD /path`
  lines (api) or URLs (ui) and would only crash the script.
- `ui.mjs`: `steps[0]` is the URL to navigate to; every other step is free
  text recorded verbatim into the case's `observation` — the script never
  interprets it as an instruction.
- `api.mjs`: every step is one `METHOD /path [json-body]` line, sent in
  order against the app started from the `id: api` block.

## `check` and never self-grading `pass`

A `qa_cases[]` entry may carry a `check: {argv: [...], assert: [{"exit_code": N}]}`
(plan §4.5). When present, the script runs `argv` and compares its exit
code against `assert[].exit_code`: `pass` on a match, `fail` otherwise. When absent, the case is always `inconclusive` with its artifacts attached —
neither script ever marks a case `pass` on its own judgment of a screenshot,
console log or HTTP response; that call is left to whoever reads the
evidence next (a review lane, or a human). When a `check` fails, the
check's own last stdout/stderr line is appended to the case's `observation`
(`|| check: ...`) so a failed check is diagnosable from the receipt alone.

## Output

Both scripts write `<evidence_dir>/qa-{ui,api}.json` matching the lane §8.4
shape (`verdict`, `cases[]`, `findings[]`, `commands_run[]`,
`environment.commit` = the repo's current HEAD) before printing
`{"status":"done"}` as their last stdout line. The app they started is
always stopped before the process exits, success or failure.
