# Repository Intent

- Preserve backward-compatible refresh-token outcome names.
- Do not expose sensitive token-validation details.
- Prefer deterministic, dependency-free verification.

# Human Judgment Boundaries

- Human approval is required to rename public outcomes or weaken the no-secret-leak invariant.
- Agents may change internal helper structure and focused tests inside an approved Ticket contract.

# Verification Profiles

- `service-change`: `node scripts/verify.mjs`
- `docs-only`: inspect changed Markdown links and terminology
