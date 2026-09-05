# Repository Intent

- Keep the todo domain module pure and dependency-free.
- Public outcome names are a compatibility contract; changing them needs
  human approval.
- Verification is deterministic: `node scripts/verify.mjs` must pass before
  any handoff claim.

# Human Judgment Boundaries

- Human approval is required to rename public outcomes or change the state
  file format in a way that loses user data.
- Agents may change internal helper structure and focused tests inside an
  approved Ticket contract.

# Verification Profiles

- `module-change`: `node scripts/verify.mjs`
- `docs-only`: inspect changed Markdown links and terminology
