# Withdraw a published release

Slug: withdraw-release
Draft: no node exists yet; `pulse-planning` adopts this file into `works/<story-id>/`.

## Outcome

An operator can withdraw a release that has already been published to a channel,
and clients stop being offered it.

## Success signals

- A withdrawn release is no longer served as current for its channel.
- Rollback is reliable.

## Scope boundary

In scope:

- Withdrawing a bundle that is currently marked current for a channel.

Out of scope:

- Multi-region withdrawal.

## Vocabulary

- `withdraw` — removing a published release from the hub.
- `rollback` — returning clients to the version preceding the withdrawn one.

## Decisions

- D1 — Who may withdraw a release? — The same administrators who may publish it,
  under the BR-04 approval rule. — Withdrawal has the same blast radius as a
  publish, so a weaker authority rule would route around BR-04.
  - Decision node: no, reason kept here.
- D2 — Does a withdrawal delete the bundle? — No, it is marked withdrawn and
  stays retrievable. — BR-07 requires seven-year evidence retention.
  - Decision node: no, reason kept here.

## Open questions

- (resolved) Does a withdrawal need its own approval step? — Yes, the BR-04 rule
  applies unchanged. See D1.
- (delegated) Should a withdrawal also restore the previous manifest to clients,
  or only stop serving the withdrawn one?
- (deferred) Multi-region withdrawal.
- (blocking) When a client is offline for longer than the retention of the
  preceding version, what is it offered on its next poll? This changes what E-03
  promises the user.
