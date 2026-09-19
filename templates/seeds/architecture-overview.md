# Architecture overview

The map of how this system is built. Fill each section from the code, keep
it true as the code moves, and split a section into its own
`docs/architecture/<area>.md` (with file-level `applies_to` frontmatter)
once it outgrows a screen — this file then links to it. Product behavior
belongs in `docs/product/`, the reasons behind a choice in
`docs/decisions/`; link to them rather than restating.

## System context

What this system is for in one paragraph, who and what talks to it (users,
other services, third parties), and what it deliberately does not do.

## Components

One row per deployable or top-level module. "May depend on" is the rule a
reviewer enforces, not a description of today's imports.

| Component | Responsibility | Lives in | May depend on |
|---|---|---|---|
| | | | |

A diagram of the components and the calls between them goes here
(a `mermaid` block is fine).

## Key flows

The two or three requests that matter most, end to end: entry point, each
component crossed, what is read and written, where it can fail. One
numbered list or sequence diagram per flow.

## Data model

The persistent entities, their relations and who owns writes to each.
Where the schema is defined and how it migrates.

## Conventions

Cross-cutting rules every change follows: error shape, validation,
auth, logging, configuration, state management, naming. State the rule and
why it exists; link the decision when there is one.

## Invariants

What must stay true for the system to be correct, and what breaks first
when a newcomer ignores it. Prefer an invariant a test or a check enforces,
and name that test.

## Where things go

A new endpoint, model, screen, migration, job — the directory and the
existing file to copy the pattern from.
