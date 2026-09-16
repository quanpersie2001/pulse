# Offline Release Hub

## Requirement Overview

### Destination
Regulated teams can prepare, verify and withdraw an offline software release using a repository-local workflow, with enough evidence for an auditor to reproduce the release decision.

### Context
The destination is confirmed and this map is approved. Publication authority was settled in DEC-004: a publish above the configured risk level requires a second administrator's approval.

### In scope
- Assemble a release from approved artifacts.
- Verify signatures and retain verification evidence.
- Withdraw a release that has already reached clients.

### Out of scope
- Hosted multi-tenant operation in the first release.

### Glossary
- `release bundle` — immutable artifacts plus the manifest and verification evidence.
- `hub` — the repository-local service that stores published bundles.

## Business Rules

- BR-01: A release bundle is immutable after verification begins.
- BR-02: Every accepted artifact records the digest that was verified.
- BR-04: A publish above the configured risk level requires a second administrator's approval. See DEC-004.
- BR-07: A withdrawn release remains retrievable as evidence for seven years.

## Exception Scenarios

- E-01
  - Trigger: An artifact digest differs from its manifest entry.
  - User-visible outcome: The release is rejected and names the mismatched artifact.
  - Error code: `artifact_digest_mismatch`
- E-03
  - Trigger: A client polls for a release that was withdrawn while the client was offline.
  - User-visible outcome: The client is told the release was withdrawn and is offered the version preceding it.
  - Error code: `release_withdrawn`

## Open Questions

### Open
- Enterprise policy support may matter later, but its actors, policy source, and required behavior are not yet clear enough to phrase as one question.

### Decisions so far
- DEC-004 — Publishes above the configured risk level require a second administrator's approval.
- Local-first first release — hosted multi-tenant operation is outside this destination.

### Ruled out
- Hosted control plane — excluded from the first-release destination.

## Implementation notes

The hub stores bundles under `var/hub/bundles/<bundle-id>/` and serves a poll
endpoint at `GET /v1/channel/<channel>/current`, which returns the manifest of
the bundle currently marked current for that channel. Clients poll on a fixed
fifteen-minute interval; there is no push channel. Withdrawal is not
implemented: nothing today clears or repoints the `current` marker.
