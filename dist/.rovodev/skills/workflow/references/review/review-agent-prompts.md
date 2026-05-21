# Review Agent Prompts

Use these focused prompts to run specialist review passes.

## Behavior reviewer

"Review this changeset for functional correctness against approved current-slice scope and expected behavior. List concrete mismatches with reproducible symptoms."

## Regression reviewer

"Review for regressions across neighboring flows and boundary contracts. Identify affected paths and user-visible risk if unaddressed."

## Security reviewer

"Review for security weaknesses introduced or exposed by this changeset. Prioritize auth, data handling, trust boundaries, and unsafe input/output handling."

## Evidence reviewer

"Validate whether verification evidence is fresh, specific, and sufficient for each closed item. Flag stale, missing, or non-reproducible records."

## Release synthesizer

"Consolidate findings, deduplicate overlap, assign severity/owner, and decide Gate 4 posture with explicit pass/fail rationale."
