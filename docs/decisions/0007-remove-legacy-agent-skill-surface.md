# Decision 0007: Remove the legacy agent skill surface

## Status

Accepted. Narrowed by [Decision 0009](0009-skill-surface-over-cli.md) on
2026-09-06: a guidance-only skill surface returns; the rule that no prose owns
lifecycle or state stands.

## Context

Pulse previously packaged a conversational workflow router and standalone
agent skills beside the Rust Core and daemon. That surface duplicated lifecycle
and authority guidance, advertised commands that did not consistently match the
Rust CLI, and created a second product contract outside typed Core/Daemon
boundaries.

The plugin manifests and generated distribution were already absent, while the
source `skills/` tree, public documentation, and architecture tests still
treated them as shipped authority. Keeping that partial surface made it unclear
whether repository state advanced through prose or through the Rust contracts.

## Decision

Pulse no longer owns or packages an agent skill/router surface.

- The Rust `pulse` executable is the only supported public product surface.
- Core owns repository semantics, readiness, reservations, evidence, and proof
  gates.
- Daemon owns host-local project, workspace, session, provider, process,
  timeline, effect, assignment, and recovery state.
- Agent judgment is expressed through typed inputs, outputs, receipts, and
  policy-controlled authority; prose guidance is not a competing runtime.
- The legacy `skills/`, generated `dist/`, and plugin manifests must remain
  absent.
- Target repositories may own their own `AGENTS.md`, scripts, hooks, checks,
  policies, and evals without becoming a Pulse package surface.

## Consequences

- README and contributing guidance describe source/binary usage rather than
  plugin installation or conversational router commands.
- Tests guard absence of the legacy package surface instead of validating its
  generated output.
- Reboot documents use Agent/reviewer guidance and typed contracts as the
  semantic-judgment boundary.
- Future Orchestration must compose Core and Runtime through typed protocols; it
  must not reintroduce a prose-owned lifecycle authority.
- Historical proposals may describe the migration context, but cannot restore
  the removed public contract.
