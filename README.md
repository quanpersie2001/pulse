<div align="center">

<img src="assets/logo-combination.svg" alt="Pulse logo" width="420" />

# Pulse

<p><strong>A local-first harness for understandable, verifiable agent delivery</strong></p>

<p>
  <img alt="Version" src="https://img.shields.io/badge/version-0.1.0-0F766E?style=flat-square" />
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" />
  <img alt="Runtime" src="https://img.shields.io/badge/runtime-Rust-8B5CF6?style=flat-square" />
</p>

<p><em>Keep agents aligned to approved scope, verified execution, and auditable outcomes.</em></p>

</div>

---

## What is Pulse?

Pulse is a local-first harness engineering system. It combines durable
repository knowledge, a local work graph, evidence and review loops, and a
host-local daemon for project, workspace, session, provider, and process
lifecycle. Its supported public surface is the Rust `pulse` executable; the
repository no longer packages an agent workflow router or standalone skills.

## Architecture

Pulse is one product, executable, and release unit:

```text
Pulse product
├── local `pulse` executable
│   ├── Core — work graph/contracts, docs/knowledge/policy, reservations, evidence and proof gates
│   ├── Daemon Runtime — host-local projects, workspaces, sessions, providers and timeline
│   └── future Orchestration — composes Core and Runtime; not implemented
└── target-repository harness — docs, policies, scripts, hooks, checks and evals
```

Core is independently usable for repository work. The Daemon Runtime owns
host-local lifecycle and external process effects. Future Orchestration may
coordinate independent runtime sessions, but has no semantic authority. A
single writer owns each mutable authority, and proof—not liveness—advances
repository meaning: a process exit, provider idle state, or delivered message
does not by itself complete work.

### Daemon application boundary

The runtime path is deliberately one facade and one persistence authority:

```text
CLI / local protocol / MCP transports
        -> one DaemonApplication facade
        -> private flat use-case modules
        -> one StateStore + ProcessOwner + provider registry
```

The facade and module tree are rooted at [`src/daemon/application/mod.rs`](src/daemon/application/mod.rs); transport envelopes and requests live in [`src/daemon/protocol/mod.rs`](src/daemon/protocol/mod.rs), durable runtime state in [`src/daemon/persistence/mod.rs`](src/daemon/persistence/mod.rs), and host process ownership in [`src/daemon/process/mod.rs`](src/daemon/process/mod.rs). The private application tree owns project, workspace, session, turn, communication, timeline, assignment, recovery, dispatch, and effect mechanics. Daemon may call typed public Core reservation/proof gates; Core never imports daemon. The detailed record is [`proposals/daemon-application-decomposition.md`](proposals/daemon-application-decomposition.md), with [Decision 0005](docs/decisions/0005-rust-daemon-runtime-control-plane.md) and [Decision 0006](docs/decisions/0006-peer-agent-assurance-topology.md) as governing context.

To add a daemon use case: define the protocol request, add authorization and
routing in dispatch, place the behavior in one cohesive private owner, keep
state mutation and its timeline event atomic, add focused contract coverage
under the single [`tests/daemon.rs`](tests/daemon.rs) crate, and extend the
explicit architecture inventory. This is internal Daemon decomposition, not
Orchestration; future Orchestration remains unimplemented. Core/kernel/graph
ownership remains documented by [`src/kernel/`](src/kernel/) and
[`src/graph/`](src/graph/).

## The Delivery Chain

1. Core reads the graph, contracts, applicable docs, evidence, policy, and source state.
2. `pulse work packet` builds a revision- and source-fenced execution packet.
3. `pulse session assign` reserves approved work and durably delivers its bootstrap to a provider session.
4. A bound acknowledgement activates the reserved Ticket.
5. The worker implements and verifies the lease-bound packet in its workspace.
6. Typed handoff moves the Ticket to independent verification.
7. Verification maps every acceptance item to passing checks/evidence; Core may close a low-risk Ticket whose documentation posture is `none` and whose QA posture is either `none` or satisfied by a required checkpoint.
8. Required Ticket QA closes only with a current passed `qa_checkpoint` receipt covering the exact affected Story baseline cases. Story-deferred QA, documentation promotion, and medium-or-higher risk remain fail-closed until their dedicated assurance resolvers are installed.

`pulse session verify` reads checks from `--checks` and the acceptance map from
`--acceptance`. The acceptance file is a JSON array of
`{"acceptance_id","check_names","evidence_receipt_ids"}` objects and must cover
the exact current contract IDs. After a passed verification, an authorized
reviewer invokes `pulse session close-assignment <saga-id> --actor ...
--source-commit ... --summary ...`; Core revalidates every binding before
writing the close receipt and `done` transition.

`pulse qa baseline <story-id>` parses and validates the canonical Story
baseline. `pulse qa resolve <ticket-id>` resolves a required Ticket impact to
exact current case revisions. After handoff and before verification,
`pulse session qa-checkpoint <saga-id> --actor ... --source-commit ...
--executor <id>` runs the repository-allowlisted structured executor declared
at `.pulse/qa/executors/<id>.json`. The daemon owns its bounded process group,
passes a typed input-file path as the final argument, validates typed JSON on
stdout, ingests declared artifacts, and records the immutable checkpoint
receipt automatically. The returned receipt ID must be referenced by the
verification acceptance map; QA is never a mutable status field.

An executor may additionally declare fixed repository-relative `start`,
`healthcheck`, `reset`, and `cleanup` commands. Every lifecycle command receives
the same typed input path and must return one structured environment identity
matching the candidate commit and fixture revision. Pulse skips execution when
preparation fails, always attempts declared cleanup after a known start outcome,
and emits payload version 2; a failed cleanup can never produce a passed receipt.

### The 4 Human Gates

| Gate | What it blocks |
| --- | --- |
| **Gate 1** | Planning before decisions are locked |
| **Gate 2** | Execution prep before shape approval |
| **Gate 3** | Execution before validated current work approval |
| **Gate 4** | Merge before review completion |

## Why use Pulse

| Problem | Pulse response |
| --- | --- |
| Requirements drift in chat | Lock decisions in context artifacts |
| Plans are plausible but brittle | Validate before execution |
| Parallel workers collide | Coordinate through Rust daemon session/lease operations |
| Work is hard to audit later | Preserve artifacts, evidence, and review trail |

## Installation

Pulse currently ships from source:

```bash
cargo install --path .
pulse --help
```

Initialize only an explicit target repository, never this development
repository:

```bash
pulse graph bootstrap --repo-root <target-repo> --json
pulse daemon start
```

Prebuilt binary distribution remains an unresolved product decision.

## Project Docs

| Read this when you want... | Link |
| --- | --- |
| The product direction and architecture | [PULSE_REBOOT.md](PULSE_REBOOT.md) |
| The detailed design ownership map | [pulse-reboot/README.md](pulse-reboot/README.md) |

## Maintainer Notes

Before handing back a change:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for source ownership, validation, and PR process.

<div align="center">

MIT License

</div>
