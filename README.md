<div align="center">

<img src="assets/logo-combination.svg" alt="Pulse logo" width="420" />

# Pulse

<p><strong>A local truth layer for developers working with coding agents</strong></p>

<p>
  <img alt="Version" src="https://img.shields.io/badge/version-0.1.0-0F766E?style=flat-square" />
  <img alt="License" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" />
  <img alt="Runtime" src="https://img.shields.io/badge/runtime-Rust-8B5CF6?style=flat-square" />
</p>

</div>

---

## What Pulse is

Pulse is a local CLI for one developer using one or more coding agents in one
repository. It keeps the truth about what needs doing, hands each agent exactly
the context for one Ticket, runs any agent or script through a configured
command, refuses to close work without evidence, and turns failures into
better docs and checks.

Pulse does not run agents, does not run tests, and has no daemon. All state
lives in `.pulse/`, `works/` and `docs/` under Git.

The full product definition, target design and golden path are in
[`PRODUCT.md`](PRODUCT.md). This README describes what exists today.

## Status

Pre-release. The Rust core (work graph, packet, docs registry and search,
evidence receipts, close gates) is implemented and covered by an integration
suite. The runner, ratchet commands and event-log communication described in
`PRODUCT.md` are not implemented yet; the daemon runtime that previously
backed them was removed under
[Decision 0008](docs/decisions/0008-narrow-scope-to-truth-layer.md).

Nothing has yet run end to end on a real repository. The next milestone is the
seven-step golden path in `PRODUCT.md` §7 against `examples/todolist/`.

## What works today

| Area | Commands | Notes |
|---|---|---|
| Repository init | `pulse init` | Creates `.pulse/` planes and a default-deny authority policy |
| Work graph | `pulse work create\|show\|list\|edit\|transition\|supersede\|ready\|rollup\|packet` | Sharded JSON nodes/edges, CAS revisions, lifecycle gates |
| Graph queries | `pulse graph edge add\|validate\|export\|neighborhood\|affected-by` | Deterministic edge IDs, cycle checks |
| Docs | `pulse docs register\|list\|show\|applicable\|search\|get\|tree\|index\|validate` | Registry sidecar, path-scope applicability, section-level BM25 search |
| Evidence | `pulse evidence receipt record\|show\|verify`, `artifact put\|verify` | Immutable content-hashed receipts |
| QA baseline | `pulse qa baseline\|resolve` | Parses the `pulse-qa` block in `works/<story>/qa.md` |
| Knowledge | `pulse knowledge create\|show\|list\|edit\|validate` | Store and validation only; no promotion or recall yet |

Every command accepts `--json` and `--repo-root <path>`.

## Install

```bash
cargo install --path .
pulse --help
```

Initialise only an explicit target repository, never this repository's root:

```bash
pulse --repo-root <target-repo> init --json
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

Operating rules for agents and contributors are in [`AGENTS.md`](AGENTS.md).
Contribution workflow is in [`CONTRIBUTING.md`](CONTRIBUTING.md). Current
code architecture is in [`ARCHITECTURE.md`](ARCHITECTURE.md). Decisions
are under [`docs/decisions/`](docs/decisions/). Retired design material is
under [`design/archive/`](design/archive/).

<div align="center">

MIT License

</div>
