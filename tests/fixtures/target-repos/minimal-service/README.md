# Minimal Token Service

A dependency-free target repository fixture used to exercise the Pulse harness.

The service classifies refresh-token failures into stable domain outcomes:

- expired token → `TokenExpired`;
- malformed, revoked, or unknown token → `InvalidToken`.

Run its repository verification profile with:

```bash
node scripts/verify.mjs
```

This tracked directory is a read-only template. Pulse tests copy it to a
temporary directory before bootstrapping or mutating `.pulse/` state.
