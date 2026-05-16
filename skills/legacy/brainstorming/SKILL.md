---
name: brainstorming
description: >-
  Use before feature work only when the design is still unclear — for shaping vague
  product intent, comparing possible feature shapes, and turning fuzzy requests into a
  documented, approved spec before exploring or planning begins. Do not use this when
  the feature intent is already clear and the remaining work is to lock implementation
  decisions; that belongs to pulse:exploring. Trigger phrases: design, brainstorm,
  think through, what should we build, help me think, explore options, compare directions,
  unclear feature shape, vague product request.
metadata:
  version: '1.2'
  ecosystem: pulse
  position: '1b of 9 — optional pre-step before pulse:exploring'
  dependencies:
    - id: node-runtime
      kind: command
      command: node
      missing_effect: degraded
      reason: Optional visual brainstorming runtime uses a local Node server for advanced UI comparisons.
---

# Brainstorming Skill

Turns vague intent into a documented, approved design spec through structured dialogue —
before any codebase research or implementation planning begins.

Research shows that validated designs reduce planning rework by stopping assumption drift before it compounds.

This skill should behave like a disciplined design grilling session:
- walk the design tree until no planning-critical branch is unresolved
- ask the user only what the repo and docs cannot already answer
- when a strong default exists, include a recommended answer so the user can confirm or override it quickly

<HARD-GATE>
Do NOT invoke any implementation skill, write any code, create beads, or take any
implementation action until a design has been presented AND the user has approved it.
This applies regardless of perceived simplicity. The design can be short — but it MUST
exist and be approved.
</HARD-GATE>

## Anti-Pattern: "This is too simple to need a design"

Every new feature with unclear design goes through this process. A new config capability, a new function that expands behavior, a new UI path — if the shape is still fuzzy, this is where you turn it into an approved spec. "Simple" feature work is where unexamined assumptions cause the most wasted planning work. The spec can be a paragraph. But you MUST present it and get approval.

This is not the path for trivial non-feature corrections already covered by `small_change` or `Micro Mode`. If the request is only a wording fix, local correction, or similarly bounded non-feature adjustment, follow the lighter route defined by `pulse:using-pulse` instead of forcing a brainstorming spec.

---

## Quick Reference

| Step | What you do | What you produce |
|---|---|---|
| Explore context | Read just enough project material to understand what already exists | Internal context snapshot |
| Assess scope | Decide whether this is one feature or multiple independent systems | Scoped brainstorming target |
| Visual decision point | Decide whether upcoming questions are easier to answer by seeing options | User consent for visual support, or text-only path |
| Clarifying questions | Ask one question at a time to uncover purpose, constraints, and success criteria | Validated requirements |
| Approaches | Present 2–3 viable directions with trade-offs | Chosen direction |
| Design | Present the solution in sections and validate incrementally | Approved design |
| Spec | Write the approved design to `history/<feature>/spec.md` | Stable spec for exploring |
| Self-review | Run the spec reviewer subagent and fix serious issues | Planning-ready spec |
| User review gate | Wait for explicit approval on the written spec | Approved handoff artifact |
| Handoff | Update `.pulse/STATE.md` and recommend `pulse:exploring` as the next manual step | Clean pipeline transition |

---

## Checklist

Create a task for each item and complete them in order:

1. **Explore project context** — read files, docs, recent commits relevant to the request
2. **Assess scope** — is this one feature or multiple independent systems? (see Decomposition)
3. **Offer visual support** — if upcoming questions would be easier to answer by seeing options, offer visuals in their own message
4. **Ask clarifying questions** — one at a time; purpose, constraints, success criteria
5. **Propose 2–3 approaches** — with trade-offs and your recommendation
6. **Present design** — in sections scaled to complexity; get user approval after each section
7. **Write spec doc** — save to `history/<feature>/spec.md` and note the path
8. **Spec self-review** — subagent check for placeholders, contradictions, scope, ambiguity
9. **User reviews spec** — ask user to confirm before proceeding
10. **Handoff to exploring** — recommend `pulse:exploring` as the next manual step to lock implementation decisions

---

## Process Flow

```dot
digraph brainstorming {
    "Explore project context" [shape=box];
    "Multi-system?" [shape=diamond];
    "Decompose first" [shape=box];
    "Visual questions ahead?" [shape=diamond];
    "Offer visual support\n(own message only)" [shape=box];
    "Ask clarifying questions" [shape=box];
    "Propose 2-3 approaches" [shape=box];
    "Present design sections" [shape=box];
    "User approves design?" [shape=diamond];
    "Write spec doc" [shape=box];
    "Spec self-review (subagent)" [shape=box];
    "User reviews spec?" [shape=diamond];
    "Recommend pulse:exploring handoff" [shape=doublecircle];

    "Explore project context" -> "Multi-system?";
    "Multi-system?" -> "Decompose first" [label="yes"];
    "Multi-system?" -> "Visual questions ahead?" [label="no"];
    "Decompose first" -> "Visual questions ahead?";
    "Visual questions ahead?" -> "Offer visual support\n(own message only)" [label="yes"];
    "Visual questions ahead?" -> "Ask clarifying questions" [label="no"];
    "Offer visual support\n(own message only)" -> "Ask clarifying questions";
    "Ask clarifying questions" -> "Propose 2-3 approaches";
    "Propose 2-3 approaches" -> "Present design sections";
    "Present design sections" -> "User approves design?";
    "User approves design?" -> "Present design sections" [label="no, revise"];
    "User approves design?" -> "Write spec doc" [label="yes"];
    "Write spec doc" -> "Spec self-review (subagent)";
    "Spec self-review (subagent)" -> "User reviews spec?";
    "User reviews spec?" -> "Write spec doc" [label="changes requested"];
    "User reviews spec?" -> "Recommend pulse:exploring handoff" [label="approved"];
}
```

**The terminal state is an approved `spec.md` plus `.pulse/STATE.md` update, with a recommendation to run `pulse:exploring` next.** Do NOT invoke planning, validating,
or any execution skill by default. After brainstorming, the ONLY valid next step is `pulse:exploring`.

---

## Phase 1: Explore Context

Before asking any question, understand what already exists:

- Read `.pulse/project-docs.json` first when present, then read the listed project docs before relying on feature history alone
- If `.pulse/project-docs.json` is absent, detect likely project docs (README, architecture, ADR, domain docs) and read the smallest relevant set
- Reuse existing glossary/terminology from project docs when present; do not invent new terms when established names already exist
- Before asking, eliminate questions the repo can already answer. Read docs, code, and recent repo history first, then ask only for unresolved design intent.
- Check relevant files, docs, and the last few commits related to the topic
- Identify existing patterns, components, or decisions that constrain the design
- Note what can be reused vs. what needs to be created from scratch

Build an internal picture first — it makes your clarifying questions concrete instead of generic.

---

## Phase 2: Scope Assessment

Before asking detailed questions, assess whether the request is one feature or several:

**One feature** — scoped work with a clear boundary. Continue normally.

**Multiple independent systems** — e.g., "build a platform with auth, billing, and analytics."
Flag this immediately:

> "This covers [A], [B], and [C] — three independent systems. Each needs its own
> brainstorming session. Let's start with [most foundational]. I'll note the others for later."

Then brainstorm the first sub-system through the full flow. Each sub-system gets its own
spec → exploring → planning → execution cycle.

### Step-back move — use selectively

Before detailed questioning, decide whether the request needs one brief step-back pass.
This is not a new phase and not a replacement for sequential questioning. It is a short
framing move to help you ask better questions.

Use it only when one of these is true:
- the request names a solution but not the problem it should solve
- multiple feature shapes could satisfy the request
- the user is jumping quickly into screens, components, or flows before the core outcome is clear
- you notice yourself optimizing a local detail before the product goal is concrete

If you use the move, do it once, briefly, before Phase 3:
1. name the core outcome in plain language
2. name 2–4 decision axes that matter most
3. identify what should not be optimized yet
4. turn that framing into the next single question

Keep the output internal unless a short external framing statement will help the user align.
Do not turn the step-back move into a mini-plan, a multi-question bundle, or an excuse to skip the structured question flow.

Example internal frame:
- Outcome: "Help a first-time user complete X confidently."
- Decision axes: primary user, success event, scope boundary, failure tolerance
- Not yet: exact layout, polish details, implementation mechanics

---

## Visual Decision Point

When upcoming questions involve layout, visual hierarchy, diagrams, flows, or side-by-side
interface choices, offer visual support once before continuing.

Use this offer as its own message:

> "Some of this may be easier to evaluate if I show concrete options instead of only describing them in text. I can use inline previews or small mockups for the visual decision points. Want me to do that when it helps?"

<HARD-GATE>
This offer MUST be its own message. Do NOT combine it with a clarifying question,
a context summary, or a recommendation. Ask, wait, then continue.
</HARD-GATE>

**How to decide:**

- **Use visuals** for layout comparisons, information hierarchy, diagrams, wireframes, and other questions where seeing options will reduce ambiguity.
- **Stay in text** for goals, scope, constraints, prioritization, trade-offs, and conceptual choices.
- A UI topic is not automatically visual. "Which outcome matters most?" is text. "Which dashboard layout is closer?" is visual.

**How to present visual choices:**

- Prefer `AskUserQuestion` with `preview` for side-by-side concrete artifacts.
- If the active harness offers another structured question tool instead of `AskUserQuestion`, use that tool rather than asking a plain-text visual question.
- Escalate to the local visual server only for genuinely complex visual ambiguity: styling direction, multi-screen flow shape, design-system composition, dense layout comparison, or hierarchy questions where a browser-rendered screen will clarify faster than previews.
- Start it with `scripts/start-visual-server.sh --project-dir <repo-root>`.
- If startup returns a `url`, tell the user the visual runtime is active, share the exact URL, tell them to open it in a browser, make their selection there, and return to the terminal after interacting.
- If startup returns an `error` or Node is unavailable, briefly tell the user the runtime could not be used, surface any useful retry hint, and continue with structured question-tool previews or text-only fallback if no question tool exists. Do NOT block the session on the runtime.
- After serving a visual screen, read `state_dir/events` on the next turn to pick up browser selections.
- Keep choices focused — 2–4 options max.
- Use single-select for competing directions; use multi-select only for independent add-on ideas.
- If a preview, mockup, or browser screen will not make the decision clearer, do not create one.

Accepting visual support does NOT turn the whole session visual. Decide per question whether text or visuals are the better fit.

---

## Phase 3: Clarifying Questions

<HARD-GATE>
Ask ONE question at a time. Wait for the user's response before the next.
Do NOT batch questions. Do NOT answer your own questions.
If the active harness provides `AskUserQuestion`, `AskMeTool`, or another structured question tool, you MUST use it for every brainstorming question.
Do NOT ask a plain-text question while a structured question tool is available.
Only fall back to plain-text questions when no structured question tool exists in the current harness.
This gate is non-negotiable — sequential questioning surfaces significantly more
latent requirements than batched approaches.
</HARD-GATE>

**Rules:**

- One question-tool invocation per turn — or one plain-text fallback question when no tool exists
- Use structured question tools in this order when available: `AskUserQuestion` → `AskMeTool` → another equivalent harness-native question tool
- Multiple-choice preferred over open-ended when possible
- Start broad (what, why, for whom) then narrow (constraints, edge cases, success criteria)
- For every question, include a clearly labeled recommended answer when a strong default exists
- Keep walking the decision tree until each design-critical branch is chosen, rejected, delegated, or explicitly deferred
- Do not stop after collecting preferences if a downstream planner would still need to guess
- If the request is still shapeless after context review, use one brief step-back move before the next question so the question targets the real decision instead of a local detail
- 3–4 questions per topic area, then checkpoint with the structured question tool when available: "More questions about [area], or move on?"
- Do not mix plain-text questions and tool-based questions arbitrarily inside the same session

**Question patterns:**

- **Product intent / constraints** → structured multiple-choice or short open-ended question via the harness question tool when available
- **Competing layouts / hierarchy / flows** → offer visual support first, then use structured previews, mockups, or the advanced runtime when needed
- **Trade-off choice** → keep it in text unless the trade-off is inherently visual, but still ask through the harness question tool when available
- **Checkpointing** → after a few questions on one area, confirm whether to continue or advance with the harness question tool when available

Examples:
- Text: "Which primary outcome should this optimize for first?"
- Visual: "Which of these three dashboard layouts is closer to the experience you want?"

**Scope creep** — when the user suggests something out of scope:
> "[Feature X] is a new capability — that's its own work item. I'll note it as a
> deferred idea. Back to [current topic]: [return to question]"

---

## Phase 4: Propose Approaches

Present 2–3 different approaches before committing to one:

- Describe each option concisely with its trade-offs
- Lead with your recommended option and explain why
- Invite the user to push back or ask about specific trade-offs

Do NOT start designing until the user picks a direction.

---

## Phase 5: Present Design

Once the direction is clear, present the design:

- Scale each section to its complexity — a few sentences if simple, 200–300 words if nuanced
- Ask "Does this look right so far?" after each section before moving to the next
- Cover: architecture, key components, data flow, error handling, testing strategy
- Be ready to revise — if something doesn't make sense, go back and clarify

**Design for isolation:**

- Break the system into units each with one clear purpose and well-defined interfaces
- For each unit: what does it do, how do you use it, what does it depend on?
- For new feature work, define module ownership from the start so each unit can evolve and optimize independently.
- A narrow first phase is acceptable only when the ownership model and final boundaries are already correct.
- Do not collapse multiple future concerns into one temporary implementation just to ship an MVP-shaped first version.
- Can someone understand a unit without reading its internals? Can you change the internals
  without breaking consumers? If not, the boundaries need work.

**Working in existing codebases:**

- Follow existing patterns — don't propose unrelated refactoring
- Where existing code has problems that affect the work (a file that's grown too large,
  tangled responsibilities), include targeted improvements as part of the design
- Stay focused on what serves the current goal

---

## Phase 6: Write Spec Doc

After the user approves the design, write the spec:

**Path:** `history/<feature-slug>/spec.md`

The spec must include:
- Problem statement and goals
- Approved approach (one sentence summary)
- Architecture and key components
- Data flow
- Error handling approach
- Testing strategy
- Out of scope (explicitly list what was deferred)

---

## Phase 7: Spec Self-Review

After writing the spec, spawn a subagent to review it:

```
You are a spec document reviewer. Verify this spec is complete and ready for planning.

Spec to review: history/<feature>/spec.md

What to check:
| Category     | What to Look For |
|--------------|-----------------|
| Completeness | TODOs, placeholders, "TBD", incomplete sections |
| Consistency  | Internal contradictions, conflicting requirements |
| Clarity      | Requirements ambiguous enough to cause building the wrong thing |
| Scope        | Focused enough for a single feature cycle |
| YAGNI        | Unrequested features, over-engineering |

Calibration: Only flag issues that would cause real problems during planning.
Approve unless there are serious gaps.

Output:
Status: Approved | Issues Found
Issues (if any): [Section] — [specific issue] — [why it matters for planning]
Recommendations (advisory, do not block): [suggestions]
```

- If Issues Found: fix inline, re-spawn reviewer, repeat (max 2 iterations)
- After 2 iterations with unresolved issues: ask the user to review directly

---

## Phase 8: User Review Gate

After the spec self-review passes:

> "Spec written to `history/<feature>/spec.md`. Please review it and let me know if
> you want any changes before we start locking implementation decisions."

Wait for the user's response. If they request changes: make them, re-run the self-review
loop. Only proceed once the user approves.

---

## Phase 9: Handoff

After user spec approval:

1. Update `.pulse/STATE.md`:
   ```
   Current: brainstorming complete for <feature>
   Spec: history/<feature>/spec.md
   Next: invoke pulse:exploring to lock implementation decisions
   ```

2. Present to user:
   > "Spec approved. Next step: run `pulse:exploring` to extract implementation decisions
   > (gray areas, scope boundaries, and locked choices) before planning begins."

3. Optional continue-now path:
   - Only if the user explicitly asks to continue now, invoke `pulse:exploring` in the same session.
   - Otherwise stop after the spec approval + `.pulse/STATE.md` update + recommendation above.

<HARD-GATE>
Do NOT invoke planning, create beads, or write any code.
The terminal state of this skill is a written, approved spec and STATE update.
The ONLY valid next step is pulse:exploring (manual by default, or invoked now only when the user explicitly asks).
</HARD-GATE>

---

## Key Principles

- **One question at a time** — never overwhelm
- **Multiple-choice preferred** — easier to answer than open-ended
- **Question relentlessly, but only about decisions the repo cannot already answer**
- **Use visuals only when they clarify** — seeing options should remove ambiguity, not add noise
- **Honor existing glossary first** — prefer established project terminology and call out conflicts early
- **YAGNI ruthlessly** — remove unrequested features from all designs
- **Always propose alternatives** — 2–3 approaches before settling
- **Incremental validation** — present design in sections, get approval before continuing
- **Be ready to revise** — go back and clarify when something doesn't fit
- **Do not scaffold project docs here** — brainstorming can consume existing docs but durable project-doc scaffolding belongs to exploring when needed

---

## What This Skill Does NOT Do

These are `pulse:exploring`'s responsibilities — not yours:

- Deep codebase research (only quick context reads here)
- Locking implementation decisions with stable IDs (D1, D2...)
- Gray area extraction against domain probes
- Writing `CONTEXT.md` (exploring does this)
- Creating beads or work items

Brainstorming delivers a design spec. Exploring delivers locked decisions.
Planning consumes both.

---

## Red Flags

Stop immediately if you catch yourself doing any of these:

- Writing code or pseudocode during the design phase
- Asking two questions in the same message
- Asking a plain-text question while `AskUserQuestion`, `AskMeTool`, or another structured question tool is available
- Offering visual support and a clarifying question in the same message
- Skipping the spec because the feature "seems obvious"
- Answering a question you just asked
- Treating every UI topic as visual instead of deciding per question
- Invoking planning or executing before spec is approved
- Creating beads or referencing bead IDs

---

## Anti-Patterns

**"The user wants to move fast"**
Speed comes from clarity. A 10-minute design session prevents hours of planning rework
caused by wrong assumptions baked into beads.

**"I already know what to build"**
Your assumptions are hypotheses until the user validates them.
Run Phase 3 and let the user confirm the direction.

**"This is too small to document"**
The spec can be three sentences. But it MUST exist so exploring has a stable target.

**"This is a visual topic, so every question should be a mockup"**
No. Use visuals only when seeing options will remove ambiguity. Goals, priorities, and constraints still belong in plain text.

---

## References

- `references/spec-reviewer-prompt.md` — subagent prompt for spec document review
- `references/visual-support-guidance.md` — when to use previews vs the advanced visual runtime during brainstorming
