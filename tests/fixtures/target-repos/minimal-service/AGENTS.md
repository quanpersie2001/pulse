# Agent Guidance

## Repository purpose

This is a minimal service fixture for testing the Pulse harness.

## Source map

- `src/token.mjs` owns refresh-token failure classification.
- `test/token.test.mjs` owns executable behavior examples.
- `docs/product/authentication.md` owns the public behavior contract.
- `docs/architecture/overview.md` owns the module boundary.
- `scripts/verify.mjs` is the deterministic verification entry point.

## Constraints

- Preserve the stable `TokenExpired` and `InvalidToken` outcome names.
- Do not add third-party runtime or test dependencies.
- Update product documentation when public classification behavior changes.
- Run `node scripts/verify.mjs` before claiming implementation success.
