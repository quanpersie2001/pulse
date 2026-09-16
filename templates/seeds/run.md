# Run

Lanes `qa-ui`/`qa-api` read the two `pulse-run` blocks below to start and
stop this repo's own app; edit `start`/`ready_url`/`stop`/`log` for how
this repo actually runs. Keep `--build` in `start` so a lane always serves
the code the worker just changed, and keep `await_exit: true` so the lane
waits for that build/recreate instead of grading the previous instance
(dogfood ST-1, F2). Block format: `scripts/qa/README.md` (copied in by
`pulse init --with-qa-templates`).

```pulse-run
id: api
start: ["docker", "compose", "up", "-d", "--build", "api"]
ready_url: "http://127.0.0.1:8000/health"
stop: ["docker", "compose", "stop", "api"]
log: ".pulse/runtime/logs/api.log"
await_exit: true
```

```pulse-run
id: ui
start: ["docker", "compose", "up", "-d", "--build", "ui"]
ready_url: "http://127.0.0.1:3000"
stop: ["docker", "compose", "stop", "ui"]
log: ".pulse/runtime/logs/ui.log"
await_exit: true
```
