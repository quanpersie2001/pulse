---
name: validating
description: >-
  Use after planning and before execution to verify approved current work is
  reality-fit, feasibility-ready, and explicitly approved at Gate 3.
metadata:
  version: '1.5'
  position: 3
  chain: exploring → planning → validating → (swarming | executing)
  dependencies:
    - id: beads-cli
      kind: command
      command: br
      missing_effect: unavailable
      reason: Validating verifies and manages beads through br.
    - id: beads-viewer
      kind: command
      command: bv
      missing_effect: unavailable
      reason: Validating inspects the bead graph with bv before approving execution.
---

# Validating

If preflight readiness is missing, stale, or blocked (check `.pulse/tooling-status.json`), stop and invoke `pulse:using-pulse`.

## Purpose

Do not start execution unless current work is feasibility-ready in real repo conditions. This skill verifies that:
- mode and shape still fit current reality
- assumptions are proven with concrete evidence
- current-work artifacts are executable
- beads are schema-valid and implementation-ready when needed
- execution stays blocked until explicit user approval

## Non-Negotiable Gates

- **Gate A — Approved shape is required** in `work-shape.md`, `phase-plan.md`, or `epic-map.md`.
- **Gate B — Reality + feasibility must pass** before structural polish.
- **Gate C — If beads exist, bead schema must pass** before structural checker.
- **Gate D (Gate 3) — Explicit user approval is mandatory before execution.**

If any gate fails, stop and route back; never “proceed anyway.”

## Required Inputs

- `history/<feature>/CONTEXT.md`
- `history/<feature>/discovery.md`
- `history/<feature>/approach.md`
- approved shape artifact: `work-shape.md`, `phase-plan.md`, or `epic-map.md`
- mode-required current-work artifacts:
  - direct/spike/small: current work in `work-shape.md`
  - epic-map: `current-story-pack.md`
  - phase-plan: `phase-<n>-contract.md` and `phase-<n>-story-map.md`
- `.beads/*.md` for current work only when beads already exist

If any required non-bead artifact is missing, return to `pulse:planning`.

## Runtime Flow (Hot Path)

### 0) Orient to mode, shape, and current work (mandatory every run)

Read both:
- approved shape artifact (source of truth)
- `.pulse/STATE.md` (mirror)

Confirm and present:
- mode
- shape artifact and approval status
- current work and practical goal
- mirror status (in sync / out of sync / missing)

If shape approval is not `APPROVED`, stop.
If shape artifact and mirror disagree, stop and return to planning for sync.

Use orientation template from `references/runtime-appendix.md`.

### 1) Reality gate (fail fast)

Run mode-fit and repo-fit checks before structural validation:
- mode still matches task reality
- approved shape matches real constraints
- assumptions are explicit and testable
- no smaller safe path is being overruled without reason
- proof surface exists (files/APIs/commands/runtime checks)

If reality gate fails, route back to planning.

### 2) Feasibility matrix + spikes/probes

Build feasibility matrix for blocking assumptions:
- always for `high_risk_feature`
- for `standard_feature` whenever meaningful assumptions remain

Require spikes for unproven assumptions that can invalidate current work:
- one spike bead = one yes/no `spike_question`
- isolated execution with 30-minute timebox
- findings at `.spikes/<feature>/<spike-id>/FINDINGS.md`
- definitive `YES` or `NO`

If timebox expires without decision:
- present findings
- offer +15m extension only with explicit approval, replan, or mitigation
- never continue silently

Decision handling:
- **YES**: require planning-owned artifacts/beads to reflect constraints before execution approval (route to planning when those artifacts must change)
- **NO**: stop, update planning artifacts, return to planning, then re-run validating

Bead-absence branch (feasibility-first path):
- If feasibility is `READY` or `READY WITH CONSTRAINTS` and beads are required for current work but do not yet exist, stop and route to `pulse:planning` to create only validated current-work beads.
- After bead creation, resume `pulse:validating` at Schema Gate, then continue structural checks and bead review before Gate 3 approval.

### 3) Schema gate (before structural checker)

Run schema gate only when current-work beads exist.

Before any structural checker:
- verify every bead has canonical required fields
- verify `testing_mode: tdd-required` beads include red/green `tdd_steps`
- verify `verify` and `verification_evidence` are concrete
- verify file scope is not obviously overbroad
- verify HIGH-risk beads include meaningful `learning_refs` when prior recall clearly applies

If schema fails, repair bead schema defects when local/clear; route to planning when the fix requires reshaping planning-owned artifacts.

Use schema checklist in `references/runtime-appendix.md`.

### 4) Structural verification (max 3 iterations)

Run plan-checker only after feasibility + schema pass. PASS only if all dimensions pass:
1. mode/shape coherence
2. current-work coverage and ordering
3. decision coverage
4. dependency correctness
5. file scope isolation
6. context budget
7. verification completeness
8. integration/exit-state/risk coherence

Rules:
- PASS only if all dimensions pass.
- On fail, fix exact artifacts and re-run.
- Maximum 3 iterations. If still failing after 3, escalate and stop.

### 5) Bead polishing and readiness

When beads are required, polish bead graph:
- `bv --robot-suggest` for dependency completeness
- `bv --robot-insights` for cycles/bottlenecks/orphans
- `bv --robot-priority` for foundational ordering
- deduplicate overlapping same-goal same-scope beads
- run fresh-eyes bead review and resolve all CRITICAL flags
- confirm current-work ↔ bead mapping coherence both directions

Then run readiness check:
- entry state is concrete
- exit state is observably provable
- integration path is credible
- verification is concrete
- architecture boundaries are preserved
- scopes remain surgical

Any “no/not sure” routes back to planning artifacts or bead set; do not approve.

### 6) Final approval gate (Gate 3 hard stop)

Present validation summary and request explicit approval.
Use structured question tools when available; otherwise plain text.

Execution is forbidden until approval is explicit.

On approval:
- update `.pulse/STATE.md` and `.pulse/state.json` to validated status
- default approved path: record `gate: GATE 3`, `gate_status: approved`, `next_action: manual_invoke`, and set `next_skill_recommended` from `recommended_mode`, then stop
- optional fast path: only continue in the same session when the user explicitly chooses an equivalent of `Approve and continue now`; in that case set `next_action: continue_now` before continuing
- mode mapping for `next_skill_recommended`:
  - `recommended_mode=swarm` → `pulse:swarming`
  - `recommended_mode=single-worker` → `pulse:executing`

On rejection:
- capture concern category and route back precisely (shape/current-work/approach/beads)

Use final approval template/options in `references/runtime-appendix.md`.

## Execution Mode Compatibility

Validating must support both downstream modes:
- **Swarm mode**: all validations complete, recommend `pulse:swarming` as next skill and default to `next_action: manual_invoke`
- **Single-worker mode**: all validations complete, recommend `pulse:executing` as next skill and default to `next_action: manual_invoke`

Only switch to `next_action: continue_now` when the user explicitly asks to keep going in the same context. The validation standard is identical in both modes.

## Lightweight Mode

Allowed only for confirmed LOW-risk direct/small single-slice work:
- abbreviated reality + feasibility check
- skip spikes
- run `bv --robot-suggest` when beads exist
- still require Gate 3 explicit approval

If uncertain, run full mode.

## Context Budget

If context exceeds 65%, write a validating-owned handoff using the shared envelope and register it in `.pulse/handoffs/manifest.json`.

## Red Flags

- any execution before Gate 3 approval
- skipping orientation on resume
- validating while shape approval is not `APPROVED`
- validating when shape artifact and `.pulse/STATE.md` disagree
- running structural checker before reality/feasibility/schema pass
- attempting a 4th structural iteration
- missing or non-decisive HIGH-risk spike outcomes
- continuing after a `NO` spike
- unresolved CRITICAL bead-review flags at approval time
