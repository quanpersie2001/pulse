# Offline Release Hub

## Requirement Overview

### Destination
Regulated teams can prepare and verify an offline software release using a repository-local workflow, with enough evidence for an auditor to reproduce the release decision.

### Context
The destination is confirmed. This map is still draft because publication authority and platform integration remain unresolved.

### In scope
- Assemble a release from approved artifacts.
- Verify signatures and retain verification evidence.
- Recover safely from an interrupted local release.

### Out of scope
- Hosted multi-tenant operation in the first release.

### Glossary
- `release bundle` — immutable artifacts plus the manifest and verification evidence.

## Business Rules

- BR-01: A release bundle is immutable after verification begins.
- BR-02: Every accepted artifact records the digest that was verified.

## Exception Scenarios

- E-01
  - Trigger: An artifact digest differs from its manifest entry.
  - User-visible outcome: The release is rejected and names the mismatched artifact.
  - Error code: `artifact_digest_mismatch`

## Open Questions

### Open
- (blocking; owner: human:product; type: grilling) Must every publish action require a second administrator's approval, or only releases above a configured risk level?
- (blocking; owner: team:platform; type: research) Which native keychain APIs can store signing-key references without exporting key material on macOS, Windows, and Linux?
- Enterprise policy support may matter later, but its actors, policy source, and required behavior are not yet clear enough to phrase as one question.

### Decisions so far
- Local-first first release — hosted multi-tenant operation is outside this destination.

### Ruled out
- Hosted control plane — excluded from the first-release destination.
