---
name: reviewing
description: Use after execution completes when the user needs final quality verification and release-readiness review before closeout.
metadata:
  version: '1.3'
  ecosystem: pulse
  upstream: swarming
  downstream: compounding
  dependencies:
    - id: beads-cli
      kind: command
      command: br
      missing_effect: unavailable
      reason: Reviewing creates review beads and closes the epic through br.
    - id: beads-viewer
      kind: command
      command: bv
      missing_effect: unavailable
      reason: Reviewing verifies the live bead graph before epic closeout.
---

# Reviewing

If preflight readiness is missing, stale, or blocked (check `.pulse/tooling-status.json`), stop and invoke `pulse:using-pulse` before continuing.

`br` is required for reviewing. If `br` is unavailable, Gate 4 cannot run; stop immediately and route to preflight/tooling remediation via `pulse:using-pulse`.

Reviewing is the last automated quality gate before shipping. It verifies behavior, safety, and completeness for Pulse ecosystem closeout rather than trusting bead closure alone.

## When to Invoke

- After `pulse:swarming` reports final current work is complete and the approved shape artifacts plus `.pulse/STATE.md` agree no later work remains (`work-shape.md`, `phase-plan.md`, or `epic-map.md` + `current-story-pack.md` when epic-map is active)
- During Pulse ecosystem closeout immediately before Gate 4 decisioning
- Optional flags: `--serial`, `--skip-uat` (only when human UAT is intentionally omitted and non-interactive evidence is used)

## Required Inputs

Read before starting:

- `.pulse/project-docs.json` when present, plus minimal relevant listed docs
- `history/<feature>/CONTEXT.md`
- `history/<feature>/approach.md`
- `history/<feature>/lifecycle-summary.md` when present
- `.pulse/STATE.md`
- reviewing-owned changeset evidence for the active Pulse closeout path

If `.pulse/project-docs.json` is absent, detect and read the smallest relevant project docs (README, architecture, ADR, domain docs).

## Runtime Contract

All execution-time rules live in `references/runtime-appendix.md`. Treat that file as canonical for:

- 4+1 review orchestration
- severity mapping and review bead creation rules
- Gate 4 hard-block behavior for P1
- artifact verification contract and severity mapping
- UAT failure routing
- finishing checklist and closeout

## Minimum Flow

1. Run specialist review (4+1 model).
2. Run artifact verification for all promised deliverables.
3. Turn every accepted finding into a review bead by severity and deduplicate overlaps before presenting results.
4. Enforce hard gate: if any P1 review bead exists, stop and present the blocking bead IDs; do not continue closeout.
5. Run human UAT unless explicitly skipped under allowed mode.
6. If UAT or finishing checks reveal new accepted findings, create or update the corresponding review/fix beads before proceeding.
7. Execute finishing and update Pulse state artifacts.
8. Recommend `pulse:compounding` as the next skill and default to `next_action: manual_invoke`.

## Handoff

If paused, write a reviewing-owned handoff using `../using-pulse/references/handoff-contract.md` and register it in `.pulse/handoffs/manifest.json`.

After finishing completes, report feature completion, update runtime state for Gate 4 approval, and recommend compounding without auto-triggering it unless the user explicitly asks to continue now.

## References

- `references/runtime-appendix.md` — canonical runtime contract
- `references/review-agent-prompts.md` — exact prompts for all 5 agents
- `references/review-bead-template.md` — review bead format and creation contract
