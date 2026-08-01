# Pulse harness reference

The harness is the combination of the `pulse:workflow` guidance surface,
human-authored work artifacts, Rust graph services, and the Rust daemon.

## Ownership

1. **Workflow guidance** selects a move, explains gates, and records decisions
   in the owning work artifacts.
2. **Rust graph services** own graph bootstrap, validation, node/edge reads, and
   graph mutations through the `pulse graph` and `pulse work` command families.
3. **The Rust daemon** owns project, workspace, session, process, reservation,
   and timeline runtime through `pulse daemon`, `pulse project`, `pulse
   workspace`, and `pulse session`.

The installed skill contains no mutable runtime implementation. It must not
write, repair, migrate, or mirror Pulse state.

## Supported repository graph

The Rust graph repository uses `.pulse/workgraph/` with `nodes/`, `edges/`,
`manifest.json`, and `schemas/`. Treat command output and committed work
artifacts as evidence. Do not hand-edit materialized graph data.

## Workflow boundary

`pulse:workflow <command>` is a manual guidance surface. It may recommend an
explicit Rust command after an approval gate, but it does not invoke hidden
bootstrap, onboarding, reservation, session-resume, or migration behavior.
