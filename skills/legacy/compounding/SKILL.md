---
name: compounding
description: Use after completed Pulse work when the user asks to capture post-cycle machine-readable learnings for future planning and execution.
metadata:
  version: '1.4'
  ecosystem: pulse
  position: '8 of 9 — runs after reviewing, before next feature'
  dependencies:
    - id: beads-cli
      kind: command
      command: br
      missing_effect: unavailable
      reason: Compounding reads bead history to reconstruct executed work.
---

# Compounding

If preflight readiness is missing, stale, or blocked (check `.pulse/tooling-status.json`), stop and invoke `pulse:using-pulse` before continuing.

Compounding turns completed feature work into reusable memory for future planning and execution.

Compounding is the canonical post-cycle, machine-readable learning pass for completed Pulse work. Use `pulse:dream` for runtime-artifact consolidation outside that post-cycle moment.

## When to Use

- after `pulse:reviewing` completes and feature is merged
- after systematic bug-fix work reveals non-obvious root cause
- after abandoned work that produced reusable lessons

Skip only when no durable/reusable learning emerged.

## Runtime Contract

All operational rules live in `references/runtime-appendix.md`. Treat that file as canonical for:

- 3-stream analysis (pattern/decision/failure)
- synthesis quality bar (`applicable-when` must be specific)
- propagation taxonomy and routing destinations
- promotion rules for `critical-patterns.md`
- durable memory capture behavior in `.pulse/memory/*`
- state updates and handoff outputs

## Minimum Flow

1. Gather context from history artifacts, verification evidence, and bead graph.
2. Run three analysis streams and collect outputs.
3. Synthesize one learnings file at `.pulse/memory/learnings/YYYYMMDD-<slug>.md`.
4. Classify each learning by propagation type and route to destination.
5. Promote only truly global-critical learnings.
6. Update `.pulse/STATE.md` and `.pulse/state.json`.

## References

- `references/runtime-appendix.md` — canonical runtime contract
- `references/learnings-template.md` — learnings file structure
- `references/analysis-prompts.md` — prompts for pattern/decision/failure analysis
- `references/corrections-and-ratchets.md` — correction and ratchet file structures
