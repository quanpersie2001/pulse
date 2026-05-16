# `pulse:workflow brainstorm` Visual Support Guidance

Use this when a design decision would be clearer if the user can compare options visually.

## When to use visuals

Prefer visual support when the decision is about:

- layout direction
- information hierarchy
- page or screen flow
- wireframe comparison
- diagramming relationships or sequence
- side-by-side interface alternatives

Stay in text when the decision is about:

- goals
- scope boundaries
- constraints
- priorities
- non-visual trade-offs
- success criteria

## Preferred interaction pattern

1. Offer visual support in its own message.
2. Prefer structured question previews when the comparison is small.
3. Escalate to the local visual runtime only when previews are not enough.
4. Keep options focused and mutually exclusive when possible.
5. Return to text as soon as the visual ambiguity is resolved.

## Advanced local visual runtime

Use the local runtime only when browser-rendered options will materially reduce ambiguity.

### Start the runtime

```bash
scripts/start-visual-server.sh --project-dir /path/to/repo
```

Expected startup output includes:

- `url`
- `screen_dir`
- `state_dir`

### Runtime workflow

1. If a `url` is returned, tell the user the visual runtime is active.
2. Share the exact URL.
3. Ask the user to open it in a browser, compare options, and return to the terminal.
4. Write one HTML screen per option set into `screen_dir`.
5. Read `state_dir/events` on the next turn to capture their choice.
6. If the runtime is unavailable, fall back to previews or text.
7. Stop the runtime when done:

```bash
scripts/stop-visual-server.sh <session_dir>
```

## Fallback rule

If Node is unavailable, startup fails, or the environment makes the local URL unusable, briefly explain that the browser runtime could not be used and continue without blocking the brainstorming session.

## Preview design rules

- Show 2-4 options max.
- Keep previews intentionally low-fidelity unless polish itself is the decision.
- Make the difference between options obvious.
- Do not mix visual differences with conceptual differences in one comparison unless the user explicitly wants that.
