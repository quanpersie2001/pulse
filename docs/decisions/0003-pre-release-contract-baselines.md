# Decision 0003: Pre-release contract baselines

## Status

Accepted.

## Context

Pulse is still converging toward its initial Core v1 release. Earlier Phase 1
implementation proposals treated intermediate Slice state as if it were released
schema or payload history, which introduced predecessor models and migration
work for development data that Pulse had never promised to support.

Phase and Slice numbers describe implementation order. They are not product,
schema, payload, event, receipt, or API versions.

## Decision

Before the initial Core v1 release, each persisted or public contract family has
one current baseline, conventionally version `1` when a version field is needed.
When the design changes, Pulse updates that current baseline in place across its
owner docs, schemas, source, tests, and fixtures.

Internal development state, historical Slice bytes, and local fixtures are not
supported predecessor state. They are regenerated, updated to the current
baseline, or rejected as schema drift. Pulse does not add predecessor decoders,
migrate-on-load behavior, compatibility bridges, or migration commands solely
to preserve them.

A successor version or migration is eligible only after a real release or an
explicit support commitment for external durable state. That change requires a
separate accepted Decision defining compatibility scope, migration or explicit
no-support behavior, recovery, rollback, tests, and operator impact.

The accepted owner documents under `pulse-reboot/` define intended contracts.
Proposals describe implementation strategy; source and tests prove current
implementation. None of them may silently turn an internal Slice into a
compatibility boundary.

## Consequences

- Current pre-release schemas and payloads are amended directly.
- Historical proposals may retain obsolete implementation history, but an active
  proposal must be re-baselined before reuse.
- Core v1 does not carry migration code for internal development generations.
- Real post-release compatibility work remains explicit and evidence-backed.
