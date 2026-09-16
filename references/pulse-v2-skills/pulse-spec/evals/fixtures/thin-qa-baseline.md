# Withdraw a published release — QA baseline

## Scope

Withdrawal removes a bundle from a channel and the poll endpoint stops serving
it.

## Posture

required

## Risks

- RISK-STALE: a device keeps serving a withdrawn bundle.

## Exit criteria

- All cases pass.

## Cases

### QA-001 Withdrawing works

- Intent: The operator withdraws a bundle and it works.
- Surface: api
- Priority: high
- Owner: platform
- Steps:
  1. POST /channels/stable/withdraw with the current bundle id.

### QA-002 Poll returns 404 after withdrawal

- Intent: A channel with no current bundle answers 404.
- Surface: api
- Priority: high
- Risks: RISK-STALE
- Steps:
  1. Withdraw the current bundle.
  2. GET /channels/stable/current.
- Expected:
  - HTTP 404 with body.code = no_current_bundle.

```pulse-check
run: node scripts/qa/withdraw.mjs && node scripts/qa/poll.mjs
assert:
  - exit_code: 0
```

### QA-003 Handler sets the status column

- Intent: `WithdrawHandler` sets `bundles.status` to the withdrawn value.
- Surface: api
- Priority: medium
- Steps:
  1. Call the handler.
- Expected:
  - The column holds the withdrawn value.

### QA-004 Operator sees the confirmation dialog

- Intent: The operator is asked to confirm before withdrawal completes.
- Surface: ui
- Priority: low
- Applicability: not_applicable
- Steps:
  1. Open the channel page and press Withdraw.
- Expected:
  - A confirmation dialog appears.
