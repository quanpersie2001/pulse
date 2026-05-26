# `pulse:workflow intake`

New-work admission manual for turning raw user input into a classified Pulse work stream before brainstorming, exploration, planning, or execution begins.

This command is the only place where a new user request enters an empty Pulse workflow session.

## Purpose

Intake answers:

- what type of work is this?
- what existing or new work boundary should contain it?
- does existing work already cover, satisfy, or block this request?
- which artifacts should be proposed now and created only after confirmation?
- how risky is it?
- what command should run next?

Intake admits work into the workflow. It does not design the solution, lock implementation decisions, create execution tasks, validate readiness, implement code, or approve gates.

## Core invariants

- Intake runs only after `pulse:workflow use` has established a fresh empty-session posture.
- Intake is read-only until the operator presents a mutation package and receives explicit user confirmation.
- Before confirmation, classification, correlation, blockers, runtime posture, and proposed boundary details live only in the operator response.
- After confirmation, intake may mutate only the confirmed package: boundary package, runtime-only package, or no-write package.
- Intake must not create executable `TASK` or `BUG` work, reservations, ready queue entries, or workflow gate approvals.
- Intake must check existing durable work before proposing new work.
- Weak proof never silently creates, reopens, or closes work; ask the minimum routing question instead.
- Default continuation is manual. Do not invoke the recommended next command unless the user explicitly asks to continue.

## Flow

Run intake in three phases.

```text
Phase 1 — Eligibility
  - prove the current Pulse session is empty
  - block if active, resumable, conflicted, gated, reserved, ready, or executable work exists

Phase 2 — Classification
  - classify input type
  - restate the work item
  - correlate against works/ and .pulse/workgraph
  - check duplicate or satisfaction evidence
  - classify risk flags, lane, affected surfaces, and work boundary
  - define artifact obligations

Phase 3 — Admission result
  - present a no-write terminal result, routing question, or mutation proposal
  - if structural mutation is proposed, present the hard gate and stop
  - after explicit confirmation, re-check posture, perform only confirmed mutations, then recommend the next command
```

Stop as soon as one phase cannot be completed safely. Do not continue to downstream workflow commands from intake unless the user explicitly asks after required confirmation is complete.

## Entry criteria

Run `pulse:workflow intake <user input>` only when all are true:

- the user provided new input to classify
- `pulse:workflow use` has established readiness and loaded this operator session
- the empty-session proof is fresh according to [Freshness and posture proof](#freshness-and-posture-proof)
- the latest posture proof reports no active, resumable, conflicted, gated, reserved, executable, or ready work
- `.pulse/runtime/state.json` has no active epic, story, item, command, or pending gate
- `.pulse/runtime/handoffs/manifest.json` has no active handoff to resume
- `.pulse/workgraph` has no ready execution work for an existing stream

If any active, resumable, conflicted, gated, reserved, executable, or ready session work exists, block immediately.
Dormant open workgraph items may still be used for correlation and routing when the latest posture proof reports no active session to resume.

Block message shape:

```text
Intake blocked: current Pulse session is not empty.
Run pulse:workflow use, then resume, review, close, compound, or explicitly abandon the existing work before starting new intake.
```

### Freshness and posture proof

The optimal default is to run:

```bash
node .gemini/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json
```

Use that output as the authoritative posture proof immediately before the first intake posture gate (proof-only eligibility check).

A `use` result is fresh only while intake can prove it still represents the current runtime posture. Treat proof as stale unless at least one is true:

- intake is running in the same uninterrupted operator turn immediately after `pulse:workflow use`, with no intervening file mutation, workgraph command, reservation command, handoff update, or downstream workflow command
- the session load artifact records a `generated_at` timestamp, it is inside the implementation-defined freshness window, and `.pulse/runtime/state.json`, `.pulse/runtime/handoffs/manifest.json`, and `.pulse/workgraph` have not changed since that timestamp
- intake has just run `node .gemini/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` and the returned posture confirms the session is empty for intake purposes

The proof must explicitly cover:

- active epic, story, item, command, or pending gate in `.pulse/runtime/state.json`
- active or resumable handoff in `.pulse/runtime/handoffs/manifest.json`
- conflicted, reserved, executable, or ready work reported by runtime/workgraph status
- ready execution work in `.pulse/workgraph` for an existing stream

If freshness cannot be proven, block instead of guessing.

## Input type classification

Choose exactly one primary input type. The input type controls artifact obligations and flow constraints; it is not decorative metadata.

| Input type | Use when | Flow effect |
| --- | --- | --- |
| `new_spec` | the user provides a completely new product or project spec | propose a spec intake boundary and identify required downstream product contract, candidate epic, validation, and decision artifacts |
| `spec_slice` | the user selects work from an existing spec or product contract | find related contract material and propose the story boundary or update needed for that slice |
| `change_request` | the user changes or extends accepted behavior | identify affected contract material, propose the story update or new boundary, and identify downstream verification expectations |
| `new_initiative` | the user introduces a large new area spanning multiple stories | propose an initiative or epic intake boundary, identify candidate downstream stories, and select a likely first story shape without creating every story upfront |
| `maintenance_request` | the user asks for dependency, performance, architecture, operational, refactor, or similar technical work | propose a technical story boundary when needed and identify downstream validation report or decision obligations; usually touch fewer product contract artifacts unless behavior changes |
| `harness_improvement` | the user asks to improve Pulse workflow, process, templates, or harness behavior | propose a harness story or initiative boundary and identify downstream router/reference/template/runtime contract updates or harness backlog proposal obligations |

A read-only review of workflow or harness material does not by itself require intake mutation. If the review produces implementation or tracking work, classify that resulting work as `harness_improvement` and follow the normal boundary proposal and confirmation rules.

## Risk, lane, and affected surfaces

Risk flags are canonical uppercase identifiers everywhere: intake classification, `intake.md`, `state.intake.risk_flags`, workgraph metadata, and tests. Lowercase risk flag names are invalid.

Canonical risk flags:

| Risk area | Flag |
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

Lane rules:

```text
0-1 risk flags:
  tiny or normal, based on request impact and routing uncertainty

2-3 risk flags:
  normal with stronger validation expectations

4+ risk flags:
  high_risk

Any high-risk trigger:
  high_risk unless the user explicitly narrows scope
```

High-risk triggers:

- `AUTH`
- `DATA` when data loss is possible
- `MIGRATION`
- `SECURITY`
- `EXTERNAL_API`
- `WEAK_PROOF` when validation requirements are weakened
- `PUBLIC_CONTRACTS` when public behavior can break

Classify likely affected surfaces using request-level evidence only (no deep implementation analysis):

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

Use surfaces to explain why the next command is safe and to sanity-check input type, lane, and risk flags. Changes to `router`, `runtime`, or `workgraph` behavior are usually at least `normal`; downstream-facing changes to `product_contract` or `provider_metadata` usually add `PUBLIC_CONTRACTS`; public workflow behavior changes in `harness` usually classify as `harness_improvement`.

## Existing work correlation

Before creating any new epic or story boundary, intake must correlate the request against existing durable work. This is admission control, not optional enrichment.

Correlate against:

- current `works/` artifact structure
- current `.pulse/workgraph/items.jsonl` metadata
- linked product-contract or verification artifacts already referenced by candidate epic/story material
- direct evidence from the user request when they name a known story, epic, bug, or spec slice

Use the strongest available evidence in this order:

1. explicit item IDs or paths named by the user
2. existing open story or bug whose stated contract matches the request
3. existing closed story with closely related behavior and verification history
4. existing verification evidence that the requested outcome is already satisfied
5. narrow code evidence only when it is named by the user, already surfaced by `use`, or checkable with a targeted read
6. weak semantic similarity only

Choose exactly one correlation outcome:

| Outcome | Meaning | Default action |
| --- | --- | --- |
| `new_work` | no existing durable boundary adequately contains the request | propose a new epic/story boundary and ask for confirmation before creating it |
| `existing_open_work` | an open epic/story already owns the requested behavior | propose reuse/update of that boundary and ask for confirmation before writing new intake/context artifacts |
| `existing_closed_related_work` | a closed story is closely related, but the user is asking for additional behavior or a behavior delta | propose a new story linked to the closed story; do not reopen the closed story by default |
| `already_satisfied` | the request appears implemented already or fully covered by existing accepted behavior | do not create new execution work; present evidence and ask only if confirmation is needed |
| `ambiguous_or_blocked` | evidence conflicts, is too weak, or required context is missing | stop intake and ask the routing question |

### Duplicate and satisfaction check

After finding a likely boundary, explicitly check whether the request is already satisfied or merely a duplicate restatement of open work.

Ask:

- does an open item already describe the requested behavior closely enough that new boundaries would duplicate it?
- does a closed item plus current evidence show the behavior is already implemented and verified?
- is the user asking for a behavior delta over prior work rather than a bugfix or follow-up within that same story?

If proof is strong and the answer is `already_satisfied`, do not create new execution work. Present the evidence and stop, or ask one routing question if evidence is suggestive but not decisive.

If proving satisfaction requires broad codebase investigation, do not turn intake into `explore`; classify as `ambiguous_or_blocked` or route to `explore` with the suspected match presented in the response. Persist the suspected match only after the user explicitly approves recording the routing result.

### Behavior delta over closed work

A small follow-up change to a closed story is still new work when it changes accepted behavior. Do not reopen the closed story or add a task under it by default.

Instead:

- propose a new small story for the delta
- propose a traceability link to the prior closed story
- carry forward relevant context or verification evidence in the intake artifact after confirmation
- note why the old story is related but insufficient

## Work boundary

Choose exactly one boundary:

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
- If existing open work owns the request, use `single_story` and update that owning story boundary after confirmation instead of creating a task-shaped intake path.
- `harness_improvement` should be `single_story` or `initiative` when it changes public workflow behavior.
- `already_satisfied` uses boundary `none` and must not create a new boundary or execution work.
- `existing_closed_related_work` that changes behavior should propose a new `single_story` boundary linked to the closed story, not reopen it by default.
- `ambiguous_or_blocked` means `blocked` until the routing question is answered.

## Artifact obligations

Intake first produces a proposed intake package and asks the user to confirm any structural boundary before writing durable files. A confirmed intake produces the durable intake package.

Use [intake.template.md](intake.template.md) as the required shape for any boundary `intake.md` artifact.

Before confirmation, show the expected artifact path without creating files:

```text
Story boundary:
  works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/intake.md

Initiative or epic boundary:
  works/epics/<epic-id>-<epic-slug>/intake.md
```

After confirmation, determine boundary path source by mutation type:

- **New boundary creation path (`new_work`, `existing_closed_related_work`)**: use `dirname(item.content_path)` from the `node .gemini/skills/workflow/scripts/pulse.mjs workgraph create --json` result as the source of truth for the concrete boundary directory. For example, `works/epics/E-1-example/S-1-slice/README.md` means the confirmed intake artifact is `works/epics/E-1-example/S-1-slice/intake.md`.
- **Existing boundary update path (`existing_open_work`)**: do not run `workgraph create`. Use the matched item's existing `content_path` (and current status metadata) as the source of truth for the owning boundary directory, then target `<owning-boundary>/intake.md`.

For `existing_open_work`, the confirmed intake note belongs to the existing owning boundary. If that boundary does not have `intake.md`, create it after confirmation. If `intake.md` already exists, append a dated `Additional Intake` section by default using the append-only format in [intake.template.md](intake.template.md) (do not overwrite previous intake material unless the user explicitly confirms replacement).

Artifact obligations by input type:

- `new_spec`: confirmed spec intake boundary now; downstream product contract material, candidate epic analysis, validation shape, and decisions
- `spec_slice`: confirmed story slice boundary or update now; downstream related product contract links
- `change_request`: confirmed story update or new boundary now; downstream current behavior, target behavior, and verification matrix updates
- `new_initiative`: confirmed initiative or epic intake boundary now; downstream candidate story list and selected first story refinement
- `maintenance_request`: confirmed technical story boundary when needed; downstream validation report or decision
- `harness_improvement`: confirmed harness story or initiative boundary when needed; downstream router/reference/template/runtime contract updates or harness backlog proposal

## Confirmation packages

Intake has exactly three output package types.

| Package | Use when | Confirmation required | Writes allowed after confirmation |
| --- | --- | --- | --- |
| No-write package | result is terminal, blocked, routing-only, or not worth persisting | no | none |
| Runtime-only package | preserving a no-structural-mutation result is useful | yes, explicit runtime-record confirmation | update `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` together |
| Boundary package | confirmed epic/story creation or existing boundary update is needed | yes, explicit structural hard-gate confirmation | create/update workgraph and `works/` artifacts, then record matching runtime posture |

Use one confirmation for the whole proposed mutation package. If the user confirms a boundary package, that same confirmation covers the matching runtime posture update; do not ask for a second runtime-record approval.

Before any structural mutation, present this hard gate and stop:

```xml
<HARD_GATE>
Confirm the proposed intake boundary before any workgraph or works/ mutation.

Proposed workgraph operation(s):
- ...

Expected boundary path:
- ...

Expected intake artifact:
- .../intake.md

Reply with explicit approval to create/update this boundary, or provide corrections. Do not continue on silence or ambiguous acknowledgement.
</HARD_GATE>
```

Immediately before presenting this structural-mutation hard gate, run or verify a fresh `node .gemini/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` posture proof. This is separate from the initial posture gate in Phase 1. If proof is stale, missing, or contradicted by `.pulse/runtime`, `.pulse/workgraph`, or `works/` changes during classification, do not present the creation/update hard gate; route back to `pulse:workflow use` or ask the operator to rerun status first.

For runtime-only packages, ask for explicit confirmation before writing runtime mirrors:

```xml
<RUNTIME_CONFIRMATION>
No structural mutation will be performed.

Proposed runtime-only record:
- intake.status: ...
- intake.correlation_outcome: ...
- intake.matched_item_ids: ...
- intake.proposed_boundary: ... (only when recording an explicitly approved pending proposal; omit for terminal no-structural results)
- intake.recommended_next_command: ...

Reply with explicit approval to record this runtime state, or provide corrections. Do not continue on silence or ambiguous acknowledgement.
</RUNTIME_CONFIRMATION>
```

## Confirmed mutation procedure

After the user confirms the hard gate:

1. Run `node .gemini/skills/workflow/scripts/pulse.mjs status --repo-root <repo> --json` again.
2. Stop without mutation if active, resumable, conflicted, gated, reserved, executable, or ready work appeared since the hard gate.
3. Confirm that the approved proposal still matches current runtime/workgraph posture.
4. Apply only the confirmed boundary mutation path:
   - **New boundary creation path**: create the confirmed EPIC/STORY boundary using the workgraph CLI.
   - **Existing boundary update path (`existing_open_work`)**: do not create new EPIC/STORY items; reuse the matched open item boundary from its current `content_path`.
5. Write or append boundary `intake.md` using [intake.template.md](intake.template.md) and the confirmed classification details.
6. Record matching runtime posture only as permitted by the confirmed package and only when explicitly confirmed.
7. Recommend the next command.

Supported workgraph mutations (creation path only):

- one `EPIC` when a new epic or initiative boundary is needed
- one `STORY` when a story-sized boundary is known
- `linked_items` references for non-blocking traceability when the new story is related to prior closed work or parallel context

For `existing_open_work`, no create mutation is supported or needed. Intake may only update boundary artifacts (for example `intake.md`) and optional metadata updates that were explicitly confirmed and are supported by the CLI.

CLI patterns:

```bash
node .gemini/skills/workflow/scripts/pulse.mjs workgraph create --repo-root <repo> --kind EPIC --title "<confirmed epic title>" --json
node .gemini/skills/workflow/scripts/pulse.mjs workgraph create --repo-root <repo> --kind STORY --parent <epic-id> --title "<confirmed story title>" --json
node .gemini/skills/workflow/scripts/pulse.mjs workgraph link add --repo-root <repo> <new-story-or-epic-id> <related-item-id> --json
```

Add optional `--owner`, `--priority`, `--label`, and `--risk` flags only when they were part of the confirmed proposal. If a confirmed package requires both a new epic and a new story, create the epic first, read the returned `item.id`, then use that ID as the story `--parent`.

For existing boundaries, metadata updates are allowed only when both are true:

- the user explicitly confirmed those metadata changes in the intake hard gate package
- the specific update operation is supported by the current workgraph CLI

If either condition is false, do not mutate metadata; proceed with artifact-only intake updates or re-ask for a corrected confirmation package.

Do not hand-author IDs, guess final paths before `workgraph create` returns, run `workgraph create` for `existing_open_work`, create executable `TASK` or `BUG` metadata, create reservations, create ready queue entries, or approve gates.

## Runtime recording

Do not write intake posture to `.pulse/runtime/state.json` or `.pulse/runtime/STATE.md` before user confirmation.

When runtime posture is recorded, update both mirrors together in the same operation. State should express general routing posture with `active_command: intake` and `next_action` set for manual invocation by default. Intake-specific fields must live under `state.intake`:

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
    "recommended_next_command": null
  }
}
```

Field rules:

- use `state.intake.proposed_boundary` only after explicit confirmation of a package that permits runtime writes (runtime-only or boundary package)
- `state.intake.status: awaiting_creation_confirmation` is valid only when the user explicitly approved recording a pending proposal in runtime mirrors while structural mutation remains unapproved
- record active epic/story IDs only after confirmed creation succeeds
- use `state.intake.artifact_path` only after a durable intake artifact is written
- for `already_satisfied`, record matched item IDs and evidence summary only after explicit runtime-only confirmation permits recording the terminal result
- because `already_satisfied` is terminal, `state.intake.recommended_next_command` must be `null` (no downstream command)
- gate state remains `none` or `pre_gate`; Gate 1 begins only after `pulse:workflow design` produces an approvable solution design

No-structural-mutation outcomes:

| Outcome | Default package | Confirmed runtime-only package |
| --- | --- | --- |
| `already_satisfied` | present evidence and stop without writing files | record `status: already_satisfied`, matched IDs, evidence summary, and no downstream command |
| `ambiguous_or_blocked` | ask the routing question and stop without writing files | record `status: needs_user_routing` or `blocked`, evidence summary, open question, and recommended next action |
| routing-only recommendation | present the recommended next command and stop without writing files | record classification, correlation outcome, lane, and recommended next command |

Do not write `works/` artifacts or workgraph metadata for no-structural-mutation outcomes unless the user later confirms a boundary creation/update hard gate.

## Routing decision

Recommend exactly one next command, one immediate hard-gate action, or no command when intake reaches a terminal result such as `already_satisfied`.

| Situation | Next action |
| --- | --- |
| repository/session readiness is unclear | `pulse:workflow use` |
| structural workgraph or `works/` mutation is proposed | present the hard gate and stop until explicit approval |
| new spec or initiative lacks a selected story shape after confirmed boundary handling | `pulse:workflow brainstorm` |
| story intent exists but discovery evidence is needed after confirmed boundary handling | `pulse:workflow explore` |
| tiny or clear low-risk work needs Pulse-managed durability | `pulse:workflow design` when solution decisions are needed, otherwise `pulse:workflow plan` under the owning or newly confirmed story boundary |
| request maps to existing open work | propose reuse/update of the existing stream, hard-gate any artifact mutation, then recommend the next unmet command |
| request appears already satisfied with strong evidence | stop and present evidence; no new execution command |
| intake cannot safely classify or route | block and ask the routing question |

When structural work is needed, the next immediate action is hard-gate confirmation, not `brainstorm`, `explore`, or `plan`.

### Routing questions

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

`brainstorm` may follow intake when the work is real but the story shape, design direction, or first slice is still unclear. Brainstorm can ask for direction approval on `work-brief.md`, but it still does not replace solution design approval.

- Brainstorm direction approval remains before `explore` when needed.
- Gate 1 remains after `design`.
- Gate 2 remains after `plan`.
- Gate 3 remains after `validate`.
- Gate 4 remains after `review`.

Stop after intake when:

- lane is `normal` or `high_risk`
- boundary is `initiative`
- any routing question was needed
- structural workgraph or `works/` mutation is proposed and awaiting hard-gate approval
- the recommended next command is not obvious from existing artifacts

## Exit contract

Successful exit requires:

- fresh posture proof confirmed the session was empty before the intake posture gate, and was re-checked before any structural-mutation hard gate
- exactly one input type selected
- exactly one existing work correlation outcome selected
- duplicate/satisfaction evidence presented, and persisted only when included in a confirmed package
- lane selected with canonical uppercase risk flags and any high-risk trigger applied explicitly
- work boundary selected or blocker stated
- proposed workgraph operation(s), expected boundary path, and expected `intake.md` path presented before structural mutation
- explicit user confirmation received before creating/updating EPIC/STORY metadata or writing boundary `intake.md`
- durable `intake.md` written under `works/` only after confirmation when a work boundary is created or updated
- structural EPIC/STORY metadata created only when needed and only after confirmation
- runtime state updated with intake posture and no gate approval only when included in a confirmed package
- exactly one next command, hard-gate action, routing question, or terminal no-command result stated

## Red flags

Stop if you catch yourself:

- running intake without fresh empty-session proof
- continuing while active, resumable, conflicted, gated, reserved, executable, or ready work exists
- creating or updating EPIC/STORY metadata before explicit user confirmation
- writing boundary `intake.md` before explicit user confirmation
- creating executable tasks, bugs, reservations, ready queue entries, or gate approvals
- planning implementation details, doing deep codebase analysis, or validating readiness
- creating every story for a large initiative upfront
- treating input type as decorative metadata
- opening new work without checking existing open, closed, or already-satisfied work
- reopening a closed story by default when the request is a behavior delta
- routing high-risk work directly to execution
