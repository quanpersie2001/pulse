# `pulse:workflow execute`

Single-worker and swarm-worker execution manual for implementing Gate 3-approved Pulse v2 workgraph items.

Execute answers:

> Can this validated TASK/BUG be implemented, verified with fresh evidence, closed, committed atomically, and reported without changing the approved plan or solution?

Execute is intentionally narrow. It implements approved executable items only. It does not admit new work, choose solution design, decompose tasks, validate readiness, review quality, or approve merge/release.

Supports both:

- standalone single-worker execution after validate recommends `single-worker`
- worker execution under `pulse:workflow swarm`

## Mission

Implement one execution-ready `TASK` or `BUG` at a time from the approved current slice, within declared scope, with fresh verification evidence, atomic close/commit discipline, and safe handoff/reporting.

## Entry criteria

Run `pulse:workflow execute` only when all are true:

- Gate 3 is explicitly approved for the current slice
- runtime state, workgraph metadata, and story artifacts agree on the active epic/story/slice
- the executable unit is a `TASK` or `BUG`, not an `EPIC` or `STORY`
- the item has a complete execution contract in its README
- the item has concrete verification evidence target(s), normally its workgraph `verification_path`
- execution mode matches invocation:
  - standalone execute -> validate recommended `single-worker`
  - swarm worker -> invoked by `pulse:workflow swarm` with worker bootstrap context

Do not execute when approvals, item contracts, runtime posture, reservations, or current-slice boundaries are ambiguous.

If entry fails, reroute precisely:

| Failure | Reroute |
| --- | --- |
| Runtime/session posture unclear or stale | `pulse:workflow use` |
| Gate 3 missing, pending, rejected, or not current | `pulse:workflow validate` |
| TASK/BUG contract, file scope, dependencies, or verification plan are incomplete | `pulse:workflow plan` |
| Solution decision refs are missing, contradictory, or infeasible | `pulse:workflow design` |
| Discovery/evidence is missing for a blocking implementation fact | `pulse:workflow explore` |

## Loop overview

```text
Initialize -> Select TASK/BUG -> Read Contract -> Reserve Scope -> Mark In Progress
     -> Implement -> Verify Evidence -> Close -> Commit -> Release & Report
        ^                                                               |
        +---------------- Context OK? / More current-slice work? -------+
                         Context critical? -> Handoff -> Stop
```

## Command-local references

- [runtime-appendix.md](runtime-appendix.md) — executable item filter, contract checklist, verification evidence, reports, handoff payload, quick commands, and recovery rules

## Core invariants

- `solution-design.md` and approved `plan.md` are immutable inputs.
- Execute may not change solution decisions, task decomposition, dependency shape, docs impact, or verification strategy.
- `TASK`/`BUG` README content is the human execution contract.
- `.pulse/workgraph/items.jsonl` is mutated only through `node .github/skills/workflow/scripts/pulse.mjs workgraph ...`.
- Runtime reservations are short-lived execution leases; workgraph `owner` is durable responsibility.
- Fresh verification evidence is mandatory before close.
- Implementation gaps, deviations, unplanned decisions, and tradeoffs discovered during execution must be captured in the item's `implement-gap.md` and surfaced in completion reports.
- One item maps to one atomic commit unless the user explicitly chooses a non-commit execution mode outside Pulse's normal closeout.

## Step 1 — Initialize

Determine mode from invocation and runtime state:

- if invoked by `pulse:workflow swarm`, run in worker mode
- otherwise run in standalone single-worker mode

### 1a. Restore worker bootstrap context (worker mode only)

Capture and preserve these fields for the whole run:

- `runtime_identity`
- `coordinator_identity`
- `adapter_name`
- `active_epic_id`
- `active_story_id` or current-slice scope
- optional `startup_hint`

Treat `startup_hint` as a hint or rescue instruction, not a silent permanent assignment. Re-check live ready state and the item contract before claiming work.

If required startup fields are missing, request clarification on the active coordination surface before selecting work.

### 1b. Read project/runtime context

Read in this order:

1. `AGENTS.md`
2. `node .github/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`
3. `.pulse/runtime/state.json`
4. `.pulse/runtime/STATE.md`
5. active handoff file when resuming
6. active current-slice story artifacts only after the target item is known

Confirm:

- Gate 3 is approved
- recommended mode matches this invocation
- active epic/story/item pointers do not conflict with workgraph output
- no active handoff or reservation makes execution unsafe

If any required runtime file is absent, note the absence and use `status` output to decide whether to route to `use`. Do not invent context.

### 1c. Report `[ONLINE]` before claiming work (worker mode only)

Before selecting work, post `[ONLINE]` on the active coordination surface with:

- `runtime_identity`
- `AGENTS.md: read`
- `pulse:workflow execute: loaded`
- `Next step: node .github/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json`

Runtime mapping:

- Claude Code -> send startup acknowledgment to coordinator via `SendMessage`
- Codex -> reply on the parent coordination thread
- otherwise -> use the active coordination surface defined by adapter/runtime

Do not claim work before this startup report.

### 1d. Check owner-scoped handoff

Use owner-scoped handoffs:

- worker mode -> `.pulse/runtime/handoffs/worker-<runtime_identity>.json`
- standalone mode -> `.pulse/runtime/handoffs/single-worker.json`

If a handoff exists and belongs to the same owner identity:

1. read it and restore active item, reservations, verification state, blockers, and progress markers
2. verify runtime/workgraph state is still current
3. resume from recorded point without redoing already-complete steps
4. mark consumed or archive only after resume is confirmed

A worker must not consume another worker's handoff directly.

### 1e. Exceptional path: coordinator-reassigned orphaned handoff

Only the coordinator may reassign an orphaned worker handoff when the original owner identity is unavailable.

Before resuming a reassigned handoff, require coordinator confirmation that:

- prior worker inactivity was confirmed
- reservations for the prior owner were checked and safely transferred or released
- shared-branch commit queue state was checked and safely transferred
- `.pulse/runtime/handoffs/manifest.json` and the owner handoff file record previous owner, new owner, reason, and coordinator approval

If those updates are missing, do not resume the handoff.

## Step 2 — Select the next executable item

In worker mode, every loop starts with coordination visibility. Check the active coordination surface for:

- new coordinator instructions
- unresolved blocker replies
- conflict decisions
- handoff/recovery instructions
- commit-slot status affecting the next item

Then inspect live ready queue:

```bash
node .github/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json
```

Select the highest-priority ready item that is all of:

- kind `TASK` or `BUG`
- belongs to the active story or Gate 3-approved current slice
- `OPEN`
- dependency-unblocked
- not reserved by another active worker
- compatible with current mode and coordination constraints

Ignore `EPIC` and `STORY` rows for execution. If only non-executable ready rows exist, route to `plan` or `validate` rather than implementing from an epic/story summary.

### Exceptional path: direct orchestrator hint

If swarm suggests an item, treat it as a hint. Re-check `ready`, `workgraph show`, reservations, and the item README before claiming it.

## Step 3 — Read and verify the item contract

Read the selected item metadata:

```bash
node .github/skills/workflow/scripts/pulse.mjs workgraph show --repo-root <repo> <item-id> --json
```

Then read:

- item `content_path` README
- item `verification_path`
- parent story `plan.md`
- parent story `solution-design.md`
- parent story `discovery.md` when the item or plan cites discovery evidence
- cited `references/*.md` only when needed for this item
- cited learning refs only; do not load all learnings

Minimum contract fields are defined in [runtime-appendix.md](runtime-appendix.md#item-contract-checklist).

If required fields are missing, placeholder-only, contradictory, or imply a different solution, stop and reroute. Do not guess from prose.

If `testing_mode` is `tdd-required`, confirm red and green commands plus expected signals are present before implementation begins.

### Non-trivial item read rule

Do not rely on the item summary alone for sensitive work. Before reserving files or writing code, re-read approved current-slice artifacts when any are true:

- `testing_mode=tdd-required`
- explicit file scope crosses modules, ownership boundaries, public contracts, runtime/workgraph behavior, data, security, or provider integration
- item has multiple dependencies or explicit parallel coordination risk
- verification is integration-heavy or multi-step
- after the README read, more than one plausible implementation path remains

Use `solution-design.md`, `plan.md`, the item README, and workgraph metadata as the canonical current-work contract. If they disagree, route upstream.

## Step 4 — Reserve declared scope

In worker mode, reserve all declared file paths before editing:

```bash
node .github/skills/workflow/scripts/pulse.mjs reservation reserve \
  --repo-root <repo> \
  --agent <runtime_identity> \
  --item <item-id> \
  --path "src/foo.ts" \
  --path "src/bar.ts" \
  --json
```

Standalone mode has no cross-worker race by default, but explicit file scope remains a hard boundary. If an active swarm or shared reservation surface exists on the same branch, obey it.

If implementation needs files outside declared scope, stop and get scope expansion approved through the appropriate upstream command or coordinator. Do not silently edit outside scope.

### If reservation fails in worker mode

1. report `[FILE CONFLICT]` immediately
2. include item ID, requested paths, current holder when visible, and why the scope is needed
3. wait for coordinator resolution
4. keep monitoring the coordination surface while blocked

Do not proceed without safe reservations. Do not edit around conflicts.

## Step 5 — Mark item in progress

After contract and reservation checks pass, mark the item active:

```bash
node .github/skills/workflow/scripts/pulse.mjs workgraph update --repo-root <repo> <item-id> --status IN_PROGRESS --json
```

If the item is no longer eligible, re-read `ready` and reroute or select another item. Do not implement an item that another actor already changed out from under you.

## Step 6 — Implement

### Read before writing

Read every source, test, docs, runtime, or work-content file you will modify. Do not edit from memory.

### Honor locked decisions

Resolve every decision ID in the item README against `solution-design.md`. Implement exactly as approved. Do not reinterpret or improve locked decisions during execution.

### State assumptions before coding

Before changing files, be explicit about:

- existing path/component being extended
- what the item asks to preserve and change
- which current-slice done condition this item advances
- what verification result will prove success

If ambiguity remains across plausible interpretations, stop and route the contract back for clarification.

### Follow existing patterns

Match repository conventions for naming, imports, error handling, tests, runtime state handling, CLI patterns, docs style, and generated-artifact rules.

Do not introduce temporary architecture that violates approved boundaries.

### Maintain implementation gap notes

During implementation, keep an item-local gap log when execution reveals anything the approved specs did not cover.

Default path:

```text
<dirname(item content_path)>/implement-gap.md
```

Create or update this file whenever any of these occur:

- you must make a local implementation decision not explicitly covered by `solution-design.md`, `plan.md`, or the item README
- the approved spec/plan is ambiguous, incomplete, outdated, or contradicted by code reality
- you make a tradeoff that affects maintainability, UX, verification, performance, compatibility, or future work
- implementation cannot fully follow the approved contract and needs a deviation, workaround, scope expansion, or upstream change
- you discover something the user/reviewer should know before review or future planning

Recording a gap is not permission to silently change the approved solution. If the gap changes product behavior, architecture, task scope, file scope, verification strategy, or risk posture, stop and reroute or request approval before implementing that change.

Use [runtime-appendix.md](runtime-appendix.md#implementation-gap-log) for the required gap log shape.

### Keep changes surgical

Prefer the smallest change that satisfies the item.

Do not:

- add speculative abstractions or refactors
- broaden scope into unrelated cleanup
- add impossible-path handling not supported by the current boundary
- edit generated outputs directly when source files/templates own generation
- create unplanned workgraph items or dependency edges

### No pseudo-implementations

Every implementation artifact must be substantive and wired: imported, exported, referenced, tested, rendered, or documented as required. TODO-only, stub-only, or floating code does not satisfy execution.

### Selective TDD

Respect `testing_mode`:

- `standard` -> implement then verify exactly as contracted
- `tdd-required` -> run real red/green loop before finalizing

For `tdd-required`:

1. add/update the smallest failing test
2. run the red command and confirm the expected failure signal
3. implement the minimal production change
4. run the green command and confirm pass
5. record red/green evidence in the verification artifact

If production code was written before the red check, discard or rewrite that part within scope and restart the TDD loop.

## Step 7 — Verify and write evidence

Run item verification commands exactly as contracted. Do not substitute easier checks.

Before running, state what success looks like in one or two lines. If you cannot, the item remains under-specified and should be repaired instead of executed.

Verification completes only when:

- contracted commands pass in a fresh run
- evidence artifacts at the workgraph `verification_path` or declared equivalent are updated for this run
- outcomes are traceable to the current execution pass

Use the verification evidence contract in [runtime-appendix.md](runtime-appendix.md#verification-evidence-contract).

If verification fails:

1. debug the root cause
2. compare failure against the pre-stated success criteria
3. retry up to 2 times when the repair remains within approved scope
4. if still blocked:
   - worker mode -> report `[BLOCKED]` and wait for coordinator resolution
   - standalone mode -> surface the blocker or route to an explicit debug/fix path

Do not redefine success after failure. Do not close without passing verification and fresh evidence.

## Step 8 — Close-readiness check

Before closing the item, confirm all true:

- edits stayed within declared file scope or approved expansion
- locked decisions still match final implementation
- all contracted verification steps passed in a fresh run
- every evidence entry is present and substantive
- `tdd-required` red/green evidence is recorded when applicable
- `implement-gap.md` is present and surfaced when execution created gaps, deviations, tradeoffs, or unplanned decisions
- no unresolved blocker, review finding, failed command, required approval, or follow-up is being hidden
- unrelated working-tree changes are not staged into this item commit

## Step 9 — Close the workgraph item

Close the completed item only after verification evidence is complete:

```bash
node .github/skills/workflow/scripts/pulse.mjs workgraph close --repo-root <repo> <item-id> --json
```

If close fails because children remain open, evidence is missing, or workgraph validation fails, stop and repair/reroute rather than bypassing the workgraph.

## Step 10 — Atomic commit

One item should produce one item-scoped commit. Do not batch multiple items or unrelated changes.

### Worker mode commit queue

Implementation and verification can run in parallel across workers, but `git add`/`git commit` are serialized under coordinator control on a shared branch.

Protocol:

1. send `READY_TO_COMMIT` with item ID and exact files to stage
2. wait for `COMMIT_SLOT_GRANTED`
3. after slot grant only:

```bash
git add <files-you-modified> <verification-path> <implement-gap.md-if-created>
git commit -m "feat(<item-id>): <summary>"
```

4. commit only declared files
5. report `COMMIT_DONE` with hash, or `COMMIT_BLOCKED` with reason

### Standalone mode

Commit directly with the same one-item discipline only when no active swarm commit queue exists for the current branch/slice. Otherwise route through the coordinator-owned queue.

## Step 11 — Release reservations and report

### Worker mode

Release reservations before `[DONE]` so other workers can acquire paths:

```bash
node .github/skills/workflow/scripts/pulse.mjs reservation release --repo-root <repo> --agent <runtime_identity> --json
```

Then post `[DONE]` with:

- item ID
- runtime identity
- commit hash
- files changed
- verification result
- evidence path(s)
- implementation gap path and summary when `implement-gap.md` exists, otherwise `None.`
- follow-up item/blocker if needed, otherwise `None.`

After reporting, inspect the coordination surface once before selecting another item.

### Standalone mode

Record equivalent completion details in `.pulse/runtime/STATE.md`, including the implementation gap path and summary when `implement-gap.md` exists, and keep `.pulse/runtime/state.json` aligned when runtime posture changes.

If current-slice execution is complete, recommend `pulse:workflow review` with manual invocation by default.

## Step 12 — Loop, complete, or pause

After each item:

1. refresh ready work:
   ```bash
   node .github/skills/workflow/scripts/pulse.mjs ready --repo-root <repo> --json
   ```
2. filter to Gate 3-approved current-slice `TASK`/`BUG` items
3. if more executable items remain and context is safe, loop to Step 2
4. if no current-slice executable items remain, confirm no open blockers/conflicts/evidence gaps/unsurfaced implementation gaps, then recommend `pulse:workflow review`
5. if context is critical, write an owner-scoped handoff and stop cleanly

Empty ready queue is a signal, not proof of completion. Completion requires current-slice artifacts, workgraph metadata, verification evidence, and runtime state to agree.

## Step 13 — Handoff

Use the shared owner-scoped handoff envelope from [`../use/handoff-contract.md`](../use/handoff-contract.md).

Required execute payload details are listed in [runtime-appendix.md](runtime-appendix.md#handoff-payload-for-execute).

Register the handoff in `.pulse/runtime/handoffs/manifest.json` with matching owner, active command, item, phase, summary, next action, and path.

Worker mode: notify coordinator after writing the handoff.

## Step 14 — Post-compaction recovery

If context compaction/summarization is detected, stop immediately. Do not continue implementing from memory.

Re-read in this order:

1. `AGENTS.md`
2. `node .github/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`
3. `.pulse/runtime/state.json`
4. current item metadata via `node .github/skills/workflow/scripts/pulse.mjs workgraph show --repo-root <repo> <item-id> --json`
5. current item README and verification path
6. parent story `plan.md` and `solution-design.md`
7. `discovery.md` or references only when required by the item contract
8. active reservations:
   ```bash
   node .github/skills/workflow/scripts/pulse.mjs reservation list --repo-root <repo> --active-only --json
   ```
9. latest coordinator updates on active coordination surface when in worker mode

Resume only after all applicable reads are complete and state still matches the approved current slice.

## Gate posture

Execute consumes Gate 3 approval. It does not approve Gate 3, Gate 4, merge, release, future slices, unplanned fixes, or scope expansions.

After execution completes for the current slice, the normal next command is `pulse:workflow review`.

## Red flags

Stop and reassess if you catch yourself:

- executing without explicit current Gate 3 approval
- implementing an `EPIC` or `STORY` summary instead of a `TASK`/`BUG`
- executing without reading the full item README contract
- executing with missing canonical contract fields or placeholders
- editing outside declared/reserved scope
- changing solution/design/plan decisions during implementation
- skipping exact verification criteria
- closing with stale, partial, or non-substantive evidence
- claiming `tdd-required` without a real red failure and green pass
- continuing after compaction without recovery reads
- implementing stubs/TODOs as completion
- guessing through ambiguity instead of routing repair
- bundling multiple items into one commit
- committing in worker mode without `COMMIT_SLOT_GRANTED`
- claiming work without reservation awareness
- blocking/completing without coordination report
- waiting silently while blocked/conflicted/handoff-ready
