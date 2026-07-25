# Architecture overview

`src/token.mjs` is a small domain boundary that maps internal refresh-token
failure reasons to public outcomes. Transport adapters should consume these
outcomes rather than duplicate classification rules.

The fixture intentionally has no third-party dependencies so Pulse integration
tests can execute it offline and deterministically.
