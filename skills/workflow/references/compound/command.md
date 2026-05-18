# `pulse:workflow compound`

If onboarding/readiness is missing, stale, or blocked (check `.pulse/runtime/tooling-status.json`), stop and invoke `pulse:workflow onboard` before continuing.

Compounding turns completed Pulse work into reusable memory for future planning and execution.

Compounding is the canonical post-cycle, machine-readable learning pass for completed Pulse work. Use `pulse:dev-note` or `pulse:dev-note-distil` for in-flight capture; use `compound` after outcomes are known. Do not turn `compound` into a generic runtime-consolidation route.

## When to Use

- after `pulse:workflow review` completes and the outcome is known
- after execution, `pulse:architecture-rescue`, or `pulse:systematic-debug-fix` work reveals non-obvious reusable lessons
- after abandoned or constrained work that still produced durable learning

Skip only when no durable or reusable learning emerged.

## Runtime Contract

All operational rules live in `runtime-appendix.md`. Treat that file as canonical for:

- context sources across `works/`, `.pulse/runtime/*`, `.pulse/workgraph/*`, and verification artifacts
- 3-stream analysis (`pattern`, `decision`, `failure`)
- synthesis quality bar (`applicable-when` must stay specific)
- propagation taxonomy and routing destinations
- promotion rules for `.pulse/memory/critical-patterns.md`
- durable memory capture behavior in `.pulse/memory/*`
- state updates and handoff outputs

## Minimum Flow

1. Gather context from work artifacts, verification evidence, runtime state, and workgraph state.
2. Run three analysis streams and collect outputs.
3. Synthesize one learnings file at `.pulse/memory/learnings/YYYYMMDD-<slug>.md`.
4. Classify each learning by propagation type and route to destination.
5. Promote only truly global-critical learnings.
6. Update `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`.

## References

- `runtime-appendix.md` — canonical runtime contract
- `learnings-template.md` — learnings file structure
- `analysis-prompts.md` — prompts for pattern/decision/failure analysis
- `corrections-and-ratchets.md` — correction and ratchet file structures
