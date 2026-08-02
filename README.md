<div align="center">

<img src="assets/logo-combination.svg" alt="Pulse logo" width="420" />

# Pulse

<p><strong>A local-first harness for understandable, verifiable agent delivery</strong></p>

<p>
  <a href=".codex-plugin/plugin.json">
    <img alt="Version" src="https://img.shields.io/badge/version-3.5.3-0F766E?style=flat-square" />
  </a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" />
  <a href="skills/workflow">
    <img alt="Router" src="https://img.shields.io/badge/router-pulse%3Aworkflow-8B5CF6?style=flat-square" />
  </a>
</p>

<p><em>Keep agents aligned to approved scope, verified execution, and auditable outcomes.</em></p>

</div>

---

## What is Pulse?

Pulse is a local-first harness engineering system. It combines a workflow
router, durable repository knowledge, a local work graph, evidence and review
loops, and executable repository capabilities. The public workflow skill is
**`pulse:workflow`**; its subcommands guide use, intake, exploration, design,
planning, validation, execution, review, and compounding. Standalone utility
skills remain packaged separately for focused non-router tasks.

## Architecture

Pulse is one product, executable, and release unit:

```text
Pulse product
├── local `pulse` executable
│   ├── Core — work graph/contracts, docs/knowledge/policy, reservations, evidence and proof gates
│   ├── Daemon Runtime — host-local projects, workspaces, sessions, providers and timeline
│   └── future Orchestration — composes Core and Runtime; not implemented
└── repository harness assets — stateless skills, scripts, hooks and evals
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

1. `pulse:workflow use` guides the operator through runtime and graph readiness; it does not perform the operation.
2. `pulse:workflow explore` locks decisions in feature context artifacts.
3. `pulse:workflow plan` selects shape and execution contract.
4. `pulse:workflow validate` proves feasibility before implementation.
5. `pulse:workflow swarm` or `pulse:workflow execute` delivers approved work.
6. `pulse:workflow review` enforces merge quality gates.
7. `pulse:workflow compound` captures reusable learnings.

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

### Claude Code

```bash
/plugin marketplace add quanpersie2001/pulse
/plugin install pulse@pulse
```

### Codex

```bash
codex plugin marketplace add quanpersie2001/pulse
```

Codex reads the marketplace name from [`.agents/plugins/marketplace.json`](.agents/plugins/marketplace.json), so the installed plugin key is `pulse@pulse-dev`.

### After Install

Start with **`pulse:workflow use`** in the target repo as guidance. The installed skill does not initialize state; with operator approval, use the existing Rust commands `pulse graph bootstrap --repo-root <repo> --json` and `pulse daemon start` for those operations.

Operational commands require the Rust `pulse` CLI to be available on the
target environment's `PATH`. The plugin does not install or package that
binary; binary installation and distribution remain an unresolved product
decision.

## Project Docs

| Read this when you want... | Link |
| --- | --- |
| The product direction and architecture | [PULSE_REBOOT.md](PULSE_REBOOT.md) |
| The detailed design ownership map | [pulse-reboot/README.md](pulse-reboot/README.md) |

## Maintainer Notes

When public docs or `pulse:workflow` router metadata change:

```bash
bash scripts/check-markdown-links.sh
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for skill structure, versioning, and PR process.

<div align="center">

MIT License

</div>
