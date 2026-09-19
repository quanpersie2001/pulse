# Run

Lanes `qa-ui`/`qa-api` read the two `pulse-run` blocks below to start and
stop this repo's own app; edit `start`/`ready_url`/`stop`/`log` for how
this repo actually runs. Keep `--build` in `start` so a lane always serves
the code the worker just changed, and keep `await_exit: true` so the lane
waits for that build/recreate instead of grading the previous instance
(dogfood ST-1, F2). The optional `migrate` key runs a migration command
after `start` has settled and before `ready_url` is polled — without it a
fresh database volume stays on its old schema forever (dogfood ST-2,
F26/F29); omit it when the app migrates itself. Block format:
`scripts/qa/README.md` (copied in by `pulse init --with-qa-templates`).

```pulse-run
id: api
start: ["docker", "compose", "up", "-d", "--build", "api"]
migrate: ["docker", "compose", "run", "--rm", "api", "alembic", "upgrade", "head"]
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

Ordering: story-scope qa lanes run against the committed tree — run and
seal them **after** the story's final commit, immediately before
`pulse close-story`; a commit landing after the qa seal stales the
receipts (`close_story_qa_not_satisfied`, dogfood 0025 F14).
