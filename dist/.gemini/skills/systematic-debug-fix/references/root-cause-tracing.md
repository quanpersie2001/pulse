# Root Cause Tracing

Use this reference when the visible failure happens far away from the real bug source.

## When to Read This

- Stack traces are long.
- The error shows up deep inside helpers, infrastructure, or libraries.
- The bad value is visible, but its origin is unclear.
- Tests or background jobs create side effects and the triggering caller is unknown.

## Procedure

1. Record the exact symptom.
2. Find the immediate line that throws, writes, mutates, or fails.
3. Ask what called that line and with which value.
4. Keep walking backward through the call chain until the original trigger is found.
5. Fix at the earliest point that can prevent the bad state.
6. Add validation or instrumentation on the path back down if the operation is risky.

## What to Capture

- the dangerous input or state
- the caller that passed it
- the first place where the value becomes invalid
- the test, request, or job that started the chain

## Instrumentation Tips

- Log before the dangerous operation, not after it fails.
- Capture input values, current working context, and a stack trace when possible.
- In tests, prefer output that is guaranteed to surface in the test runner.

## Stop Conditions

Do not stop tracing just because the immediate cause is obvious. Stop only when one of these is true:

- the source of the invalid value is identified
- a system boundary prevents further tracing and the remaining uncertainty is documented
- the issue is confirmed to be external, environmental, or nondeterministic and the evidence supports that conclusion

## Outcome

A good trace ends with a sentence like:

`The failure happens in X, but the root cause starts in Y when Z passes invalid state into the chain.`
