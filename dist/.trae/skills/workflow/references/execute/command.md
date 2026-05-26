# `pulse:workflow execute`

Worker execution operating manual for implementing approved Pulse work items with strict scope, verification, commit, and handoff discipline.

Supports both:

- standalone single-worker execution
- worker execution under `pulse:workflow swarm`

## Mission

Select ready work safely, implement within declared boundaries, verify with fresh evidence, and report or pause without breaking coordination integrity.

## Entry criteria

Run `pulse:workflow execute` only when:

- Gate 3 is explicitly approved
- active current-slice boundaries are known
- execution-ready work items exist

Do not execute when approvals, item contracts, or slice boundaries are ambiguous.

## Loop overview

```text
Initialize -> Get Work Item -> Reserve Scope -> Implement -> Verify -> Close & Report
     ^                                                                     |
     +---------------------- Context OK? loop -----------------------------+
                         Context critical? -> Handoff -> Stop
```

## Step 1 — Initialize

Determine mode from invocation and runtime state:

- if invoked by `pulse:workflow swarm`, run in worker mode
- otherwise run in standalone single-worker mode

### 1a. Restore worker bootstrap context (worker mode only)

Capture and keep these fields together for the run:

- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `epic_id`
- `feature_name`
- optional `startup_hint`

Treat `startup_hint` as a hint, not a silent permanent assignment. Re-check live ready state before you claim work.

### 1b. Read project context (in this order)

1. `AGENTS.md`
2. `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`
3. `.pulse/runtime/state.json`
4. `.pulse/runtime/STATE.md`
5. active current-slice artifacts under `works/` (from runtime state and selected item contract)

If any required file is absent, note the absence and continue without inventing content.

If the item cites decision or learning refs, read cited artifacts only.

### 1c. Report `[ONLINE]` before claiming work (worker mode only)

Before selecting work, post `[ONLINE]` on the active coordination surface including:

- `runtime_identity`
- `AGENTS.md: read`
- `pulse:workflow execute: loaded`
- `Next step: node .trae/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json`

Do not claim work before this startup report.

Runtime mapping:
- Claude Code -> send startup acknowledgment to coordinator via `SendMessage`
- Codex -> reply on the parent coordination thread
- otherwise -> use the active coordination surface defined by adapter/runtime

### 1d. Check for handoff

Use owner-scoped handoffs:

- worker mode -> `.pulse/runtime/handoffs/worker-<runtime_identity>.json`
- standalone mode -> `.pulse/runtime/handoffs/single-worker.json`

If a handoff exists and was written by the same owner identity:

1. read it and restore active item, progress markers, open blockers
2. resume from recorded point without redoing already-complete steps
3. archive or mark consumed and update `.pulse/runtime/handoffs/manifest.json`

A worker must not consume another worker’s handoff directly.

### 1e. Exceptional path: coordinator-reassigned orphaned handoff

Only the coordinator may reassign an orphaned worker handoff when original owner identity is unavailable.

Before resuming a reassigned handoff, require coordinator confirmation that:

- prior worker inactivity was confirmed
- reservations for prior owner were checked and safely transferred
- shared-branch commit queue state was checked and safely transferred

Coordinator must update `.pulse/runtime/handoffs/manifest.json` and the handoff owner file with:

- previous owner
- new owner
- reason
- coordinator approval

If those updates are missing, do not resume from that handoff.

## Step 2 — Get the next work item

In worker mode, every loop starts with coordination visibility, not blind item selection.

Check active coordination surface for:

- new coordinator instructions
- unresolved blocker replies
- conflict decisions
- handoff/recovery instructions

Then inspect live ready queue:

```bash
node .trae/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json
```

Select the highest-priority ready item that:

- has no open dependencies
- is not reserved by another worker
- is compatible with current mode and coordination constraints

### Exceptional path: direct orchestrator hint

If coordinator/swarm suggests an item, treat it as a hint or rescue instruction. Re-check live queue and contract before claiming.

### Read item contract fully

Read the selected item contract from the active workgraph metadata before claiming it.

Minimum fields to confirm:

- `dependencies`
- `scope` or explicit file/path set
- `verify` commands
- `verification_path` or equivalent evidence target(s)
- `testing_mode` (`standard` or `tdd-required`)
- `decision_refs`
- optional `learning_refs`

If required fields are missing or contradictory, stop and route item back for repair.

If `testing_mode=tdd-required`, confirm red/green command steps are present before implementation.

### Current-work artifact read rule for non-trivial items

Do not rely on item summary alone for architecturally or operationally sensitive work.

Before reserving files or writing code, re-read active current-slice artifacts when any are true:

- `testing_mode=tdd-required`
- multi-file scope crosses modules/owners
- multiple upstream dependencies or explicit parallel coordination risk
- integration-heavy or multi-step verification path
- after contract read, more than one plausible implementation path remains

Use active current-slice artifacts in `works/` as canonical current-work contract.

If item touches interfaces, ownership boundaries, or high-risk constraints, also re-read relevant locked decisions in runtime/state-linked artifacts.

## Step 3 — Reserve scope

In worker mode, reserve all declared paths before editing:

```bash
node .trae/skills/workflow/scripts/pulse.mjs reservation reserve --repo-root <repo> --agent <runtime_identity> --item <item-id> --path "src/foo.ts" --path "src/bar.ts" --json
```

In standalone mode there is no cross-worker race, but declared scope remains a hard boundary.

### If reservation fails (worker mode)

1. report `[FILE CONFLICT]` immediately on coordination surface
2. include item ID, requested paths, current holder (if known), and why scope is needed
3. wait for coordinator resolution
4. keep monitoring coordination surface while blocked

Do not proceed without reservations. Do not edit around conflicts.

### If reservation succeeds

Proceed to implementation immediately.

## Step 4 — Implement

### Read before writing

Read every source file you will modify. Do not edit from memory.

### Honor locked decisions

Before writing code, resolve decision IDs from `decision_refs` and implement exactly as locked.

### State assumptions before coding

Before changing files, be explicit about:

- existing path/component being extended
- what the item asks to preserve vs change
- which current-slice done condition this item advances
- what verification result will prove success

If ambiguity remains across plausible interpretations, stop and route contract back for clarification.

### Follow existing patterns

Match repository conventions for naming, imports, error handling, and tests.

Do not create temporary architecture that violates approved boundaries.

### Keep change surgical

Prefer the smallest change that satisfies the item.

Do not:
- add speculative abstractions or refactors
- broaden scope into unrelated cleanup
- add impossible-path handling that current boundary cannot produce

### No pseudo-implementations

Every implementation artifact must be substantive and integrated (wired/imported/used), not TODO-only or stub-only completion.

### Selective TDD

Respect `testing_mode`:

- `standard` -> implement then verify
- `tdd-required` -> run real red/green loop before finalizing

For `tdd-required`:

1. add/update minimal failing test
2. run red command and confirm expected failure
3. make minimal production change
4. run green command and confirm pass

If production code was written before red check, rewrite within scope and re-run proper loop.

## Step 5 — Verify

Run item `verify` commands exactly as contracted. No work item may close without fresh, scoped evidence from this execution pass.

Each completed item must include evidence for:

- commands/tests run
- observed outputs
- artifacts produced
- unresolved gaps, explicitly `None.` when none remain

Use [completion-report-contract.md](completion-report-contract.md) for the completion payload and minimum verification artifact shape.

Before running, be able to state what success looks like in one or two lines. If not possible, item is under-specified and should be repaired.

Verification completes only when:

- verify commands pass in a fresh run
- evidence artifacts in `verification_path` (or declared equivalent) are updated for this run
- outcomes are traceable to current execution pass

Evidence record must include:

- item ID and feature context
- `testing_mode`
- verification timestamp
- every verify command executed
- exit code per command
- concise observed result per command
- paths to generated proofs/logs/screenshots/findings
- explicit unresolved gaps or `None.`

If `testing_mode=tdd-required`, also record red command + observed failure signal and green command + observed passing signal.

If verification fails:

1. debug root cause
2. compare failure against pre-stated success criteria
3. retry up to 2 times
4. if still blocked:
   - worker mode -> notify coordinator and stay blocked
   - standalone mode -> route to explicit debug/fix path

Do not redefine success after failure. Do not close without passing verify and fresh evidence.

## Step 6 — Close and report

All actions here are mandatory. Do not claim next item until completion report is sent/recorded.

### 6a. Close-readiness check

Before close, confirm all true:

- edits stayed within declared scope or approved expansion
- locked decisions still match final implementation
- all verify steps passed in fresh run
- all evidence entries are present and substantive
- no unresolved blocker/finding is silently deferred

### 6b. Close item

Close the completed item through the active workgraph mutation surface only after verification evidence is complete.

### 6c. Atomic commit via coordinator-owned queue (worker mode)

One commit per item. Do not batch multiple items or unrelated changes.

In worker mode, implementation/verification can run in parallel across workers, but `git add`/`git commit` are serialized under coordinator queue on the shared branch.

Protocol:

1. send `READY_TO_COMMIT` with item ID and exact files you will stage
2. wait for `COMMIT_SLOT_GRANTED`
3. after slot grant only:

```bash
git add <files-you-modified>
git commit -m "feat(<item-id>): <summary matching completed work item>"
```

4. commit only declared files
5. report `COMMIT_DONE` with hash, or `COMMIT_BLOCKED` with reason

In standalone mode, use same one-item commit format only when no active swarm commit queue exists for current branch/feature; otherwise route through coordinator queue.

### 6d. Release reservations (worker mode)

```bash
node .trae/skills/workflow/scripts/pulse.mjs reservation release --repo-root <repo> --agent <runtime_identity> --json
```

Release before completion report so others can acquire files immediately.

### 6e. Report completion

- worker mode -> post `[DONE]` with item ID, runtime identity, commit hash, files changed, verify result, evidence paths, follow-up item if any
- standalone mode -> record equivalent completion details in `.pulse/runtime/STATE.md`

### 6f. One coordination check after reporting (worker mode)

Before claiming next item, inspect coordination surface once for follow-up instructions or resolved blockers.

## Step 7 — Loop or pause

After each item:

- if context is below critical threshold, return to Step 2
- if context is critical, write owner handoff and stop cleanly

Write handoff using command-local contract at `skills/workflow/execute/handoff-contract.md`:

- worker mode -> `.pulse/runtime/handoffs/worker-<runtime_identity>.json`
- standalone mode -> `.pulse/runtime/handoffs/single-worker.json`

Register handoff in `.pulse/runtime/handoffs/manifest.json`.

Worker mode: notify coordinator after writing handoff.

## Step 8 — Post-compaction recovery

If context compaction/summarization is detected, stop immediately and re-read before further implementation:

1. `AGENTS.md`
2. `.pulse/runtime/state.json`
3. current item contract from the active workgraph metadata
4. required current-slice artifacts under `works/`
5. active reservations: `node .trae/skills/workflow/scripts/pulse.mjs reservation list --repo-root <repo> --active-only --json`
6. latest coordinator updates on active coordination surface

Resume only after all applicable reads complete.

## Red flags

Stop and reassess if you notice:

- executing without full item contract read
- executing item with missing canonical fields
- editing outside reserved/declared scope
- skipping exact verification criteria
- closing with stale or partial evidence
- claiming `tdd-required` without real red failure and green pass
- continuing after compaction without recovery reads
- implementing stubs/TODOs as completion
- ignoring locked decisions
- guessing through ambiguity instead of routing repair
- bundling multiple items into one commit
- committing in worker mode without `COMMIT_SLOT_GRANTED`
- claiming work without reservation awareness
- blocking/completing without coordination report
- waiting silently while blocked/conflicted/handoff-ready

## Quick reference

| Action | Call |
|---|---|
| List ready items | `node .trae/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json` |
| Read item contract | active workgraph metadata |
| Reserve scope | `node .trae/skills/workflow/scripts/pulse.mjs reservation reserve --repo-root <repo> --agent <runtime_identity> --item <item-id> --path "..." --json` |
| Release scope | `node .trae/skills/workflow/scripts/pulse.mjs reservation release --repo-root <repo> --agent <runtime_identity> --json` |
| List active reservations | `node .trae/skills/workflow/scripts/pulse.mjs reservation list --repo-root <repo> --active-only --json` |
| Close item | active workgraph mutation surface |
| Report lifecycle events | active coordination surface (`[ONLINE]`, `[DONE]`, `[BLOCKED]`, `[FILE CONFLICT]`, `[HANDOFF]`) |

## Inputs from `pulse:workflow swarm` (worker mode)

- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `epic_id`
- `feature_name`
- optional `startup_hint`

If startup inputs are missing, request clarification from coordinator before proceeding.
