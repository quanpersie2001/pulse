# `pulse:workflow intake`

New-work admission manual for turning raw user input into a classified Pulse work stream before brainstorming, exploration, planning, or execution begins.

This command is the only place where a new user request enters an empty Pulse workflow session.

## Mission

Classify the new request, determine which Pulse artifacts must be created or updated, resolve whether the request belongs to existing work or is already satisfied, and route the operator to the next safe workflow command.

Intake answers:

- what type of work is this?
- what existing or new work boundary should contain it?
- does existing work already cover, satisfy, or block this request?
- which artifacts must be created or updated downstream?
- how risky is it?
- what command should run next?

Intake does not design the solution, lock implementation decisions, create execution tasks, validate readiness, or implement code.

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
Choose work boundary: direct_task, single_story, initiative, or blocked
    |
    v
Define artifact obligations
    |
    v
Recommend next command
```

The flow is intentionally admission-focused. Stop as soon as one step cannot be completed safely, record the blocker, and do not proceed to downstream workflow commands.

## Entry criteria

Run `pulse:workflow intake <user input>` only when all are true:

- `pulse:workflow use` has just established readiness and loaded the session
- session load reports no active, resumable, conflicted, or executable work
- `.pulse/runtime/state.json` has no active epic, story, item, command, or pending gate
- `.pulse/runtime/handoffs/manifest.json` has no active handoff to resume
- `.pulse/workgraph` has no ready execution work for an existing stream
- the user provided new input to classify

If any current work exists, block immediately.

Block message shape:

```text
Intake blocked: current Pulse session is not empty.
Run pulse:workflow use, then resume, review, close, compound, or explicitly abandon the existing work before starting new intake.
```

Intake must not be used as a side channel to start unrelated work while an existing epic, story, task, bug, handoff, or gate is still open.

## Input type classification

Choose exactly one primary input type.

| Input type | Use when | Flow effect |
| --- | --- | --- |
| `new_spec` | the user provides a completely new product or project spec | create spec intake, initial product contract material, candidate epics, validation shape, and decisions |
| `spec_slice` | the user selects work from an existing spec or product contract | find related contract material and create or update the story for that slice |
| `change_request` | the user changes or extends accepted behavior | update existing contract material, update or create the story, and update verification expectations |
| `new_initiative` | the user introduces a large new area spanning multiple stories | create initiative or epic material, identify candidate stories, and select a first story without creating every story too early |
| `maintenance_request` | the user asks for dependency, performance, architecture, operational, refactor, or similar technical work | create a story, validation report, or decision when needed; usually touch fewer product contract artifacts unless behavior changes |
| `harness_improvement` | the user asks to improve Pulse workflow, process, templates, or harness behavior | update router/reference/template/runtime contract material or record a proposal in `.pulse/harness/HARNESS_BACKLOG.md` |

Input type determines artifact obligations and flow constraints. It is not just a label.

## Lane classification

After input type, choose a lane:

- `tiny`
- `normal`
- `high_risk`

Risk flags:

- `auth`
- `authorization`
- `data_model`
- `audit_security`
- `external_systems`
- `public_contracts`
- `cross_platform`
- `existing_behavior`
- `weak_proof`
- `multi_domain`

Classification rules:

```text
0-1 risk flags:
  tiny or normal, based on implementation impact

2-3 risk flags:
  normal with stronger validation

4+ risk flags:
  high_risk

Any hard gate:
  high_risk unless the user explicitly narrows scope
```

Hard gates:

- auth
- authorization
- data loss or migration
- audit/security
- external provider behavior
- weakening validation requirements
- public contract breakage

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
4. code or verification evidence that the requested outcome is already satisfied
5. weak semantic similarity only

If correlation is weak, do not guess. Ask the minimum routing question needed before creating boundaries.

### Correlation outcomes

Intake must classify the request into exactly one of these outcomes before artifact creation:

| Outcome | Meaning | Default action |
| --- | --- | --- |
| `new_work` | no existing durable boundary adequately contains the request | create a new epic/story boundary as needed |
| `existing_open_work` | an open epic/story already owns the requested behavior | route into that existing boundary and update its intake/context artifacts |
| `existing_closed_related_work` | a closed story is closely related, but the user is asking for additional behavior or a behavior delta | create a new story linked to the closed story; do not reopen the closed story by default |
| `already_satisfied` | the request appears implemented already or fully covered by existing accepted behavior | do not create new execution work; present evidence and ask only if confirmation is needed |
| `ambiguous_or_blocked` | evidence conflicts, is too weak, or required context is missing | stop intake and ask the routing question |

### Duplicate and satisfaction check

After finding a likely boundary, explicitly check whether the request is already satisfied or is merely a duplicate restatement of open work.

Ask:

- does an open item already describe the requested behavior closely enough that new boundaries would be duplication?
- does a closed item plus current evidence show the behavior is already implemented and verified?
- is the user actually asking for a new behavior delta over prior work rather than a bugfix or follow-up within that same story?

If the answer is `already_satisfied`, intake must not create new execution work.
Record the evidence, explain why the request looks satisfied, and either:

- stop with an `already_satisfied` routing result, or
- ask one routing question if the evidence is suggestive but not decisive

Weak proof is not enough to close the loop silently.
When proof is weak, prefer `ambiguous_or_blocked` and ask the user to confirm whether the desired behavior already exists.

### Behavior delta over closed work

A small follow-up change to a closed story is still new work when it changes accepted behavior.
Do not reopen the closed story or add a task under the closed story by default.
Instead:

- create a new small story for the delta
- link it to the prior closed story for traceability
- carry forward any relevant context or verification evidence
- note why the old story is related but insufficient

This preserves closure semantics while keeping the new request visible as a separate behavior change.

## Work boundary

Choose one boundary:

- `direct_task`
- `single_story`
- `initiative`
- `blocked`

Rules:

- `new_initiative` normally produces an `initiative` or epic boundary plus a selected first story.
- `new_spec` may produce an initiative boundary or a first story when the first slice is obvious.
- `spec_slice` normally produces a `single_story` boundary.
- `change_request` may be `direct_task` only when tiny, clear, low-risk, and clearly belongs to existing open work.
- `maintenance_request` may be `direct_task` or `single_story` depending blast radius.
- `harness_improvement` should be `single_story` or `initiative` when it changes public workflow behavior.
- `already_satisfied` work must not create a new boundary or new execution work.
- `existing_closed_related_work` that changes behavior should produce a new `single_story` boundary linked to the closed story, not reopen it by default.
- `ambiguous_or_blocked` means `blocked` until the routing question is answered.

Do not create an execution-ready `TASK` or `BUG` during intake.

## Artifact output

Successful intake produces an intake package.

If a story boundary is known, write:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/INTAKE.md
```

If only an initiative or epic boundary is known, write:

```text
works/epics/<epic-id>-<epic-slug>/INTAKE.md
```

Temporary runtime notes may be used while classifying, but successful intake must leave the durable result under `works/`.

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

Artifact obligations should state what downstream commands must create or update. Examples:

- `new_spec`: spec intake, product contract material, candidate epics, validation shape, decisions
- `spec_slice`: story slice and related product contract links
- `change_request`: current behavior, target behavior, story update, verification matrix update
- `new_initiative`: initiative or epic intake, candidate story list, selected first story
- `maintenance_request`: validation report, decision, or technical story if needed
- `harness_improvement`: router/reference/template/runtime contract updates or harness backlog proposal

## Workgraph output

Intake may create only structural metadata:

- one `EPIC` when a new epic or initiative boundary is needed
- one `STORY` when a story-sized boundary is known
- `linked_items` references for non-blocking traceability when the new story is related to prior closed work or parallel context
- links from metadata to the durable `INTAKE.md` content path

If correlation outcome is `existing_open_work`, update or extend the existing structural metadata instead of creating a duplicate story.
If correlation outcome is `already_satisfied`, do not create new EPIC/STORY/TASK/BUG metadata unless the user clarifies that the behavior is actually different.

Intake must not create execution work:

- no executable `TASK` or `BUG`
- no reservations
- no ready queue entries
- no Gate 1, Gate 2, Gate 3, or Gate 4 approvals

## Runtime output

Record intake posture in `.pulse/runtime/state.json` and `.pulse/runtime/STATE.md` without approving any gate.

State should express:

- `active_command: intake`
- active epic/story IDs when created
- intake status: `classified`, `needs_user_routing`, `blocked`, or `approved`
- input type
- lane
- risk flags
- artifact path
- recommended next command
- next action: manual invoke by default

Gate state remains `none` or `pre_gate`. Gate 1 begins only after `pulse:workflow explore` produces an approvable context artifact.

## Routing decision

Recommend exactly one next command.

| Situation | Next command |
| --- | --- |
| repository/session readiness is unclear | `pulse:workflow use` |
| new spec or initiative lacks a selected story shape | `pulse:workflow brainstorm` |
| story intent exists but behavior/context decisions need locking | `pulse:workflow explore` |
| tiny or clear low-risk work has enough context for execution shaping | `pulse:workflow plan` |
| request maps to existing open work | route to the existing stream and recommend the next unmet command in that stream |
| request appears already satisfied with strong evidence | stop and present the evidence; no new execution command |
| intake cannot safely classify or route | block and ask the routing question |

Default continuation is manual. Do not invoke the next command unless the user explicitly asks to continue now.

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

For non-trivial intake results, stop after presenting the routing decision. Do not auto-invoke the next command unless the user explicitly asks to continue.

Stop after intake when:

- lane is `normal` or `high_risk`
- boundary is `initiative`
- any routing question was needed
- the recommended next command is not obvious from existing artifacts

## Exit contract

Successful exit requires:

- fresh `use` result confirmed the session was empty before intake
- input type selected
- existing work correlation outcome selected
- duplicate/satisfaction evidence recorded
- lane selected with risk flags
- work boundary selected or blocker stated
- durable `INTAKE.md` written under `works/`
- structural EPIC/STORY metadata created only when needed
- runtime state updated with intake posture and no gate approval
- next command recommendation stated

## Red flags

Stop if you catch yourself:

- running intake while active or resumable work exists
- creating executable tasks or bugs
- approving gates
- planning implementation details
- doing deep codebase analysis
- creating every story for a large initiative upfront
- treating input type as decorative metadata
- opening a new story without checking for existing open, closed, or already-satisfied work
- reopening a closed story by default when the request is actually a behavior delta
- routing high-risk work directly to execution
