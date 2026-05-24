# `pulse:workflow explore`

Discovery and evidence-gathering manual for turning approved work direction into a durable research base for solution design.

Explore answers:

> What does the repo, domain, and external evidence say that design must account for?

It does **not** choose the final solution, lock product/technical decisions, create task breakdowns, or prepare implementation work. Those decisions belong after discovery.

## Mission

Produce a story-scoped `discovery.md` and any needed `references/*.md` research reports so the next workflow step can make final decisions without guessing.

Explore gathers:
- current repo behavior and architecture evidence
- relevant docs, story artifacts, runtime state, and tests
- existing patterns and constraints
- contradictions or drift between artifacts and implementation
- external/domain/library/provider/security evidence when needed
- decision questions that must be resolved later

## Entry criteria

Run `pulse:workflow explore` when:

- intake has confirmed an owning work boundary
- work direction exists, either directly from intake or from `work-brief.md`
- discovery/evidence is needed before final solution decisions can be made
- no later approved solution design already covers the exact current scope without drift

Do not run when:

- repo readiness/session posture is stale or blocked
- the work boundary is unclear
- the request is asking to choose final design/solution without enough discovery
- the request is task breakdown or implementation work

## Required reads before research

Read in this order when present:

1. owning story `intake.md`
2. story `work-brief.md`
3. existing story `discovery.md` if resuming or refreshing discovery
4. `.pulse/runtime/STATE.md` and `.pulse/runtime/state.json`
5. relevant story references under `works/epics/<epic>/<story>/references/`
6. targeted repo docs, code, tests, runtime files, and recent history needed for the discovery scope

Rule: answer from evidence first. Ask users only for research scope clarification or missing context evidence cannot provide.

## Command-local references

- [`discovery.template.md`](discovery.template.md) — required `discovery.md` structure
- [`context-reviewer-prompt.md`](context-reviewer-prompt.md) — discovery reviewer prompt
- [`gray-area-probes.md`](gray-area-probes.md) — probe bank for discovery questions and decision surfaces

## Phase model

### Phase 0 — Scope and research depth

Classify discovery depth:

| Depth | Use when |
| --- | --- |
| `quick` | direction is clear and repo surface is small |
| `standard` | normal code/docs/test discovery is needed |
| `deep` | architecture, data, security, integration, migration, public contract, or external evidence materially affects later decisions |

If unclear, ask one scope question. Do not start broad research without knowing what decision surface the evidence must support.

Stop when:
- no confirmed story boundary exists
- the requested research belongs to another story
- the user asks for final design instead of discovery

### Phase 1 — Evidence map

Identify which evidence surfaces matter:

- `ARTIFACTS` — intake, work brief, prior discovery, references
- `CODE` — implementation paths, call sites, module boundaries
- `TESTS` — existing coverage, fixtures, verification harness
- `DATA` — schema, migrations, persistence, ownership model
- `RUNTIME` — state, commands, operational behavior
- `DOCS` — product docs, API contracts, README/reference docs
- `EXTERNAL` — provider/library/domain/security/current-state research

For each surface, record why it matters or why it is out of scope.

### Phase 2 — Repo and artifact discovery

Gather evidence without deciding the final solution:

- trace relevant existing behavior
- identify established patterns and constraints
- list reusable surfaces and risky coupling
- find contradictions between artifacts, docs, code, tests, and runtime state
- identify missing evidence
- record paths and concrete observations

Do not turn findings into final decisions. Write findings as evidence:

```text
Finding: Current request routing already uses direct handler-like functions in `path`.
Implication for design: design should decide whether to preserve direct flow or introduce an abstraction.
```

Not:

```text
Decision: Use direct handlers.
```

### Phase 3 — Deep-research invocation when needed

Invoke or follow [`skills/deep-research/../../../deep-research/SKILL.md`](../../../deep-research/SKILL.md) when external evidence is needed for later design decisions.

Use deep research for:
- external provider/API behavior
- library/framework trade-offs
- security/privacy/compliance guidance
- architecture or scaling patterns
- data partitioning/multi-tenant strategies
- market/domain/product conventions
- current-state research that cannot be answered from repo artifacts

Deep-research output must be saved under the owning story:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/references/<topic-slug>.md
```

Reference file requirements:
- research question
- why it matters for this story
- executive summary
- key findings with citations
- design implications, clearly labeled as implications rather than decisions
- risks/gaps
- sources and methodology

Do not rely on web snippets alone when external research affects design. Read key sources in full where practical.

### Phase 4 — Decision surface extraction

From the evidence, identify questions the later solution design must resolve.

A valid design question:
- materially affects product behavior, technical design, data shape, API/UX, migration, verification, or risk
- is grounded in evidence or a documented gap
- is within the confirmed story boundary

Write questions as decision inputs, not decisions:

```text
Design must decide:
- whether to preserve direct handler flow or introduce a mediator-style abstraction
- whether tenant ownership maps to existing Organization or needs a new concept
```

### Phase 5 — Assemble `discovery.md`

Write:

```text
works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/discovery.md
```

Use [`discovery.template.md`](discovery.template.md).

Required content:
- research scope and depth
- source inputs read
- research questions investigated
- evidence by surface with concrete paths/citations
- external references created under `references/`
- existing patterns and constraints
- contradictions or drift
- risks and confidence
- candidate options surfaced by evidence, without selecting the final solution
- decision surface for design
- open questions split into blocking vs deferrable

### Phase 6 — Discovery self-review

Run a self-review using [`context-reviewer-prompt.md`](context-reviewer-prompt.md).

The review must catch:
- unsourced claims
- missing required evidence surfaces
- final design decisions leaking into discovery
- task planning/work breakdown leakage
- missing deep-research references where external evidence is required
- unresolved contradictions hidden as assumptions
- unclear handoff questions for design

Fix serious issues and rerun once. After two failed repair loops, stop and ask for direct user review.

### Phase 7 — Handoff

After discovery passes review:

1. Update runtime mirrors together if recording workflow posture:
   ```text
   Current: exploration/discovery complete for <work>
   Discovery: <works story discovery.md path>
   References: <works story references/ paths if any>
   Next: invoke pulse:workflow design
   ```

2. Present a concise handoff:
   > Discovery complete. Next step: `pulse:workflow design` to turn evidence into final solution decisions.

3. Do not invoke `pulse:workflow design` unless the user explicitly asks to continue.

## Role boundaries

Explore owns:
- discovery scope
- evidence gathering
- external research orchestration
- source/citation quality
- contradiction and risk surfacing
- decision-question handoff for design

Explore does not own:
- final product decisions
- final technical design
- architecture/API/schema/UX selection
- migration or rollout plan as final design
- task breakdown
- work item creation
- implementation

## Pause/resume posture

If pausing:
- record current research depth, completed evidence surfaces, pending research questions, and next source to inspect
- keep partial findings clearly marked as partial
- do not mark discovery complete while blocking evidence is missing

## Red flags

Stop if you catch yourself:
- writing final decisions instead of findings/implications
- choosing architecture, schema, API, or UX solution as final
- creating tasks or execution slices
- treating external snippets as sufficient evidence for high-risk decisions
- skipping deep-research when external/provider/security evidence is material
- writing discovery outside the owning story directory
- saving references outside `works/**/references/`
- routing directly to plan

## Exit contract

Successful exit requires:
- `discovery.md` written under the owning story
- all material claims backed by paths, citations, command output, or explicit gaps
- external research saved under `references/<topic-slug>.md` when used
- no final solution design or task breakdown in discovery
- clear decision surface for `pulse:workflow design`
- next command recommendation: `pulse:workflow design` (manual invoke by default)
