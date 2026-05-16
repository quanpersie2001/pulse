---
name: planning
description: >-
  Use after pulse:exploring when locked decisions are ready and the user needs a
  mode-gated plan with an approved work shape and current-work prep for
  validation.
metadata:
  version: '2.4'
  ecosystem: pulse
  dependencies:
    - id: beads-cli
      kind: command
      command: br
      missing_effect: unavailable
      reason: Planning creates beads for validated current work through br.
    - id: beads-viewer
      kind: command
      command: bv
      missing_effect: unavailable
      reason: Planning uses bv to verify bead graph structure.
    - id: gitnexus
      kind: mcp_server
      server_names: [gitnexus]
      config_sources: [repo_codex_config, global_codex_config, plugin_mcp_manifest]
      missing_effect: degraded
      reason: Planning uses GitNexus MCP tools for codebase discovery when available.
---

# Planning

If preflight readiness is missing, stale, or blocked (check `.pulse/tooling-status.json`), stop and invoke `pulse:using-pulse` before continuing.

Planning has two jobs:
1. Choose the least workflow that protects the work.
2. Prepare only the approved current work for validating.

If this skill cannot explain the work with practical outcomes and realistic examples, it is not done.

## Core Planning Model

```text
Mode
  -> Shape
    -> Epic?
      -> Current Work
        -> Beads?
```

- **Mode**: `direct_task` | `spike` | `small_change` | `standard_feature` | `high_risk_feature`
- **Shape**: work-shape, phase-plan, or epic-map
- **Current Work**: direct item, spike question, current story, or current phase
- **Bead**: worker-sized executable unit for validated current work

For new feature work, define architecture/reality basis before shaping current work.

## Hot-Path Rules

- `history/<feature>/CONTEXT.md` is the source of truth; planning reads but never overrides locked decisions.
- Every `small_change` still requires an approved mini `CONTEXT.md`; do not treat low risk as permission to skip locked decisions.
- Read `.pulse/project-docs.json` first when present, then listed docs before relying only on feature history artifacts.
- Start with a mode gate and record why smaller modes are insufficient when above `small_change`.
- Gate 2 is mandatory: do not prepare current-work artifacts before explicit shape approval.
- For tough work, prefer epic maps when capability/risk areas are clearer than milestone phases.
- Create beads only for approved current work, and only after feasibility passes (except already-proven direct/small work).
- Carry relevant corrections/ratchets from learnings into bead `learning_refs`.

## Pipeline

```text
CONTEXT.md
  -> Phase 0 Learnings Retrieval
  -> Phase 1 Discovery                 (history/<feature>/discovery.md)
  -> Phase 2 Synthesis                 (history/<feature>/approach.md)
  -> Phase 3 Mode Gate + Work Shape    (work-shape.md | phase-plan.md | epic-map.md)
  -> HARD-GATE (Gate 2): human approval required
  -> approved outcome: continue inside planning to current-work prep
  -> Phase 4 Current-Work Prep         (current-story-pack.md | phase-<n>-contract.md + phase-<n>-story-map.md)
  -> Handoff: recommend pulse:validating (`next_action: manual_invoke` by default)
```

Bead timing contract:
- `direct_task` and already-proven `small_change` may create current-work beads in planning.
- `standard_feature` and `high_risk_feature` default to no-bead planning output; validating must confirm feasibility first.
- After validating reaches feasibility `READY`/`READY WITH CONSTRAINTS`, route once to planning to create only current-work beads, then resume validating for schema/structure/bead review.

Load `references/planning-reference.md` for mode quality rules and artifact templates.

## Phase Execution Contract

### Before you start

1. If `.pulse/scripts/pulse_status.mjs` exists, run `node .pulse/scripts/pulse_status.mjs --json`.
2. Read `history/<feature>/CONTEXT.md`; if missing, stop and ask the human to provide or approve a mini `CONTEXT.md` through `pulse:exploring` before planning, even for `small_change`.
3. Stop if `.pulse/tooling-status.json` reports `blocked`.
4. Read project docs first (`.pulse/project-docs.json` when present; otherwise minimal relevant docs set).

### Phase 0: Learnings Retrieval (mandatory)

1. Read `.pulse/memory/critical-patterns.md`.
2. Check staleness via `.pulse/STATE.md`; if last compounding is missing or >3 completed features old, warn and continue.
3. Use recall-pack pointers first (corrections -> ratchets -> learnings), then targeted grep only if needed.
4. Record `Institutional Learnings` at top of `history/<feature>/discovery.md`.

### Phase 1: Discovery

- Explore architecture topology, existing patterns, constraints, and external references only when novel.
- Prefer GitNexus when configured; explicitly document fallback when unavailable.
- Write `history/<feature>/discovery.md` using `references/planning-reference.md`.

### Phase 2: Synthesis

- Read `CONTEXT.md` + `discovery.md`.
- Write `history/<feature>/approach.md` using `references/planning-reference.md`.
- Include gap analysis, recommended approach, alternatives, risk map, proposed structure, learnings applied.
- For every HIGH risk define: component, reason, validating owner, YES/NO spike question, affected beads, decision gate options, and `testing_mode` expectation.

### Phase 3: Mode Gate + Work Shape (Gate 2 setup)

- Choose mode first: `direct_task | spike | small_change | standard_feature | high_risk_feature`.
- Write one approved shape artifact:
  - `work-shape.md` for direct/spike/small work
  - `phase-plan.md` for milestone-shaped work
  - `epic-map.md` for capability/risk-shaped work
- Set `Approval status: PENDING` and stop for user approval.
- If revised, set or keep `REVISE_REQUIRED`; only set `APPROVED` after explicit approval.
- Gate 2 approval authorizes planning to continue into Phase 4; it does not invoke validating.
- After approval, update runtime state with `gate: GATE 2`, `gate_status: approved`, and continue to current-work prep.
- Planning is not complete until the approved current-work artifacts are written.
- Only invoke `pulse:validating` in the same session when the user explicitly chooses an equivalent of `Approve and continue to validating now`; otherwise validating remains the next manual skill after planning completion.

Approval sync checklist before moving forward:
1. Update shape artifact approval state.
2. Sync same state into `.pulse/STATE.md` and `.pulse/state.json`.
3. Confirm both artifacts name the same approved current work.
4. Enter current-work preparation after Gate 2 approval; do not invoke validating unless the user explicitly asked to continue to validating now in the same context.

### Phase 4: Current-Work Prep

- Select current work from approved shape:
  - direct/spike/small: current work in `work-shape.md`
  - epic-map: current story in `current-story-pack.md`
  - phase-plan: current phase contract/story map
- Keep prep bounded to one executable current slice.
- Current work must lock entry/exit, scope, verification, and out-of-scope boundaries.

### Phase 5: Current-Work Bead Creation (conditional)

- Create real beads with `br create`; never pseudo-beads in markdown.
- Create only current-work beads. Never create future-story/future-phase beads.
- Beads are allowed in planning only when either condition holds:
  1. mode is `direct_task` or already-proven `small_change`
  2. validating has already returned `READY`/`READY WITH CONSTRAINTS` for this current work and explicitly routed back for bead creation
- For `standard_feature` and `high_risk_feature`, do not create execution beads before feasibility passes.
- For epic/phase work, keep one whole-feature epic open until final reviewing closeout.
- Normalize every bead immediately with required schema fields:
  - `dependencies`, `files`, `verify`, `verification_evidence`, `testing_mode`, `decision_refs`, `learning_refs`
- Use `references/bead-template.md` for bead schema details.
- Fill story/work-to-bead mapping after bead creation.

## STATE + Handoff

After major transitions, update `.pulse/STATE.md` as a mirror of durable artifacts (source of truth remains history files).

If context exceeds 65% at a phase boundary:
- write `.pulse/handoffs/planning.json` with the shared envelope from `../using-pulse/references/handoff-contract.md`
- register it in `.pulse/handoffs/manifest.json`
- keep payload concise and current-work specific (`completed_through`, `artifacts_written`, `beads_created`, `open_questions`, `current_work`)

## Completion Handoff

On success:
- discovery, approach, and approved shape artifact are written
- Gate 2 approval is recorded and synced
- current-work prep artifacts are written
- bead state is explicit:
  - direct/proven-small path: current-work beads may already exist and are normalized
  - feasibility-first path: phase/epic/harder work stops at current-work prep with no execution beads yet
  - post-feasibility path: if validating returns READY/READY WITH CONSTRAINTS and beads are required but absent, planning creates only validated current-work beads before validating resumes
- HIGH-risk components are flagged for feasibility validation

Then hand off with: **Recommend `pulse:validating` for current work as the next skill, default to `next_action: manual_invoke`, and continue in the same session only when the user explicitly asks for it.**

## Red Flags

- Skipping learnings retrieval or ignoring `CONTEXT.md`
- Skipping mode gate
- Proceeding past shape approval without explicit approval
- Defaulting to phases when a work-shape or epic-map is clearer
- Creating future-work beads
- Vague, non-observable exit states
- Prose-only bead scope/verification or missing canonical fields
- HIGH-risk items without concrete YES/NO spike questions
- Missing dependencies across shared files/contexts
