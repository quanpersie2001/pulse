# Glossary

Durable terms used by Pulse workflow, runtime, and workgraph documentation.

## Current-work contract

A bounded execution slice represented by approved graph/contracts and proven by current readiness evidence before implementation starts.

## Documentation impact

The mandatory `plan.md` section that records whether `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, `docs/decisions/`, and `docs/product/` require Create, Update, or No change actions.

## Plan artifact

The lowercase story-scoped `plan.md` that decomposes approved `solution-design.md` decisions into tasks, docs updates, validation mapping, and workgraph materialization posture.

## Product docs

Domain-focused product contract files under `docs/product/`, such as `overview.md`, `billing.md`, `workflows.md`, `permissions.md`, or `api-conventions.md`. They are created only when real product/domain truth exists.

## Workgraph materialization

Creating or updating canonical Pulse work items through the Rust `pulse work` and `pulse graph` commands after approval, instead of hand-editing graph storage.
