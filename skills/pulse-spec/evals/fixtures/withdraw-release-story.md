# Withdraw a published release

## Outcome

An operator can pull back a bundle that already went out to a channel, so that
devices which have not yet fetched it never receive it, and devices that already
hold it are told the bundle is no longer current.

## Success signals

- A withdrawn bundle stops being returned by the channel poll endpoint.
- A device that already holds a withdrawn bundle learns it is no longer current
  the next time it polls.
- The bundle id of a withdrawn bundle is never handed out again.
- Withdrawing a bundle that was never published fails with a stated error rather
  than succeeding silently.

## Scope boundary

In scope: the operator-facing withdraw action, the poll response for a withdrawn
bundle, and the audit record of who withdrew what.

Out of scope: automatic rollback to the previous bundle — moved to a separate
slice, because the operator decided withdrawal and rollback are two actions.
Out of scope: multi-region propagation delay — deferred, owner `platform`,
trigger is the first region beyond `eu-west`.

## Vocabulary

- **Withdraw** — an operator removes a published bundle from a channel. The
  bundle stays in storage and keeps its id.
- **Rollback** — a separate action that makes an older bundle current again.
  Not part of this slice.
- **Current bundle** — the bundle a channel's poll endpoint returns today.

## Decisions

- D1 — A withdrawn bundle keeps its bundle id forever and the id is never
  reused. Auditors cite ids in findings, and a reused id would silently change
  what a past finding referred to. Proposed as a Decision node: hard to reverse,
  real trade-off (the id space grows without bound and storage cannot be
  compacted by renumbering).
- D2 — The operator-facing verb is `withdraw`, not `unpublish`, because the
  product contract already says withdraw.
- D3 — Withdrawing leaves no current bundle on the channel. The channel is empty
  until someone publishes again. The alternative, silently promoting the previous
  bundle, was rejected because it is rollback wearing withdrawal's name.

## Open questions

- (resolved) What does the poll endpoint return for a channel with no current
  bundle? HTTP 404 with error code `no_current_bundle`. See D3.
- (resolved) Can a withdrawn bundle be published again? No. It keeps its id and
  stays withdrawn; publishing again means a new bundle.
- (delegated) The name of the internal status value for a withdrawn bundle.
  Inside implementation freedom.
- (deferred) Multi-region propagation delay. Owner `platform`; trigger is the
  first region beyond `eu-west`.
