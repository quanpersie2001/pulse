---
name: systematic-debug-fix
description: Use when the user asks for root-cause-first debugging of bugs, test failures, flaky behavior, build issues, or integration defects with tracked verification and regression lock-down.
metadata:
  version: '1.1'
  dependencies: []
---

# Systematic Debug Fix

Investigate first. Fix second. Lock down with tests third.

Do not propose or implement a fix until there is evidence for the root cause.

## Workflow

### 1. Frame the Work

Start by deciding whether the request is:

- a single issue that still needs root-cause analysis
- multiple known issues that need tracking and cleanup
- a mixed case where some issues are known and some still need diagnosis

If more than one issue exists, create a tracker before editing code. Use one row per issue, not umbrella buckets.

Track at least:

- `ID`
- `Symptom`
- `Where`
- `Severity or dependency`
- `Root cause`
- `Status`
- `Verification`
- `Regression coverage`

Use labels such as `BUG-1`, `BUG-2`.

Order work by dependency first, then severity.

### 2. Investigate Root Cause

For the current issue, complete investigation before fixing:

1. Read the error, stack trace, failing assertion, or user report carefully.
2. Reproduce the issue consistently. If reproduction is unstable, gather more evidence before changing code.
3. Check recent changes, config differences, dependency changes, and environment differences.
4. Trace the bad value or bad state backward until the original trigger is found.
5. Compare the broken path with a working example or reference implementation when one exists.
6. Write one explicit hypothesis: `I think X is failing because Y`.

When the system crosses component boundaries, add instrumentation first. Log what enters and exits each layer so the failing boundary is visible before choosing a fix.

If the issue appears deep in the stack, fix at the source, not at the point where the symptom explodes.

Use the bundled references when the bug matches these patterns:

- Read `references/root-cause-tracing.md` when the symptom appears deep in the stack and the bad input must be traced backward.
- Read `references/defense-in-depth.md` when the root cause involves invalid data, unsafe operations, or missing guards across layers.
- Read `references/condition-based-waiting.md` when a flaky test or async flow currently relies on sleeps, timeouts, or guessed timing.

## Fixing Rules

- Do not stack multiple speculative fixes.
- Do not say "probably" and edit anyway.
- Do not hide uncertainty. If something is unclear, keep investigating.
- Do not batch unrelated fixes into one pass.

If one attempted fix fails, return to investigation with the new evidence.

If three fix attempts have failed, or each fix reveals a new symptom in a different area, stop and question the architecture before continuing.

### 3. Create a Reproduction Before the Fix

Create the smallest failing test or stable reproduction that proves the issue exists.

Prefer:

- an automated test in the project's existing framework
- a targeted repro command or script when no test harness exists
- a manual reproduction only when automation is genuinely impractical

The reproduction should fail before the fix and pass after the fix.

### 4. Fix One Issue at a Time

Apply the smallest change that addresses the confirmed root cause.

For each issue:

1. Make one focused fix.
2. Run the narrowest relevant reproduction or failing test.
3. Run broader validation for the touched area so the fix does not break neighbors.
4. Update the tracker with what changed and how it was verified.
5. Move to the next issue only after the current one is verified.

If a fix uncovers a separate problem, add a new tracker item instead of folding it into the current one.

Avoid "while I'm here" refactors unless they are required to complete the root-cause fix safely.

### 5. Lock Down Regressions

After the fixes are verified, add automated regression coverage for every completed issue by default.

For each issue, add:

- one exact reproduction test for the original failure
- boundary variants around the failure edge
- sibling coverage for the same bug family elsewhere in the codebase

Exception path (use only when automated regression coverage is genuinely impractical):

- document why automation is impractical in this environment right now
- preserve the strongest available reproduction evidence for the fixed behavior
- record the manual or command-based verification that was run instead
- list follow-up needed to make the behavior testable later when applicable

Common bug families:

- input validation
- state management
- boundary conditions
- data flow
- API contracts
- error handling
- concurrency

Prefer real code paths over mock-heavy tests. If the project has both unit and integration layers, place regression tests at the highest level that would have caught the original bug.

Use descriptive test names that explain the behavior being protected.

### 6. Verify the Lock-Down

For each new test:

- confirm it passes with the fix
- explain why it would fail without the fix, or demonstrate that failure if practical
- run the relevant suite to ensure the new coverage does not conflict with existing tests

Finish with a coverage check:

- every tracker item is resolved or explicitly deferred
- every completed fix has direct automated regression coverage, or a justified exception record with durable evidence
- every important bug family has boundary or sibling coverage where justified

## Red Flags

Stop and return to investigation if you catch yourself doing any of these:

- proposing a fix before tracing the data flow
- making multiple changes to "see what works"
- adding sleeps or retries without proving timing is the real cause
- skipping reproduction because the fix "looks obvious"
- claiming success without test or command evidence
- continuing past repeated failed fixes without questioning the design

## Output Contract

When using this skill, keep the work visible:

1. Show the issue tracker when multiple items exist.
2. State the current hypothesis before the first fix for an issue.
3. Record what verification ran after each fix.
4. Summarize added regression coverage by issue and bug family.
5. Call out anything still unverified or deferred.

## Completion Standard

Do not declare the work done until:

- the root cause is identified or the remaining uncertainty is explicitly documented
- each fix is verified with evidence
- automated regression coverage exists for the completed issues, or a justified exception includes durable verification evidence and follow-up where applicable
- unresolved risks or follow-ups are listed clearly

## Integration with Pulse

Can be invoked during `pulse:executing` for beads that involve bug fixes or defect resolution. When used within Pulse, feed the fix tracker output and regression test results into the bead's verification evidence and `learning_refs` for `pulse:compounding`.
