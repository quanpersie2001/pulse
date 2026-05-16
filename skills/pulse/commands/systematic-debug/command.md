# `/pulse systematic-debug`

Root-cause-first debugging command: investigate, prove, fix minimally, and lock regressions.

Order is mandatory: evidence -> hypothesis -> reproduction -> fix -> verification -> regression protection.

## When to run

Use when defects block progress and root cause is unknown, disputed, or repeatedly misdiagnosed:

- regressions
- flaky tests
- build/runtime failures
- integration/data-flow defects

If repeated debugging attempts reveal architecture or boundary mismatch, escalate to `rescue`.

## Required reads

Before choosing a fix path, read:

1. active item context in `.pulse/workgraph/items.jsonl` for the issue slice
2. current gate/runtime posture in `.pulse/runtime/state.json` (and `STATE.md` when helpful)
3. `skills/pulse/commands/systematic-debug/references/root-cause-tracing.md`
4. `skills/pulse/commands/systematic-debug/references/defense-in-depth.md`
5. `skills/pulse/commands/systematic-debug/references/condition-based-waiting.md` when timing or async instability appears

## Inputs

- reproducible symptom and affected scope
- logs, traces, stack evidence, and environment deltas
- prior failed hypotheses/fix attempts
- dependency ordering if multiple issues exist

## Phase order (strict)

### Phase 1 — Frame the issue set

Classify as single-issue, multi-issue, or mixed.

For multi-issue tracks, create a compact tracker (one row per issue):

- ID
- symptom
- where
- dependency/severity
- root-cause hypothesis
- verification state
- regression coverage status

Work dependency-first, then severity.

### Phase 2 — Reproduce before editing code

Establish the smallest reliable failing signal.

If repro is unstable:

- gather more evidence
- instrument boundaries
- avoid speculative edits

### Phase 3 — Trace backward to source trigger

Follow bad state/value upstream to origin.

- compare broken path vs known-good reference when available
- instrument cross-component boundaries when needed
- state one explicit hypothesis before any fix

Do not patch at explosion point when root cause is upstream.

### Phase 4 — Create failing proof artifact

Before fix, produce one deterministic failing artifact:

- targeted failing test (preferred)
- stable repro command/script
- manual reproduction only when automation is genuinely impractical

Failure must be demonstrated pre-fix.

### Phase 5 — Apply minimal focused fix

- one issue at a time
- one focused fix at a time
- no speculative multi-fix bundles
- no unrelated cleanup mixed in

If a fix fails, return to investigation with new evidence.

Escalation trigger:

- repeated failed fix attempts (especially three attempts)
- fixes revealing new symptoms across other boundaries

Route to `rescue` when pattern indicates structural mismatch.

### Phase 6 — Verify and lock regression

Per completed issue:

1. rerun original failing proof
2. run nearest relevant suite
3. add regression coverage by default:
   - exact failure case
   - boundary variants
   - sibling/family coverage where justified

Exception path (automation impractical):

- record why automation is impractical now
- preserve strongest manual/command verification evidence
- record follow-up needed to make future automation possible

### Phase 7 — Close with residual risk

Report:

- root cause per issue
- before/after verification evidence
- regression coverage summary
- unresolved/deferred risks and next checks

When the issue belongs to a tracked work item, write verification evidence in that item’s `works/**/verification.md` before closure.

## Output contract

- root-cause statement per issue
- fix evidence trail
- regression lock-down summary
- unresolved risks/follow-ups

## Stopping rules

Stop and reassess when:

- fixes are speculative
- reproduction remains non-deterministic after investigation
- repeated fix attempts fail
- bug diagnosis reveals architecture/shape drift (`rescue`)

Never claim completion without verification evidence.

## Guardrails

- no “fix first, explain later”
- no sleep/retry inflation without proven timing cause
- no umbrella tracker rows hiding multiple failures
- no success claim without reproducible proof + regression posture

## Anti-patterns

- changing multiple variables to “see what works”
- skipping failing proof because cause “looks obvious”
- merging unrelated refactors into bug-fix path
- adding brittle timing hacks instead of tracing causality

## Next command guidance

- `execute` when approved fix path is clear and bounded
- `review` when verified fixes are complete
- `rescue` when debugging exposes structural mismatch
