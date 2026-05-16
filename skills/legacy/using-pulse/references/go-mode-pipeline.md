# Go Mode Pipeline — Step-by-Step Reference

> Load this when running Pulse end-to-end. Contains the detailed sequence, gate conditions, fallback paths, and config options.
>
> Source patterns: CE `/lfg` + `/slfg`, GSD phase loop with plan-checker and verifier, Pulse architecture v2.

## Overview

Go mode is the full Pulse pipeline from raw feature request to merged, compounded learnings. It always starts with `/pulse onboard` and always has exactly 4 human gates. The pipeline now separates a whole-feature work shape from the current work slice so each gate protects the next irreversible commitment.

```text
User: "/go <feature>"
       |
       v
[BOOTSTRAP] /pulse onboard -> check runtime state, read critical-patterns.md
       |
       v
[STEP 1] exploring
       | Output: history/<feature>/CONTEXT.md
       |
       v
[GATE 1] <- HARD STOP: "Approve CONTEXT.md? (approve only by default)"
       |
       v
[STEP 2] planning (mode gate + shape)
       | Output: discovery.md, approach.md, and one approved shape artifact:
       |         work-shape.md | phase-plan.md | epic-map.md
       |
       v
[GATE 2] <- HARD STOP: "Approve shape artifact? (approve only by default)"
       |
       v
[STEP 3] current-work preparation
       | Output depends on shape:
       |   - work-shape: current work is defined directly in work-shape.md
       |   - epic-map: epic-map.md + current-story-pack.md
       |   - phase-plan: phase-plan.md + phase-<n>-contract.md + phase-<n>-story-map.md
       | Plus: current-work beads only
       |
       v
[STEP 4] current-work validation
       | Reality gate + feasibility matrix/spikes + schema gate + structural checker + bead polish
       |
       v
[GATE 3] <- HARD STOP: "Current work verified. Approve execution? (approve only by default)"
       |
       v
[STEP 5] swarming -> executing (xN workers)
       | Current work slice only
       |
       |-- if later work slices remain -> return to STEP 3 for the next work slice
       |
       v
[STEP 6] reviewing (after final work slice only)
       | 4 specialist review agents + final synthesizer -> P1/P2/P3 findings
       | Artifact verification + human UAT
       |
       v
[GATE 4] <- HARD STOP: "Approve merge? (approve only by default)"
       |
       v
[STEP 7] compounding
       | Capture learnings -> .pulse/memory/learnings/
       |
       v
DONE
```

## Runtime Branch

Read `.pulse/runtime/tooling-status.json` after `/pulse onboard`: 

- `recommended_mode=swarm` -> use `pulse:swarming`, then worker `pulse:executing`
- `recommended_mode=single-worker` -> skip `pulse:swarming`, invoke `pulse:executing` directly
- `recommended_mode=planning-only` -> stop before execution
- `recommended_mode=blocked` -> stop entirely

---

## Step 0: Onboard + Bootstrap

Run `/pulse onboard`.

Outputs:

- `.pulse/runtime/tooling-status.json`
- `.pulse/runtime/STATE.md`
- optional resume notice via `.pulse/runtime/handoffs/manifest.json`

Do not enter the rest of Go mode until onboard reports `PASS` or `DEGRADED` readiness.

After onboard passes, bootstrap the pipeline:

1. Confirm runtime bootstrap artifacts under `.pulse/runtime/` and read `critical-patterns.md`.
2. Determine feature slug from the user's description (lowercase-hyphenated, e.g. `agent-email-inbox`).
3. If `.pulse/runtime/checkpoints/<feature>/...` already exists, use it only as an advisory resume aid; do not let it override active handoffs or state mirrors.
4. Create `history/<feature>/` if it does not exist.
5. Write `.pulse/runtime/STATE.md`:
   ```text
   focus: <feature>
   phase: go-mode/exploring
   pipeline: go
   last_updated: <timestamp>
   ```

---

## Step 1: Exploring

**Invoke:** Load `pulse:exploring` skill.

**Input:** User's feature description.

**The pulse:exploring skill will:**

- Classify domain (SEE / CALL / RUN / READ / ORGANIZE)
- Identify gray areas via Socratic Q&A
- Lock decisions with stable IDs (D1, D2, ...)
- Write `history/<feature>/CONTEXT.md`
- Self-review `CONTEXT.md`

**Update STATE.md:** `phase: go-mode/gate-1`

---

## Gate 1: Approve CONTEXT.md

```text
HARD-GATE: Do not proceed until user explicitly approves.

Present:
  "Exploration complete for [feature].
   [N] decisions locked in history/<feature>/CONTEXT.md.
   [M] open questions noted.

   Key decisions:
   - D1: [summary]
   - D2: [summary]
   ... (max 5, then 'see CONTEXT.md for full list')"
```

If the active harness provides `AskUserQuestion`, `AskMeTool`, or another structured question tool, use it with focused options:
- `Approve only`
- `Approve and continue now`
- `Revise decisions`
- `Show CONTEXT.md`

Only fall back to plain text when no structured question tool exists:
- `Approve decisions? (approve only / approve and continue now / revise / show full CONTEXT.md)`

If the user selects `Revise decisions`, or gives equivalent explicit revision feedback, loop back to exploring.

If the user selects `Show CONTEXT.md`, show the artifact and remain at this gate.

If the user selects `Approve only`, or gives equivalent explicit approval without asking to continue immediately:
- update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- record `gate: GATE 1`, `gate_status: approved`, `next_skill_recommended: pulse:planning`, and `next_action: manual_invoke`
- stop there; do not auto-load planning

If the user selects `Approve and continue now`, or gives equivalent explicit approval to stay in the current context/model:
- update the same runtime fields, but set `next_action: continue_now`
- proceed to Step 2 in the same session

---

## Step 2: Planning (Mode Gate + Shape)

**Invoke:** Load `pulse:planning` skill.

**Input:** `history/<feature>/CONTEXT.md`, `.pulse/memory/critical-patterns.md`.

**This planning pass will:**

- retrieve learnings
- run discovery
- synthesize an approach
- select mode and shape strategy
- write one shape artifact:
  - `history/<feature>/work-shape.md` (direct/spike/small)
  - `history/<feature>/phase-plan.md` (milestone/phase-shaped)
  - `history/<feature>/epic-map.md` (capability/risk-shaped)
- set shape approval to pending and stop at Gate 2

**Important:** this step does **not** create beads yet.

**Update STATE.md:** `phase: go-mode/gate-2`

---

## Gate 2: Approve selected shape artifact

```text
HARD-GATE: Do not proceed until user explicitly approves.

Present:
  "Planning complete for [feature].
   Proposed mode + shape: [work-shape | phase-plan | epic-map]
   Current-work strategy:
   - work-shape: execute selected slice from shape
   - epic-map: execute selected `current-story-pack.md`
   - phase-plan: execute selected phase contract/story map

   Shape artifact: history/<feature>/<selected-shape-artifact>.md"
```

If the active harness provides `AskUserQuestion`, `AskMeTool`, or another structured question tool, use it with focused options:
- `Approve only`
- `Approve and continue now`
- `Revise phase plan`
- `Show selected shape artifact`

Only fall back to plain text when no structured question tool exists:
- `Approve this shape artifact? (approve only / approve and continue now / revise / show full artifact)`

If the user selects `Revise shape artifact`, or gives equivalent explicit revision feedback, return to the planning pass that owns the selected shape artifact.

If the user selects `Show selected shape artifact`, show the artifact and remain at this gate.

If the user selects `Approve only`, or gives equivalent explicit approval without asking to continue immediately:
- update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- record `gate: GATE 2`, `gate_status: approved`, `next_skill_recommended: pulse:planning`, and `next_action: manual_invoke`
- stop there; do not auto-enter current-work preparation

If the user selects `Approve and continue now`, or gives equivalent explicit approval to stay in the current context/model:
- update the same runtime fields, but set `next_action: continue_now`
- proceed to Step 3 in the same session

---

## Step 3: Current-Work Preparation

**Invoke:** Load `pulse:planning` again in current-work preparation mode.

**Input:** approved shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`), `approach.md`, `CONTEXT.md`.

**Current-work prep will:**

- select one executable current work slice from approved shape
- produce shape-appropriate current-work artifacts:
  - work-shape: current work is embedded in shape artifact
  - epic-map: write `history/<feature>/current-story-pack.md`
  - phase-plan: write `history/<feature>/phase-<n>-contract.md` and `history/<feature>/phase-<n>-story-map.md`
- create beads only for current work when the mode/path requires planning-side bead materialization (for example, direct_task or already-proven small_change)
- otherwise hand off artifact-only current work to validating first; if feasibility becomes READY/READY WITH CONSTRAINTS and beads are still absent, validating routes back once for current-work bead creation and then resumes validation

**Rules:**

- default to first unprepared executable slice in the approved shape
- never create future-slice beads early
- every bead must include current-work + story context

Important planning contract:

- every HIGH-risk item must define a concrete `spike_question`
- planners do not create spike beads
- planners embed relevant learnings into bead descriptions

**Update STATE.md:** `phase: go-mode/validating`

---

## Step 4: Current-Work Validation

**Invoke:** Load `pulse:validating` skill.

**Input:** approved shape artifact, shape-specific current-work artifacts, `approach.md`, `CONTEXT.md`, and optionally current-work beads if they were already created in planning.

**The pulse:validating skill will:**

- orient on mode/shape/current work and verify shape approval
- run reality-fit and feasibility checks (including spikes where needed)
- if feasibility is READY/READY WITH CONSTRAINTS and current-work beads are absent, route back once to planning for current-work bead creation, then resume validation
- run bead schema gate before structural checker when beads are present
- run structural checker loop (<=3 iterations)
- polish bead graph and run current-work readiness review

If a spike returns `NO`, stop and go back to planning.

If execution or debugging reveals that the issue is no longer a local bug but an architectural mismatch, pause the pipeline and send the work back to `pulse:planning` or `pulse:validating` before continuing.

**Update STATE.md:** `phase: go-mode/gate-3`

---

## Gate 3: Approve Feasibility-Validated Current Work

```text
HARD-GATE: This is the most critical gate. Do not proceed until user explicitly approves.

Present:
  "Validation complete for [feature], current work slice [name/id].
   [N] beads ready for execution.
   Feasibility status: [ready / blocked]
   Risk: [X] HIGH items -> spikes: [all passed / N failed]

   Any unresolved concerns: [list or 'none']

   Execution mode: [swarm / single-worker] (from .pulse/runtime/tooling-status.json)"
```

If the active harness provides `AskUserQuestion`, `AskMeTool`, or another structured question tool, use it with focused options:
- `Approve only`
- `Approve and continue now`
- `Review beads`
- `Revise plan`

Only fall back to plain text when no structured question tool exists:
- `Current work is feasibility-validated. Approve execution? (approve only / approve and continue now / review beads / revise plan)`

If the user selects `Revise plan`, or gives equivalent explicit revision feedback, return to planning or validating.
If the user selects `Review beads`, stay at this gate and inspect bead details before asking again.
If the user selects `Approve only`, or gives equivalent explicit approval without asking to continue immediately:
- update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- record `gate: GATE 3`, `gate_status: approved`, `next_skill_recommended` from `.pulse/runtime/tooling-status.json`, and `next_action: manual_invoke`
- stop there; do not auto-start swarming or execution
If the user selects `Approve and continue now`, or gives equivalent explicit approval to stay in the current context/model:
- update the same runtime fields, but set `next_action: continue_now`
- proceed to Step 5 in the same session

---

## Step 5: Swarming + Executing (Current Work Slice)

Use `pulse:swarming` if `/pulse onboard` recommends `swarm`, otherwise invoke `pulse:executing` directly.

**The pulse:swarming skill will:**

- initialize the coordination runtime
- spawn workers for the current-work bead set
- monitor current-work execution
- verify current-work beads closed

### Work-slice loop rule

After current-work execution completes:

- if approved shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`) shows later work slices still pending -> return to Step 3 for the next work slice
- if the current work slice was the final approved work slice -> proceed to Step 6
- do not use an empty epic subtree alone as proof the whole feature is complete; later work slices may not be materialized yet
- when in doubt, approved shape artifact + `.pulse/runtime/STATE.md` decide whether reviewing is allowed

**Update STATE.md:** either `phase: go-mode/planning-next-work` or `phase: go-mode/reviewing`

---

## Step 6: Reviewing

**Invoke:** Load `pulse:reviewing` skill only after the final current-work execution completes and the approved shape artifact plus `.pulse/runtime/STATE.md` agree that no later work remains.

**The pulse:reviewing skill will:**

- dispatch 4 specialist review agents first, then run the final synthesizer as agent 5
- run 3-level artifact verification
- run human UAT
- run final finishing tasks

Review remains mandatory in both normal and quick mode.

**Update STATE.md:** `phase: go-mode/gate-4`

---

## Gate 4: Approve Merge

```text
HARD-GATE: Never auto-merge.

Present:
  "Review complete for [feature].
   P1 (blocks merge): [count] - [titles if any]
   P2 (should fix):   [count]
   P3 (nice to have): [count]"
```

If the active harness provides `AskUserQuestion`, `AskMeTool`, or another structured question tool, use it with focused options.

If P1 > 0:
- `Fix findings`
- `Show review details`
- `Override merge` only with explicit user confirmation

If P1 = 0:
- `Approve only`
- `Approve and continue now`
- `Show review details`
- `Do not merge yet`

Only fall back to plain text when no structured question tool exists.

If P1 > 0 plain-text fallback:
- `P1 findings block merge. Options: fix P1s now / show P1 details / override (requires explicit user confirmation)`

If P1 = 0 plain-text fallback:
- `No blocking findings. Approve review closeout? (approve only / approve and continue now / show P2s first / not yet)`

If P1 = 0 and the user selects `Approve only`, or gives equivalent explicit approval without asking to continue immediately:
- update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md`
- record `gate: GATE 4`, `gate_status: approved`, `next_skill_recommended: pulse:compounding`, and `next_action: manual_invoke`
- stop there; do not auto-load compounding and never auto-merge

If P1 = 0 and the user selects `Approve and continue now`, or gives equivalent explicit approval to stay in the current context/model:
- update the same runtime fields, but set `next_action: continue_now`
- proceed to Step 7 in the same session

If fix beads are created, execute them and re-run reviewing before presenting Gate 4 again.

---

## Step 7: Compounding

**Invoke:** Load `pulse:compounding` skill.

**Input:** full feature history (`CONTEXT.md`, `approach.md`, approved shape artifact, review findings, execution notes).

**The pulse:compounding skill will:**

- dispatch 3 analysis subagents: patterns / decisions / failures
- write `.pulse/memory/learnings/YYYYMMDD-<feature>.md`
- promote critical items to `.pulse/memory/critical-patterns.md`
- optionally index via CASS

**Final update STATE.md:**

```text
focus: (none)
phase: idle
last_feature: <feature>
last_updated: <timestamp>
```

Output:

- durable learning entries under `.pulse/memory/learnings/`

---

## Fallback Paths

### If exploring produces a CONTEXT.md the user rejects at GATE 1

```text
-> Identify which decisions need revision
-> Load pulse:exploring skill, focus on those specific gray areas
-> Update CONTEXT.md in place
-> Re-present GATE 1
```

### If the user rejects the selected shape artifact at GATE 2

```text
-> Identify which shape boundaries, stories, or risk/capability grouping feels wrong
-> Return to the shape-selection planning pass
-> Update selected shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`)
-> Re-present GATE 2
```

### If validating fails after 3 plan-checker iterations

```text
-> Present failing dimensions to user
-> Ask: "Return to planning with these specific concerns?"
-> Load planning with the failure report as context
-> Re-run validating after current-work artifacts are re-prepared
```

### If a spike fails

```text
-> STOP: do not proceed to GATE 3
-> Present: "Spike [id] failed: [reason]. Current work slice is blocked."
-> Options: (a) Revise approach, (b) Descope the risky part, (c) Re-split work-shape boundaries
-> If revise: return to planning and then re-run validating
```

### If orchestrator context hits 65% mid-swarm

```text
-> Write handoff via .pulse/runtime/handoffs/coordinator.json and update manifest
-> Present: "Context budget reached. Current work-slice swarm paused.
            [X] beads complete, [Y] in flight.
            Resume in a new session."
-> End turn gracefully
```

### If P1 findings are present at GATE 4 and the user wants to fix

```text
-> Create fix beads via br create for each P1 finding
-> Load pulse:swarming skill (fix-bead swarm only)
-> After execution: re-run reviewing (targeted - fixes diff only)
-> Re-present GATE 4
-> Repeat until P1 = 0 or user explicitly overrides
```

---

## Pause and Resume

All pause flows use the handoff contract:

- `.pulse/runtime/handoffs/manifest.json`
- owner-scoped handoff files

Do not write or read the retired global handoff file.

When resuming:

1. run `/pulse onboard` if tooling or runtime state may have changed
2. read the manifest
3. resume the selected owner

---

## Config Options (.pulse/config.json)

Absent = enabled. Only set to disable.

```json
{
  "go_mode": {
    "skip_exploring": false,
    "skip_compounding": false,
    "auto_approve_gates": false,
    "spike_on_medium_risk": false
  },
  "validating": {
    "plan_checker_max_iterations": 3,
    "bead_polish_rounds": 3
  },
  "reviewing": {
    "parallel_agents": true,
    "serial_threshold": 6
  }
}
```

---

## Quick Mode Pipeline (Reference)

For small fixes (<=3 files, LOW risk, no gray areas):

```text
/pulse onboard
  -> verify tooling, write runtime STATE.md
  |
planning (lightweight)
  -> one-slice shape plan
  -> approval gate
  -> one current-work bead
  -> no multi-model refinement
  |
validating (lightweight)
  -> abbreviated structural verification
  -> skip spikes (LOW risk)
  -> bv check only
  |
swarming -> executing (single worker)
  |
reviewing (lightweight for small_change)
  -> still required before merge, but keep it proportionate to the work
  |
compounding (only if lesson learned)
```

---

## Design Rationale

**Why 4 gates, not 3?**

Because shape selection itself is now a first-class human decision. `CONTEXT.md` locks intent, the approved shape artifact (`work-shape.md` | `phase-plan.md` | `epic-map.md`) locks feature structure, validating locks execution-readiness for current work, and reviewing locks merge-readiness.

**Why is GATE 3 the most critical?**

Execution is the only stage that creates source-code side effects. A broken current work slice discovered post-execution costs far more to fix than one caught in feasibility validation.

**Why does planning now happen in two passes?**

Because "show me the whole shape" and "prepare one work slice for execution" are different jobs. Combining them made shape/story explanations too abstract and pushed bead creation earlier than users wanted.

**What tone should the model use at every gate?**

Concrete and scenario-first. At every gate, the model should explain:

1. what becomes true or what is wrong
2. what the system does today
3. why that matters
4. one realistic example
5. what approval or change is needed next

Gate summaries should not rely on reviewer shorthand or planner jargon alone.
