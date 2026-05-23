# `pulse:workflow intake`

New-work admission manual for turning raw user input into a classified Pulse work stream before brainstorming, exploration, planning, or execution begins.

This command is the only place where a new user request enters an empty Pulse workflow session.

## Mission

Classify the new request, determine which Pulse artifacts should be proposed, resolve whether the request belongs to existing work or is already satisfied, ask for confirmation before structural mutation, and route the operator to the next safe workflow command.

Intake answers:

- what type of work is this?
- what existing or new work boundary should contain it?
- does existing work already cover, satisfy, or block this request?
- which artifacts should be proposed now and created only after confirmation?
- how risky is it?
- what command should run next?

Intake does not design the solution, lock implementation decisions, create execution tasks, validate readiness, or implement code.
Intake also must not create or mutate a workgraph boundary until the operator has presented the proposed boundary and received explicit user confirmation.

## Intake flow

Run intake in this order:

```text
User input
    |
    v
Confirm empty session from pulse:workflow use
    |
    v
Classify input type
    |
    v
Restate as work item
    |
    v
Resolve work boundary against existing workgraph and works artifacts
    |
    v
Check duplicate/satisfaction evidence
    |
    v
Run risk checklist
    |
    v
Choose lane: tiny, normal, or high_risk
    |
    v
Choose work boundary: none, single_story, initiative, or blocked
    |
    v
Define artifact obligations
    |
    v
If no structural mutation is needed, present routing result and stop without writing files unless the confirmed mutation package includes a runtime-only record
    |
    v
If structural mutation is needed, present proposed boundary creation/update and artifact paths
    |
    v
<HARD_GATE>Ask for explicit user confirmation before any workgraph or works/ mutation</HARD_GATE>
    |
    v
Create/update only the confirmed boundary and intake artifact, or stop without mutation
    |
    v
Recommend next command
```

The flow is intentionally admission-focused. Stop as soon as one step cannot be completed safely, present the blocker, and do not proceed to downstream workflow commands.
The first pass is classification and proposal only. Before confirmation, keep classification, correlation, blockers, and proposed boundary details in the operator response only; do not write `.pulse/runtime/`, `.pulse/workgraph/`, or `works/` files. Any file mutation requires a single explicit confirmation of the proposed mutation package. If the package creates or updates a boundary, that same confirmation covers the matching runtime posture update; do not ask for a second runtime-record approval. Structural workgraph creation, workgraph metadata updates, and `works/` writes happen only after the user confirms the proposed boundary.

## Entry criteria

Run `pulse:workflow intake <user input>` only when all are true:

- the user provided new input to classify
- `pulse:workflow use` has established readiness and loaded the session for this operator session
- the empty-session proof is fresh according to [Freshness and posture proof](#freshness-and-posture-proof)
- the latest posture proof reports no active, resumable, conflicted, gated, or executable work
- `.pulse/runtime/state.json` has no active epic, story, item, command, or pending gate
- `.pulse/runtime/handoffs/manifest.json` has no active handoff to resume
- `.pulse/workgraph` has no ready execution work for an existing stream

If any active, resumable, conflicted, gated, or executable session work exists, block immediately.
Dormant open workgraph items may still be used for correlation and routing when the latest posture proof reports no active session to resume.

### Freshness and posture proof

A `use` result is fresh only while intake can prove it still represents the current runtime posture. Treat the proof as stale unless at least one of these is true:

- intake is running in the same uninterrupted operator turn immediately after `pulse:workflow use`, with no intervening file mutation, workgraph command, reservation command, handoff update, or downstream workflow command
- the session load artifact records a `generated_at` timestamp, it is within the implementation-defined freshness window, and `.pulse/runtime/state.json`, `.pulse/runtime/handoffs/manifest.json`, and `.pulse/workgraph` have not changed since that timestamp
- intake has just run `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` and the returned posture confirms the session is empty for intake purposes

The optimal default is to run `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` immediately before the first intake hard gate and use that output as the authoritative posture proof. Do not rely on memory of an earlier `use` result when the runtime files may have changed.

The posture proof must explicitly cover:

- active epic, story, item, command, or pending gate in `.pulse/runtime/state.json`
- active/resumable handoff in `.pulse/runtime/handoffs/manifest.json`
- conflicted, reserved, executable, or ready work reported by runtime/workgraph status
- ready execution work in `.pulse/workgraph` for an existing stream

If freshness cannot be proven, block instead of guessing.

Block message shape:

```text
Intake blocked: current Pulse session is not empty.
Run pulse:workflow use, then resume, review, close, compound, or explicitly abandon the existing work before starting new intake.
```

Intake must not be used as a side channel to start unrelated work while an active epic, story, task, bug, handoff, or gate is still open. Open but inactive work may be matched as existing context; `intake` must route to that stream instead of creating duplicate work.

## Input type classification

Choose exactly one primary input type.

| Input type | Use when | Flow effect |
| --- | --- | --- |
| `new_spec` | the user provides a completely new product or project spec | propose a spec intake boundary and identify required downstream product contract, candidate epic, validation, and decision artifacts |
| `spec_slice` | the user selects work from an existing spec or product contract | find related contract material and propose the story boundary or update needed for that slice |
| `change_request` | the user changes or extends accepted behavior | identify affected contract material, propose the story update or new boundary, and identify downstream verification expectations |
| `new_initiative` | the user introduces a large new area spanning multiple stories | propose an initiative or epic intake boundary, identify candidate downstream stories, and select a likely first story shape without creating every story upfront |
| `maintenance_request` | the user asks for dependency, performance, architecture, operational, refactor, or similar technical work | propose a technical story boundary when needed and identify downstream validation report or decision obligations; usually touch fewer product contract artifacts unless behavior changes |
| `harness_improvement` | the user asks to improve Pulse workflow, process, templates, or harness behavior | propose a harness story or initiative boundary and identify downstream router/reference/template/runtime contract updates or harness backlog proposal obligations |

A read-only review of workflow or harness material does not by itself require intake mutation. If the review produces implementation or tracking work, classify that resulting work as `harness_improvement` and follow the normal boundary proposal and confirmation rules.

Input type determines artifact obligations and flow constraints. It is not just a label.

## Lane classification

After input type, choose a lane:

- `tiny`
- `normal`
- `high_risk`

Risk flags are canonical uppercase identifiers everywhere: intake classification, `INTAKE.md`, `state.intake.risk_flags`, workgraph metadata, and tests. Lowercase risk flag names are not valid documented or persisted flag values.

Risk flags:

- `AUTH`
- `DATA`
- `MIGRATION`
- `SECURITY`
- `EXTERNAL_API`
- `PERFORMANCE`
- `UX`
- `CI`
- `PUBLIC_CONTRACTS`
- `CROSS_PLATFORM`
- `EXISTING_BEHAVIOR`
- `WEAK_PROOF`
- `MULTI_DOMAIN`
- `UNKNOWN`

Use the most specific canonical flag set available:

| Risk area | Canonical flag |
| --- | --- |
| Authentication or authorization behavior | `AUTH` |
| Data model, persistence shape, or data handling | `DATA` |
| Persistence migration, recovery, or data-loss-sensitive transition | `MIGRATION` |
| Audit, abuse, privacy, or security-sensitive behavior | `SECURITY` |
| External provider, integration, or API behavior | `EXTERNAL_API` |
| Performance-sensitive behavior | `PERFORMANCE` |
| User experience or interaction behavior | `UX` |
| CI, release, or verification infrastructure | `CI` |
| Public contract, API, CLI, or documented behavior compatibility | `PUBLIC_CONTRACTS` |
| Cross-platform behavior or portability | `CROSS_PLATFORM` |
| Existing accepted behavior may change | `EXISTING_BEHAVIOR` |
| Proof is weak, validation is reduced, or evidence is unclear | `WEAK_PROOF` |
| Multiple product or technical domains are affected | `MULTI_DOMAIN` |
| Unresolved or unclear risk | `UNKNOWN` |

Classification rules:

```text
0-1 risk flags:
  tiny or normal, based on implementation impact

2-3 risk flags:
  normal with stronger validation

4+ risk flags:
  high_risk

Any risk hard gate:
  high_risk unless the user explicitly narrows scope
```

Risk hard gates:

- `AUTH`
- `DATA` when data loss is possible
- `MIGRATION`
- `SECURITY`
- `EXTERNAL_API`
- `WEAK_PROOF` when validation requirements are weakened
- `PUBLIC_CONTRACTS` when public behavior can break

Lane controls workflow strictness. Input type controls artifact obligations.

## Affected surfaces

Classify likely affected surfaces without doing deep implementation analysis:

- `product_contract`
- `code`
- `tests`
- `router`
- `runtime`
- `workgraph`
- `works`
- `docs`
- `harness`
- `provider_metadata`

Use these to explain why the next command is safe.

## Work boundary resolution

Before creating any new epic or story boundary, intake must try to correlate the request to existing durable work.
This is admission control, not optional enrichment.
The goal is to prevent duplicate streams, reopen the correct open work when appropriate, and avoid creating new execution work for behavior that is already implemented.

Correlate against:

- current `works/` artifact structure
- current `.pulse/workgraph/items.jsonl` metadata
- any linked product-contract or verification artifacts already referenced by the candidate epic/story
- direct evidence from the user request when they name a known story, epic, bug, or spec slice

Use the strongest available evidence in this order:

1. explicit item IDs or paths named by the user
2. existing open story or bug whose stated contract matches the request
3. existing closed story with closely related behavior and verification history
4. existing verification evidence that the requested outcome is already satisfied
5. narrow code evidence only when it is named by the user, already surfaced by `use`, or checkable with a targeted read
6. weak semantic similarity only

If proving satisfaction requires broad codebase investigation, do not turn intake into `explore`; classify as `ambiguous_or_blocked` or route to `explore` with the suspected match presented in the response. Persist the suspected match only after the user explicitly approves recording the routing result.
If correlation is weak, do not guess. Ask the minimum routing question needed before creating boundaries.

### Correlation outcomes

Intake must classify the request into exactly one of these outcomes before artifact creation:

| Outcome | Meaning | Default action |
| --- | --- | --- |
| `new_work` | no existing durable boundary adequately contains the request | propose a new epic/story boundary and ask for confirmation before creating it |
| `existing_open_work` | an open epic/story already owns the requested behavior | propose reuse/update of that boundary and ask for confirmation before writing new intake/context artifacts |
| `existing_closed_related_work` | a closed story is closely related, but the user is asking for additional behavior or a behavior delta | propose a new story linked to the closed story; do not reopen the closed story by default |
| `already_satisfied` | the request appears implemented already or fully covered by existing accepted behavior | do not create new execution work; present evidence and ask only if confirmation is needed |
| `ambiguous_or_blocked` | evidence conflicts, is too weak, or required context is missing | stop intake and ask the routing question |

### Duplicate and satisfaction check

After finding a likely boundary, explicitly check whether the request is already satisfied or is merely a duplicate restatement of open work.

Ask:

- does an open item already describe the requested behavior closely enough that new boundaries would be duplication?
- does a closed item plus current evidence show the behavior is already implemented and verified?
- is the user actually asking for a new behavior delta over prior work rather than a bugfix or follow-up within that same story?

If the answer is `already_satisfied`, intake must not create new execution work.
Present the evidence, explain why the request looks satisfied, and either:

- stop with an `already_satisfied` routing result, or
- ask one routing question if the evidence is suggestive but not decisive

Weak proof is not enough to close the loop silently.
When proof is weak, prefer `ambiguous_or_blocked` and ask the user to confirm whether the desired behavior already exists.

### Behavior delta over closed work

A small follow-up change to a closed story is still new work when it changes accepted behavior.
Do not reopen the closed story or add a task under the closed story by default.
Instead:

- propose a new small story for the delta
- propose a traceability link to the prior closed story
- carry forward any relevant context or verification evidence in the intake artifact after confirmation
- note why the old story is related but insufficient

This preserves closure semantics while keeping the new request visible as a separate behavior change.

## Work boundary

Choose one boundary:

- `none`
- `single_story`
- `initiative`
- `blocked`

Rules:

- `new_initiative` normally proposes an `initiative` or epic boundary plus a selected first story.
- `new_spec` may propose an initiative boundary or a first story when the first slice is obvious.
- `spec_slice` normally produces a `single_story` boundary.
- `change_request` should use `single_story` when workflow-managed durability is needed, even when the change is tiny.
- `maintenance_request` should use `single_story` when workflow-managed durability is needed, or `blocked` when it does not belong in the workflow session.
- Pulse intake does not model standalone direct tasks. If the user intentionally wants an untracked tiny direct edit outside Pulse workflow, do not run intake for it; the operator may handle it outside `pulse:workflow` discipline.
- If an existing open story owns the request, use `single_story` and update that owning story boundary after confirmation instead of creating a task-shaped intake path.
- `harness_improvement` should be `single_story` or `initiative` when it changes public workflow behavior.
- `already_satisfied` uses boundary `none`; it must not create a new boundary or new execution work.
- `existing_closed_related_work` that changes behavior should propose a new `single_story` boundary linked to the closed story, not reopen it by default.
- `ambiguous_or_blocked` means `blocked` until the routing question is answered.

Do not create an execution-ready `TASK` or `BUG` during intake. Intake-managed work must resolve to an epic/story boundary before downstream planning or execution.

## Artifact output

Intake first produces a proposed intake package and asks the user to confirm the structural boundary before writing durable files.
A confirmed intake produces the durable intake package.

Before confirmation, show the expected artifact shape without creating files:

```text
Story boundary:
  works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/INTAKE.md

Initiative or epic boundary:
  works/epics/<epic-id>-<epic-slug>/INTAKE.md
```

After confirmation, use `dirname(item.content_path)` from the `node .trae/skills/workflow/scripts/pulse.mjs workgraph create --json` result as the source of truth for the concrete boundary directory, then write `INTAKE.md` in that directory. For example, a returned `item.content_path` of `works/epics/E-1-example/S-1-slice/README.md` means the confirmed intake artifact is `works/epics/E-1-example/S-1-slice/INTAKE.md`.
Temporary runtime notes may be used while classifying, but confirmed intake that creates or updates a work boundary must leave the durable result under `works/`.
For `already_satisfied`, intake may exit without writing a new `INTAKE.md` when doing so would mutate closed historical work or create a misleading new work stream. By default, present the satisfaction result, matched item IDs, and evidence summary in the response only. If the proposed mutation package is runtime-only and the user confirms it, `state.intake` and `.pulse/runtime/STATE.md` must record the satisfaction result, matched item IDs, and evidence summary.

For `existing_open_work`, the confirmed intake note belongs to the existing owning boundary. If that boundary does not have `INTAKE.md`, create it after confirmation. If `INTAKE.md` already exists, append a dated `Additional Intake` section by default and include the new user input, restated work, correlation evidence, duplicate/satisfaction check, and routing decision. Do not overwrite previous intake material unless the user explicitly confirms replacement.

Minimum `INTAKE.md` structure:

```markdown
# Intake

## User Input

## Restated Work

## Input Type

## Lane

## Risk Flags

## Affected Surfaces

## Work Boundary

## Existing Work Correlation

## Duplicate / Satisfaction Evidence

## Artifact Obligations

## Recommended Next Command

## Required Next Artifact

## Routing Decision

## Open Routing Questions

## Harness Delta Candidate
```

`Existing Work Correlation` should name the matched epic/story when one exists, the correlation outcome (`new_work`, `existing_open_work`, `existing_closed_related_work`, `already_satisfied`, or `ambiguous_or_blocked`), and the evidence strength.

`Duplicate / Satisfaction Evidence` should summarize the specific proof used to decide whether the request is already covered, partially covered, or clearly new. Include why the matched work is insufficient when creating a new delta story over a closed story.

Artifact obligations should state what the confirmed intake may create or update now and what downstream commands must produce later. Examples:

- `new_spec`: confirmed spec intake boundary now; downstream product contract material, candidate epic analysis, validation shape, and decisions
- `spec_slice`: confirmed story slice boundary or update now; downstream related product contract links
- `change_request`: confirmed story update or new boundary now; downstream current behavior, target behavior, and verification matrix updates
- `new_initiative`: confirmed initiative or epic intake boundary now; downstream candidate story list and selected first story refinement
- `maintenance_request`: confirmed technical story boundary when needed; downstream validation report or decision
- `harness_improvement`: confirmed harness story or initiative boundary when needed; downstream router/reference/template/runtime contract updates or harness backlog proposal

## Workgraph output

Intake may create only structural metadata after explicit user confirmation:

- one `EPIC` when a new epic or initiative boundary is needed
- one `STORY` when a story-sized boundary is known
- `linked_items` references for non-blocking traceability when the new story is related to prior closed work or parallel context

Do not invent unsupported workgraph metadata fields for `INTAKE.md`; the durable intake narrative lives in the adjacent `INTAKE.md` artifact.

Before confirmation, intake must present the exact proposed workgraph operation(s), expected boundary path, and `INTAKE.md` path, then stop at a `<HARD_GATE>`. Immediately before presenting this hard gate, run or verify a fresh `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` posture proof. If the proof is stale, missing, or contradicted by `.pulse/runtime`, `.pulse/workgraph`, or `works/` changes during classification, do not present the creation/update hard gate; route back to `pulse:workflow use` or ask the operator to rerun status first.

```xml
<HARD_GATE>
Confirm the proposed intake boundary before any workgraph or works/ mutation.

Proposed workgraph operation(s):
- ...

Expected boundary path:
- ...

Expected intake artifact:
- .../INTAKE.md

Reply with explicit approval to create/update this boundary, or provide corrections. Do not continue on silence or ambiguous acknowledgement.
</HARD_GATE>
```

Do not run `node .trae/skills/workflow/scripts/pulse.mjs workgraph create`, update existing metadata, or write the boundary `INTAKE.md` until the user confirms the `<HARD_GATE>`.

After the user confirms the `<HARD_GATE>`, run `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` again before any mutation. Re-check that no active, resumable, conflicted, gated, or executable work appeared since the hard gate and that the confirmed proposal still matches the current runtime/workgraph posture. If posture changed, stop without mutation and route back to `pulse:workflow use`. If the session is still safe, create only the confirmed structural boundary using the existing workgraph CLI.

For a new epic or initiative boundary:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph create --repo-root <repo> --kind EPIC --title "<confirmed epic title>" --json
```

For a story boundary under a confirmed or existing epic:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph create --repo-root <repo> --kind STORY --parent <epic-id> --title "<confirmed story title>" --json
```

Add optional `--owner`, `--priority`, `--label`, and `--risk` flags only when they were part of the confirmed proposal.
If the confirmed boundary requires both a new epic and a new story, create the epic first, read the returned `item.id`, then use that ID as the story `--parent`.
If a traceability link to related closed or parallel work was confirmed, add it after both items exist:

```bash
node .trae/skills/workflow/scripts/pulse.mjs workgraph link add --repo-root <repo> <new-story-or-epic-id> <related-item-id> --json
```

After `workgraph create` succeeds, write the intake artifact to the confirmed path defined in [Artifact output](#artifact-output).
Do not hand-author IDs or guess final paths before `workgraph create` returns.

If correlation outcome is `existing_open_work`, propose the existing boundary update instead of creating a duplicate story, then ask for confirmation before mutating it.
If correlation outcome is `already_satisfied`, do not create new EPIC/STORY/TASK/BUG metadata unless the user clarifies that the behavior is actually different.

Intake must not create execution work:

- no executable `TASK` or `BUG`
- no reservations
- no ready queue entries
- no Gate 1, Gate 2, Gate 3, or Gate 4 approvals

## Runtime output

Do not write intake posture to `.pulse/runtime/state.json` or `.pulse/runtime/STATE.md` before user confirmation. Before confirmation, intake posture belongs in the operator response only.
Use one confirmation for the whole proposed mutation package:

- Boundary package: creates or updates workgraph/`works/` artifacts and records the matching runtime posture in the same confirmed operation.
- Runtime-only package: records a no-structural-mutation result in runtime mirrors only, when preserving that result is useful.
- No-write package: presents the routing result and stops without any file mutation.

Do not ask for a separate runtime-record approval after the user has confirmed a boundary package.
Runtime state must not claim active created epic/story IDs until creation succeeds. After confirmation and creation, runtime state may record the created active epic/story IDs.

For no-structural-mutation outcomes, use this rule:

| Outcome | Default no-write package | Confirmed runtime-only package |
| --- | --- | --- |
| `already_satisfied` | Present evidence and stop without writing files | Update `state.intake` and `.pulse/runtime/STATE.md` with `status: already_satisfied`, matched IDs, evidence summary, and recommended next action |
| `ambiguous_or_blocked` | Ask the routing question and stop without writing files | Update `state.intake` and `.pulse/runtime/STATE.md` with `status: needs_user_routing` or `blocked`, evidence summary, open question, and recommended next action |
| routing-only recommendation | Present the recommended next command and stop without writing files | Update `state.intake` and `.pulse/runtime/STATE.md` with the classification, correlation outcome, lane, and recommended next command |

When runtime posture is recorded, update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` together in the same operation. Do not update only one runtime mirror.
Do not write `works/` artifacts or workgraph metadata for no-structural-mutation outcomes unless the user later confirms a boundary creation/update hard gate.

State should express general routing posture with `active_command: intake` and `next_action` set for manual invocation by default. Intake-specific fields must be recorded under the official `state.intake` namespace:

```json
{
  "intake": {
    "status": "classified | awaiting_creation_confirmation | confirmed_created | needs_user_routing | blocked | already_satisfied",
    "input_type": "new_spec | spec_slice | change_request | new_initiative | maintenance_request | harness_improvement",
    "correlation_outcome": "new_work | existing_open_work | existing_closed_related_work | already_satisfied | ambiguous_or_blocked",
    "matched_item_ids": [],
    "linked_item_ids": [],
    "satisfaction_evidence_summary": "",
    "lane": "tiny | normal | high_risk",
    "risk_flags": ["EXISTING_BEHAVIOR", "CI"],
    "artifact_path": null,
    "proposed_boundary": null,
    "recommended_next_command": "pulse:workflow explore"
  }
}
```

Use `state.intake.proposed_boundary` only after a confirmed mutation package permits writing runtime posture for a pending proposal; before that, proposed boundary details live in the response only. Record active epic/story IDs only after confirmed creation succeeds. Use `state.intake.artifact_path` only after a durable intake artifact is written. For `already_satisfied`, record the satisfaction result, matched item IDs, and evidence summary in both `state.intake` and `.pulse/runtime/STATE.md` only after a confirmed runtime-only package permits recording the result; do not mutate closed historical work just to record satisfaction evidence.

Gate state remains `none` or `pre_gate`. Gate 1 begins only after `pulse:workflow explore` produces an approvable context artifact.

## Routing decision

Recommend exactly one next command or one immediate `<HARD_GATE>` action.

| Situation | Next action |
| --- | --- |
| repository/session readiness is unclear | `pulse:workflow use` |
| structural workgraph or `works/` mutation is proposed | present the `<HARD_GATE>` and stop until explicit approval |
| new spec or initiative lacks a selected story shape after confirmed boundary handling | `pulse:workflow brainstorm` |
| story intent exists but behavior/context decisions need locking after confirmed boundary handling | `pulse:workflow explore` |
| tiny or clear low-risk work needs Pulse-managed durability | `pulse:workflow plan` under the owning or newly confirmed story boundary |
| request maps to existing open work | propose reuse/update of the existing stream, hard-gate any artifact mutation, then recommend the next unmet command |
| request appears already satisfied with strong evidence | stop and present the evidence; no new execution command |
| intake cannot safely classify or route | block and ask the routing question |

Default continuation is manual. Do not invoke the next command unless the user explicitly asks to continue now.
When new or updated structural work is needed, the next immediate action is the `<HARD_GATE>` confirmation, not `brainstorm`, `explore`, or `plan`.
Do not treat classification alone as permission to mutate workgraph or `works/`.

## Routing questions

Ask only questions that block classification or routing.

Good intake questions:

- is this one story or a larger initiative?
- should the first slice be X or Y?
- is the requested change meant to alter existing behavior or only improve internals?
- does the existing story already cover the outcome you want, or is this a new behavior delta?
- is the current implementation already acceptable, or do you want additional behavior beyond what exists now?

Do not conduct design discovery, implementation planning, or validation planning inside intake.

## Gate posture

Intake is a pre-gate admission checkpoint, not Gate 1.

`brainstorm` may follow intake when the work is real but the story shape, design direction, or first slice is still unclear. Brainstorm can ask for directional/spec approval, but it still does not replace Gate 1.

- Brainstorm/spec approval remains before `explore` when needed
- Gate 1 remains after `explore`
- Gate 2 remains after `plan`
- Gate 3 remains after `validate`
- Gate 4 remains after `review`

For non-trivial intake results, stop after presenting the routing decision or `<HARD_GATE>`. Do not auto-invoke the next command unless the user explicitly asks to continue after required confirmation is complete.

Stop after intake when:

- lane is `normal` or `high_risk`
- boundary is `initiative`
- any routing question was needed
- structural workgraph or `works/` mutation is proposed and awaiting `<HARD_GATE>` approval
- the recommended next command is not obvious from existing artifacts

## Exit contract

Successful exit requires:

- fresh posture proof, preferably `node .trae/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json`, confirmed the session was empty before intake hard gate
- input type selected
- existing work correlation outcome selected
- duplicate/satisfaction evidence presented in the response, and persisted only when included in a confirmed mutation package
- lane selected with risk flags
- work boundary selected or blocker stated
- proposed workgraph operation(s), boundary path, and `INTAKE.md` path presented before mutation
- when creation or update is proposed, explicit user confirmation received through the `<HARD_GATE>` before creating/updating EPIC/STORY metadata or writing boundary `INTAKE.md`
- durable `INTAKE.md` written under `works/` only after confirmation when a work boundary is created or updated
- for `already_satisfied`, either no new artifact is written or any evidence note must avoid mutating closed historical work misleadingly
- structural EPIC/STORY metadata created only when needed and only after confirmation
- runtime state updated with intake posture and no gate approval only when included in a confirmed mutation package
- next command recommendation stated

## Red flags

Stop if you catch yourself:

- running intake without a fresh posture proof, or while active or resumable work exists
- creating or updating EPIC/STORY metadata before explicit user confirmation
- writing boundary `INTAKE.md` before explicit user confirmation
- creating executable tasks or bugs
- approving gates
- planning implementation details
- doing deep codebase analysis
- creating every story for a large initiative upfront
- treating input type as decorative metadata
- opening a new story without checking for existing open, closed, or already-satisfied work
- reopening a closed story by default when the request is actually a behavior delta
- routing high-risk work directly to execution
