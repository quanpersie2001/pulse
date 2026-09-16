# Run

Lanes `qa-ui`/`qa-api` read the two `pulse-run` blocks below to start and
stop this repo's own app; edit `start`/`ready_url`/`stop`/`log` for how
this repo actually runs. Block format: `scripts/qa/README.md` (copied in
by `pulse init --with-qa-templates`).

```pulse-run
id: api
start: ["docker", "compose", "up", "-d", "api"]
ready_url: "http://127.0.0.1:8000/health"
stop: ["docker", "compose", "stop", "api"]
log: ".pulse/runtime/logs/api.log"
```

```pulse-run
id: ui
start: ["docker", "compose", "up", "-d", "ui"]
ready_url: "http://127.0.0.1:3000"
stop: ["docker", "compose", "stop", "ui"]
log: ".pulse/runtime/logs/ui.log"
```
