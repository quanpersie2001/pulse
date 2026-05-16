---
name: exploring
description: >-
  Use before implementation planning when feature intent is clear but implementation
  decisions are still unresolved, and the user needs those decisions locked first.
metadata:
  version: '1.0'
  ecosystem: pulse
  position: '2 of 9 — runs after using-pulse, before planning'
  dependencies: []
---

# Exploring Skill

If `.pulse/tooling-status.json` is missing, stale, or reports `FAIL`/`blocked` for the current repo, stop and invoke `pulse:preflight`, then return through `pulse:using-pulse` before continuing.

Spend 5–10 minutes extracting decisions before planning. It prevents hours of rework from planner assumptions ([GSD README](https://github.com/gsd-build/get-shit-done)).
This skill captures those decisions so downstream agents build what the user actually wants.

This skill should behave like a domain-aware grilling loop:
- walk each unresolved branch until planning no longer has to guess
- prefer source-backed answers over avoidable user questions
- sharpen terminology as decisions crystallize instead of letting fuzzy language leak downstream

## When to Use This Skill

Load when the user presents a feature request, asks to build or change something, or when
requirements are fuzzy enough that downstream agents would need to make implementation assumptions.
For Pulse `small_change`, produce an approved mini `CONTEXT.md` even when decisions are self-evident; skip only for trivial non-Pulse corrections outside the Pulse workflow.

## Config Check

If `.pulse/config.json` contains `"skip_exploring": true`, this skill should be skipped only when `history/<feature>/CONTEXT.md` already exists and has explicit human approval. However:
- Still require approved `CONTEXT.md` to exist before planning proceeds
- If no approved `CONTEXT.md` exists and exploring is skipped, hard-stop and recommend that the human provide or approve a mini `CONTEXT.md` before planning
- Quick/small-change paths still need an approved mini `CONTEXT.md`; this config is for standard/deep features where the user has pre-written CONTEXT.md

---

## Process

You MUST complete these phases in order.

When task-list tooling is available in the runtime, create and track one task per phase before starting each phase.
When task-list tooling is unavailable, use an explicit phase checklist in responses (and mirror it in `.pulse/STATE.md` when touched) with only one phase marked active at a time so ordering remains enforceable.

---

### Phase 0: Scope Assessment

**Step 0.1 — Classify scope**

Assess from the request + a 30-second project scan:

- **Quick** — bounded, low ambiguity (e.g., rename a flag, tweak a label). Skip to Phase 2.
- **Standard** — normal feature with decisions to extract. Run all phases.
- **Deep** — cross-cutting, strategic, or highly ambiguous. Run all phases with extra depth.

If scope is unclear, ask ONE disambiguation question before continuing.

**Step 0.2 — Load project docs and prior context**

Read project docs first when available:

```
Read first (if exists):
- .pulse/project-docs.json                 ← canonical project-doc index and status
```

If `.pulse/project-docs.json` exists, read the smallest relevant listed docs before relying on feature history alone.
If `.pulse/project-docs.json` is missing, detect likely project docs (README, architecture, ADR, domain docs) and read the smallest relevant set.

Then load prior context:

```
Read (if exists):
- history/<feature>/spec.md                ← approved brainstorming design, when present
- .pulse/memory/critical-patterns.md       ← promoted critical learnings
- .pulse/STATE.md                          ← any prior feature context
```

If `history/<feature>/spec.md` exists, read it first and translate the approved design into locked decisions in `CONTEXT.md`; do not leave `spec.md` as a parallel source of truth.

Build an internal summary of prior decisions. Use it to skip already-answered questions
and annotate options with "Previously decided: X."

**Step 0.3 — Multi-system decomposition check**

Does the request describe multiple independent subsystems? If yes:
> "This covers [A], [B], and [C] — three independent systems. Each needs its own exploring
> session. Let's start with [most foundational]. I'll note the others for later."

**Step 0.4 — Step-back move for decision framing**

Before domain classification, decide whether you need one brief step-back pass. This is optional framing, not a new phase and not a replacement for Socratic questioning.

Use it only when:
- the request is clear but the decision surface is unclear
- gray areas are blending together and outcome vs implementation is not separated
- discussion drifts into libraries/components before behavior is framed
- the next question would not change planning assumptions

If used, do it once:
1. restate the required outcome
2. name 2–4 decision axes
3. name what is out of exploring scope
4. ask the next single question

Keep it short and internal unless a brief external framing line helps alignment. Never let it become a mini-plan, architecture review, or bundled questions.

---

### Phase 1: Domain Classification

Classify what is being built so Phase 2 probes the right ambiguity surface.

Use the canonical type definitions in `references/gray-area-probes.md` (SEE/CALL/RUN/READ/ORGANIZE).
One feature can span types (for example, SEE + CALL). Classify all that apply before moving on.

---

### Phase 2: Gray Area Identification

Generate 2–4 gray areas for this feature using the domain probes from `gray-area-probes.md`.

A gray area is a decision that:
- Affects implementation specifics
- Was not stated in the request
- Would force the planner to make an assumption without it
- Includes glossary conflicts, overloaded terms, and code-vs-docs-vs-user contradictions when they can change implementation behavior

**Quick codebase scout** (grep only — no deep analysis):

Run a minimal keyword scan over likely source roots, read 2-3 relevant files, and annotate options with what already exists.
Keep this scout shallow; deep codebase analysis belongs to planning.

Filter OUT of gray areas:
- Technical implementation details (architecture, library choices)
- Performance concerns
- Scope expansion (new capabilities not requested)

Terminology discipline for gray areas:
- Reuse glossary terms from project docs when available
- If user language conflicts with project glossary, call out the conflict explicitly and ask which term/meaning should be locked
- If the wording materially changes implementation assumptions, cross-check the statement against both docs and quick code scout evidence before locking

---

### Phase 3: Socratic Exploration

<HARD-GATE>
Ask ONE question at a time. Wait for the user's response before asking the next question.
Do NOT batch questions. Do NOT answer your own questions.
Do NOT proceed to Phase 4 until all gray areas have been discussed and decisions locked.
This gate is non-negotiable. Elicitron (2024) demonstrates sequential questioning
identifies significantly more latent needs than batched approaches.
</HARD-GATE>

**Rules (apply without exception):**

1. One question per message — never bundled
2. Single-select multiple choice preferred over open-ended
3. Start broad (what/why/for whom) then narrow (constraints, edge cases)
4. If a question can be answered from the repo, project docs, or quick scout evidence, answer it there and ask only the remaining decision
5. For each question, provide a recommended answer when a plausible default exists
6. When discussing boundaries or relationships, introduce a concrete scenario or edge case to force precision
7. If user language conflicts with project docs or quick code evidence, stop and resolve the contradiction before locking a decision
8. If a gray area still feels muddy, use one brief step-back move before the next question so you ask about the decision that changes planning, not a local implementation detail
9. 3–4 questions per gray area, then checkpoint:
   > "More questions about [area], or move to next? (Remaining: [unvisited areas])"

**Scope creep response** — when the user suggests something outside scope:
> "[Feature X] is a new capability — that's its own work item. I'll note it as a
> deferred idea. Back to [current area]: [return to current question]"

**Decision locking** — after each gray area is resolved:
> "Locking decision [D_N]: [summary of decision]. Confirmed?"

Assign stable IDs in sequence: D1, D2, D3... These IDs are referenced in CONTEXT.md and
by all downstream agents. Do not reuse or renumber IDs once assigned.

---

### Phase 4: Context Assembly

Durable ambiguity rule:
- Exploring is the primary place to propose lazy project-doc scaffolding when repeated ambiguity is clearly project-level (not feature-local) and existing docs cannot resolve it.
- Keep this as a proposal, not an automatic write path. Confirm with the user before any scaffold action.
- Even when scaffolding is proposed, `history/<feature>/CONTEXT.md` remains the feature source of truth for locked implementation decisions.
- If repeated ambiguity is project-level rather than feature-level, add a `Project Docs Follow-up` naming the exact target: root `CONTEXT.md`, `CONTEXT-MAP.md` entry, or ADR candidate. Propose it here, but do not scaffold or edit repo docs without explicit approval.

**Step 4.1 — Write CONTEXT.md**

```
Path: history/<feature-slug>/CONTEXT.md
```

Load `references/context-template.md` and populate every section. Rules:
- Locked decisions must be concrete: "Card-based layout, not timeline" not "modern feel"
- If `spec.md` exists, every approved design choice that affects implementation must become a locked decision or an explicit deferred/non-decision in `CONTEXT.md`
- Code context must cite file paths found during the scout
- Open questions must be split: "Resolve Before Planning" vs "Deferred to Planning"
- Every locked decision must reference its stable ID (D1, D2...)
- Project-doc alignment must say whether existing repo terminology was reused, corrected, or found missing

**Step 4.2 — Self-review via subagent**

Spawn a fresh subagent using `references/context-reviewer-prompt.md` (never pass session history).

- If Issues Found: fix, re-spawn reviewer, repeat
- Maximum 2 iterations before asking the user to review directly

---

### Phase 5: Handoff

After CONTEXT.md passes review:

1. Update `.pulse/STATE.md`:
   ```
   Current: exploring complete for <feature>
   CONTEXT.md: history/<feature>/CONTEXT.md
   Locked decisions: D1...D_N
   Next skill recommended: pulse:planning
   Next action: manual_invoke
   ```

2. Present to user:
   > "Decisions captured. CONTEXT.md written to `history/<feature>/CONTEXT.md`.
   > CONTEXT.md is now the single source of truth for all downstream agents.
   > Gate 1 approval should update runtime state with `gate_status: approved`, `next_skill_recommended: pulse:planning`, and `next_action: manual_invoke` by default.
   > If the user wants to keep the same context/model, they can explicitly choose an equivalent of 'Approve and continue now'; otherwise planning stays a manual invoke."

<HARD-GATE>
Do NOT invoke planning, write code, create beads, or take any implementation action.
The terminal state of this skill is writing CONTEXT.md and announcing handoff.
The ONLY valid next step is the user invoking the pulse:planning skill.
</HARD-GATE>

---

## What This Skill Does NOT Do

Planning owns the work below, not exploring:

- external pattern/library research
- deep codebase analysis (only quick grep here)
- code, pseudocode, or implementation sketches
- bead creation or implementation approach proposals
- architecture or technical solution design

Your output is locked decisions in CONTEXT.md so downstream agents can proceed without re-asking the user.

---

## Anti-Patterns

**"This is too simple to need exploring"**
Every standard-scope request goes through this process. The CONTEXT.md can be short.
But downstream agents will make assumptions without it — and those assumptions compound.

**"I already know what to build"**
Your assumptions are hypotheses until the user confirms them.
Run Phase 3 and let the user lock the decisions.

**"The user wants to move fast"**
Speed comes from clarity. A 10-minute exploring session prevents a 2-hour planning rework
caused by locked decisions that contradicted what the user actually wanted.

---

## Red Flags

Stop immediately if you catch yourself doing any of these:

- Answering a question you just asked (HARD-GATE violation)
- Writing code or suggesting a library
- Asking two questions in the same message
- Skipping Phase 3 because the feature "seems obvious"
- Creating beads or referencing bead IDs
- Running deep codebase analysis instead of quick grep

---

## References

- `references/gray-area-probes.md` — probes per domain type (SEE/CALL/RUN/READ/ORGANIZE)
- `references/context-template.md` — the CONTEXT.md template to populate in Phase 4
- `references/context-reviewer-prompt.md` — canonical prompt for the Phase 4.2 self-review subagent
