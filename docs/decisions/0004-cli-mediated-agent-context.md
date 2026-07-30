# Decision 0004: CLI-mediated Agent context and workflow bootstrap

## Status

Accepted.

## Context

Pulse already owns typed query surfaces for work packets, documentation and
applicable historical knowledge. Rendering the full Ticket contract, Story/Epic
context, Decisions, docs excerpts, QA cases and knowledge entries into every
Codex prompt duplicates those contracts, increases prompt size and weakens
progressive disclosure.

A Worker run still needs a small initial instruction because `codex exec` must
know how to load its assignment and which authority boundaries apply.

## Decision

Daemon session prompts are small, versioned, role-specific workflow bootstraps.
An implementation Worker bootstrap contains only assignment/session/Ticket/lease
identity, retrieval steps, hard workflow rules and authority boundaries.

The Worker loads the exact committed execution contract with:

```text
pulse work packet <ticket-id> --lease <lease-id> --json
```

The lease-bound form resolves the `WorkPacketV1` embedded in the committed
`PreparedAssignmentV1`. It validates Ticket, lease, assignee, workspace and
packet fingerprint and does not silently rebuild from a newer live Ticket
revision.

Required durable documentation is loaded through `pulse docs get`; applicable
historical learning is loaded through `pulse knowledge applicable` and explicit
`knowledge get` when detail is needed. The bootstrap prompt does not inline
those corpora. Pulse does not add a separate `pulse run context` command for the
same responsibility.

## Consequences

- A daemon session bootstrap is a control/identity wrapper, not a second Ticket
  contract.
- WorkPacket remains the machine-readable execution contract and context
  manifest.
- Prompt bytes remain bounded and stable across Tickets.
- Required docs/knowledge retain typed authority, hashes, applicability and
  retrieval budgets.
- Context drift is detected at the lease-bound query boundary.
- Reviewer and QA prompts follow the same bootstrap pattern with role-specific
  assignments and authority.
