# Decision 0008: Narrow Pulse to a vendor-neutral truth layer

## Status

Accepted, 2026-09-05. One operating rule below was revised on 2026-09-06 by
`PRODUCT.md` §13 decision 2: the runner does **not** create a worktree when a
second Ticket runs concurrently. It refuses the run with
`run_isolation_required`, naming the Ticket holding the lease, and a worktree
is created only when `--isolation worktree` is passed explicitly. Worktree is
opt-in per command, never automatic. The code follows the revision.

## Context

By late August 2026 the Rust reboot had grown to roughly 50k lines of
production code and 22k lines of design documents without a single real
end-to-end run against a real repository. An audit found that the
architecture was disciplined but the scope was not: a host-local daemon
owning projects, workspaces, sessions, providers, processes and timelines; a
browser QA executor with environment lifecycle and deployment identity; a
Story qualification matrix with flaky waivers; a multi-principal
authorization model with one production principal; an MCP adapter bound to no
server; ten embedded JSON schemas never fed to a validator; eight independent
version counters. Ticket close was only implemented for `risk = low`. Getting
one Ticket to `ready` required roughly nine steps and forty fields.

Coding-agent runtimes (Claude Code, Codex, Aider, OpenCode) already own
session, process, permission and inter-agent messaging, and vendors iterate on
those surfaces faster than a one-person project can track. What they do not
own is durable work truth across sessions, proof-gated completion, bounded
repository context, and a loop that turns failures into better docs and
checks.

## Decision

Pulse is a **local CLI truth layer for a developer using coding agents in one
repository**. The full definition, feature set, golden path and code triage
live in [`PRODUCT.md`](../../PRODUCT.md), which supersedes the former
`pulse-reboot/` design set and `PULSE_REBOOT.md`.

Scope kept:

- Work graph (Epic/Story/Ticket/Decision, typed edges, lifecycle, lease, CAS).
- Packet: bounded context for one Ticket, fenced to a source commit.
- Runner: `pulse run <role> --ticket <id>` executes a command declared in
  `.pulse/config/runners.json`; any agent or script is a runner.
- Docs: registry, tags, applicability, freshness, section-level lexical search.
- Evidence gate: immutable receipts; close requires handoff, independent
  verification, and QA/docs receipts when posture requires them.
- Ratchet: learnings with provenance and applicability, promoted into docs,
  checks or Decisions, recalled into packets.
- Communication: append-only event log plus `pulse note`; no broker.
- CLI first; a thin MCP server after the CLI path is proven.

Scope removed:

- Daemon, session/process/provider management, timeline, transports.
- Pulse-owned browser QA, environment lifecycle, deployment identity, trace
  validation, Story qualification matrix, flaky waiver grants.
- Multi-principal authorization, communication grants, orchestration engine,
  deliberation, priority reconciliation receipts, persisted shaping maps.
- Decorative JSON schemas, per-document retrieval knobs, retrieval eval and
  benchmark harness, compatibility re-export shims.
- Windows as a tier-1 target.

Operating rules:

- `ticket.md` under `works/<id>/` is the Ticket contract; the node holds
  metadata and the file hash only.
- The runner works in the checkout by default; a worktree is created only
  when a second Ticket runs concurrently or when explicitly requested.
  *(Revised 2026-09-06 — see Status: a concurrent Ticket now makes the run
  refuse, and `--isolation worktree` is the only thing that creates one.)*
- Close is available for every risk level; `high` and `critical` add a human
  actor requirement rather than a missing code path.
- Dogfood target is `examples/todolist/` inside this repository, sharing its
  Git history. The `--repo-root .` prohibition for the Pulse root remains.
- No feature work until the seven-step golden path in `PRODUCT.md` runs for
  real.

## Consequences

- `pulse-reboot/` and `PULSE_REBOOT.md` are deleted; their content is either
  absorbed into `PRODUCT.md` or intentionally dropped. Historical proposals
  move to `design/archive/`.
- `src/daemon/`, `src/cli/daemon.rs`, the browser/environment parts of
  `src/qa/executor.rs`, story qualification and flaky waiver logic, embedded
  JSON schemas, and `src/graph/*.rs` shims are scheduled for removal. The
  assignment saga is retained under `design/archive/` as reference for the
  runner's lease and crash semantics.
- The command-execution contract in `src/qa/executor.rs` becomes the seed of
  a shared `runner/` module used by worker, reviewer, QA and check roles.
- README, AGENTS.md and CONTRIBUTING.md describe only what has code and
  tests; target design lives in `PRODUCT.md`.
- Decisions 0005 and 0006 remain as history of the daemon and peer-agent
  topology; they no longer describe a current or planned surface.
