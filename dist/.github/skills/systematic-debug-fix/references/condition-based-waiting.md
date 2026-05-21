# Condition Based Waiting

Use this reference when a flaky test or async flow currently depends on guessed timing.

## Goal

Wait for the condition that matters instead of guessing how many milliseconds it might take.

## Read This When

- tests use `sleep`, `setTimeout`, or arbitrary delays
- failures appear only under load or in CI
- async state sometimes updates too slowly for a fixed timeout
- parallel test runs make timing more unstable

## Preferred Pattern

1. Identify the real condition that means the system is ready.
2. Poll or subscribe until that condition becomes true.
3. Keep a timeout with a clear error message.
4. Re-read fresh state each time instead of caching stale values outside the loop.

Examples of conditions:

- an event exists
- a state machine reaches `ready`
- a file appears
- a queue length reaches a threshold
- a computed result becomes non-empty

## Keep Arbitrary Timeouts Only When

- the test is explicitly verifying timing behavior
- the duration is derived from known system behavior, not a guess
- the reason for that timing is documented inline

## Common Mistakes

- polling too fast and wasting CPU
- omitting a timeout and hanging forever
- reading stale state inside the loop
- using a timeout first instead of waiting for the actual trigger

## Rule of Thumb

If the assertion is about readiness, availability, or completion, prefer condition-based waiting over fixed delays.
