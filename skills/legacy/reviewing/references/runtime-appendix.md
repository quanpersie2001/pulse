# Reviewing Runtime Appendix (Canonical)

This appendix contains the operational contract for reviewing.

Prerequisite: `br` must be available. If `br` is unavailable, Gate 4 cannot run; stop reviewing and route to preflight/tooling remediation via `pulse:using-pulse`.

## 1) 4+1 Review Model

Agents 1-4 are specialists. Agent 5 (learnings-synthesizer) always runs after specialist completion.

| Agent | Focus |
|---|---|
| 1 `code-quality` | Simplicity, readability, DRY, error handling, type safety |
| 2 `architecture` | Coupling, boundaries, API design |
| 3 `security` | OWASP risks, authz/authn, secrets, exposure |
| 4 `test-coverage` | Missing tests, edge cases, integration gaps |
| 5 `learnings-synthesizer` | Cross-reference `.pulse/memory/`, repeated failures, compounding candidates |

Dispatch rules:

- `<=4` active agents: parallel by default
- `>=5` active agents: serial auto-switch (inform user)
- `--serial`: always serial

Agent 5 never runs in parallel with agents 1-4.

### Isolated context (critical)

Each reviewer gets only:

1. reviewing-owned changeset diff evidence for the active Pulse closeout path (`git diff <base>..<head>`)
2. `history/<feature>/CONTEXT.md`
3. `history/<feature>/approach.md`

No session chatter, implementation notes, or agent logs.

## 2) Severity Model and Review Bead Rules

| Priority | Label | Criteria | Gate |
|---|---|---|---|
| P1 | CRITICAL | Security vulnerabilities, corruption, breaking behavior | Blocking |
| P2 | IMPORTANT | Performance, architecture, reliability gaps | Non-blocking follow-up |
| P3 | NICE-TO-HAVE | Minor improvement/cleanup/docs | Non-blocking follow-up |

Creation + linkage:

- P1 -> blocking fix bead on current review/epic-close path
- P2/P3 -> non-blocking follow-up beads (`external_ref=<source-epic-id>`, `review` labels)
- P2/P3 must not be parented under current epic close path
- every accepted finding that survives dedupe must become a review bead before Gate 4 is presented
- do not leave accepted findings as prose-only notes without a bead ID

Title pattern:

- `Resolve Review P1: <problem title>`
- `Resolve Review P2: <problem title>`
- `Resolve Review P3: <problem title>`

## 3) Gate 4 Blocking Behavior (hard gate)

If any P1 review bead exists:

- stop progression immediately
- present P1 list with bead IDs and evidence
- do not proceed to finishing while P1 remains open
- if a blocking issue is still only described in prose, create the bead first and then present it as a blocker

User acknowledgment means the user has seen the P1 list and directed a fix path. It never means merge permission with open P1.

When P1 = 0 and Gate 4 approval is granted:
- default approved path: update runtime state with `gate: GATE 4`, `gate_status: approved`, `next_skill_recommended: pulse:compounding`, and `next_action: manual_invoke`, then stop
- optional fast path: continue in the same session only when the user explicitly chooses an equivalent of `Approve and continue now`; in that case set `next_action: continue_now` before loading compounding
- never auto-merge as part of either approval path

## 4) Artifact Verification Contract

Run Level 1 -> Level 2 -> Level 3 for every artifact named in `CONTEXT.md` and `approach.md`:

1. EXISTS
2. SUBSTANTIVE
3. WIRED

Use minimal project-appropriate checks and prefer direct evidence.

Severity mapping:

- L1+L2+L3 pass -> no finding
- L1+L2 only -> P2 review bead
- L1 only (stub) -> P1 review bead
- Missing -> P1 review bead

Artifact-verification findings follow the same bead rule: if the issue is accepted as real, create the review bead before summarizing the review result.

For APIs, config, infra, and CLI flows where human click-through is not applicable, collect non-interactive evidence such as:
- verification command output
- before/after behavior or config diff
- logs proving activation
- relevant automated test output

Treat `history/<feature>/verification/` as canonical evidence.

## 5) Human UAT and Failure Routing

For each SEE/CALL/RUN deliverable:

- ask one item at a time with decision ID reference
- collect Pass / Fail / Skip

On failure:

1. invoke `pulse:systematic-debug-fix`
2. create fix bead
3. execute fix bead
4. re-verify failed item

Skip is recorded in `.pulse/STATE.md` and is not a pass.

## 6) Finishing and Closeout

Precondition before finishing:
- confirm final-slice completion from the active approved shape path plus `.pulse/STATE.md`:
  - work-shape path: `history/<feature>/work-shape.md`
  - phase-plan path: `history/<feature>/phase-plan.md`
  - epic-map path: `history/<feature>/epic-map.md` and `history/<feature>/current-story-pack.md`
- confirm those artifacts agree no later current-work slice remains pending
- if those artifacts disagree, stop and route back to planning/state sync
- do not infer whole-feature completion from an empty epic subtree alone; later slices may not be materialized yet

Use this closeout checklist:

- verify the blocking epic-close path is closed (`bv --robot-triage --graph-root <epic-id>`)
- run final build/test/lint and route failures to blocking review beads
- present merge options
- verify canonical evidence in `history/<feature>/verification/`
- write/refresh `history/<feature>/lifecycle-summary.md`
- clean worktree if used
- close epic bead only after final-slice artifact eligibility check and epic-close-path check both pass
- update Pulse state artifacts

P2/P3 remain non-blocking but must be visible in PR and closeout artifacts.

State update after completion:

```text
STATUS: reviewing-complete
EPIC: <id>
HANDOFF: compounding
FLAGGED_LEARNINGS: <count>
```

## 7) Red Flags

- skipping review without approved lightweight path
- running agent 5 before specialists finish
- bypassing isolated-context contract
- continuing with open P1
- skipping artifact verification
- marking UAT failures as pass
- closing epic with open blocking beads
- closing epic before approved shape artifacts + `.pulse/STATE.md` confirm final-slice completion (`work-shape.md`, `phase-plan.md`, or `epic-map.md` + `current-story-pack.md`)
- parenting P2/P3 under current epic instead of external_ref linkage

## 8) Deprecated Finding Files

Per-finding markdown files are retired. The bead body is the canonical review artifact.
