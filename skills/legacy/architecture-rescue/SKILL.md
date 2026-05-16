---
name: architecture-rescue
description: >-
  Repo-wide or subsystem-wide architecture hygiene pass that surfaces deepening
  opportunities, shallow modules, leaky seams, and ownership drift. Default mode
  is report-only: no bead creation or execution unless explicitly requested.
metadata:
  version: '1.0'
  ecosystem: pulse
  dependencies: []
---

# Architecture Rescue

Use this skill when the user wants architecture cleanup that is broader than a single diff: repo-wide, subsystem-wide, or cross-module shape correction.

This skill is for **analysis and proposal** first. By default it is **report-only**.

## What this is (and is not)

- **This skill**: architecture rescue and hygiene across seams, ownership, interfaces, and module depth.
- **Not diff-scoped cleanup**: this skill is for system shape, seams, and ownership, not for small behavior-preserving cleanup on a concrete diff.
- **Not normal planning**: planning slices approved feature work into phases/stories/beads; architecture-rescue diagnoses architecture friction and proposes rescue candidates before planning.

## Default mode

Default to `report-only` unless the user explicitly asks for planning/execution follow-through.

In `report-only` mode:

- do not create beads
- do not invoke `pulse:planning`
- do not invoke `pulse:executing`
- do not edit architecture artifacts unless the user asks

## Inputs to read first

For the target scope (whole repo or named subsystem), read:

1. project operating docs (`CLAUDE.md`, `AGENTS.md`, and nearby module docs)
2. relevant `history/<feature>/CONTEXT.md` decisions when present
3. `.pulse/memory/*` entries that apply to architecture constraints, incidents, or non-regression rules
4. `pulse:gitnexus` and GitNexus context if available

If GitNexus is configured, prefer it for topology, process flow, and impact/breadth evidence before manual grep.

## Architecture lens

Use consistent terms from `references/LANGUAGE.md`.

Look specifically for:

1. **Shallow modules**: interface complexity close to implementation complexity
2. **Bloated interfaces**: callers need to know too many invariants/orderings/error modes
3. **Leaky seams**: cross-module coupling forces parallel edits or duplicated policy
4. **Ownership drift**: responsibilities spread across multiple modules with unclear source of truth
5. **Deepening opportunities**: ways to concentrate behavior behind smaller, clearer interfaces

Use the deletion test: if deleting a module merely moves complexity to callers, it was likely earning leverage; if it removes no real complexity, it was likely shallow or pass-through.

## Process

### 1) Scope and map

- Confirm scope: whole repo, bounded subsystem, or named execution flow.
- Build a quick architecture map: modules, seams, owners, and flow handoffs.

### 2) Find rescue candidates

Generate a short list (3-7) of high-signal candidates with concrete evidence.

For each candidate capture:

- **Area/files**
- **Current friction**
- **Rescue move**
- **Expected leverage/locality gain**
- **Risk/coordination cost**

### 3) Produce a report

Return a ranked architecture rescue report:

1. **Executive summary** (what hurts most and why)
2. **Candidate list** (ranked)
3. **Suggested rescue sequence** (what to do first/second/later)
4. **Open questions** that must be answered before execution

Do not generate implementation tasks unless asked.

## Optional follow-through modes (explicit opt-in only)

Use only when the user explicitly requests them.

- `phase-shaping`: hand off top approved candidate(s) to `pulse:planning`
- `execution-ready`: after validating gate approval, proceed via `pulse:swarming`/`pulse:executing`

Before leaving report-only mode, restate the mode change and requested scope.

## Output contract (report-only)

Provide concise output with:

- scope analyzed
- top rescue opportunities
- rationale in leverage/locality terms
- dependencies and risks
- recommended first move

Keep recommendations specific, evidence-backed, and bounded to the requested scope.

## References

- [LANGUAGE.md](references/LANGUAGE.md)
- [DEEPENING.md](references/DEEPENING.md)
- [INTERFACE-DESIGN.md](references/INTERFACE-DESIGN.md)
