# Refresh-token failure contract

Clients receive stable domain outcomes rather than internal validation details.

## Outcomes

- An expired refresh token produces `TokenExpired` so a client may request a new login.
- A malformed, revoked, unknown, or signature-invalid token produces `InvalidToken`.

## Invariants

- Responses do not expose signature, key, storage, or revocation internals.
- Adding a new internal validation reason does not create a new public outcome without an accepted product decision.
