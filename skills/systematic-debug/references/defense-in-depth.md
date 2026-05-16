# Defense in Depth

Use this reference after finding a root cause that involves invalid input, unsafe state, or a dangerous operation.

## Goal

Do not only patch the exact failure point. Make the bug hard or impossible to reintroduce through nearby code paths.

## Validation Layers

### 1. Entry Validation

Reject obviously invalid input at the first public boundary.

Examples:

- empty identifiers
- missing required fields
- paths that do not exist
- values outside allowed ranges

### 2. Business Logic Validation

Validate assumptions again where the operation becomes meaningful.

Examples:

- state transition is allowed
- object ownership is correct
- a resource is initialized before use

### 3. Environment Guards

Block dangerous behavior in special contexts such as tests, CI, migrations, or production safety boundaries.

Examples:

- refuse writes outside temp directories in tests
- refuse destructive operations without an explicit flag
- stop a job when required credentials are missing

### 4. Debug Instrumentation

Add enough context to explain the next failure quickly.

Examples:

- log the target path before a write
- log the active state before a transition
- capture stack traces around risky operations

## How to Apply It

1. Map the full data flow from source to symptom.
2. List the checkpoints where the bad data passes through.
3. Decide which validation belongs at entry, business, environment, and debug layers.
4. Add only the layers that materially improve safety for this bug family.
5. Test that bypassing one layer still gets caught by another when appropriate.

## Rule of Thumb

If one check can be bypassed by a different caller, a refactor, or a mock, one check is not enough.
