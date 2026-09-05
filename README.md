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

v0.1.0. The golden path from `PRODUCT.md` §7 has run for real on
`examples/todolist/`: two Tickets were closed on evidence receipts by real
worker/reviewer agents, with kill-and-resume, QA checkpoints, docs validation
and a captured learning; see `examples/todolist/works/ST-001/` for the run
log.

Remaining pre-1.0 work (learning promotion ergonomics, artifact ingest,
reviewer outcome classification, story close on the dogfood target) is tracked
in `PRODUCT.md` §8.

## What works today

| Area | Commands | Notes |
|---|---|---|
| Repository init | `pulse init --actor kind:id` | Creates `.pulse/` planes, tags vocabulary, and a default-deny policy with Core grants |
| Work graph | `pulse work create\|show\|list\|edit\|sync\|transition\|close\|supersede\|ready\|rollup\|packet` | Sharded JSON nodes/edges, CAS revisions, markdown Ticket contracts, lifecycle gates |
| Graph queries | `pulse graph edge add\|validate\|export\|neighborhood\|affected-by` | Deterministic edge IDs, cycle checks |
| Docs | `pulse docs register\|tags\|list\|show\|applicable\|search\|get\|tree\|index\|validate` | Eight-field registry, controlled tags, path/tag applicability, section-level search |
| Evidence | `pulse evidence receipt record\|show\|verify`, `artifact put\|verify` | Immutable content-hashed receipts |
| QA baseline | `pulse qa baseline\|resolve` | Parses the `pulse-qa` block in `works/<story>/qa.md` |
| Knowledge | `pulse knowledge create\|capture\|show\|list\|edit\|validate\|promote\|applicable` | Capture from a run, validate against evidence, promote into docs, applicability buckets; packet injects required/recommended learnings |
| Runner | `pulse run <role> --ticket <id>` | Lease, bootstrap prompt, configured command, output classification, inconclusive receipts, worktree isolation, resume after kill |
| Communication | `pulse events tail`, `pulse note` | Append-only event log with `--since`/`--ticket`/`--follow`; ticket-targeted notes surface in packets |

Every command accepts `--json` and `--repo-root <path>`. A Ticket is created with a generated `works/<id>/ticket.md`; edit that file and run `work sync` before transitioning it.

## Install

```bash
cargo install --path .
pulse --help
```

Initialise only an explicit target repository, never this repository's root:

```bash
pulse --repo-root <target-repo> init --actor human:<name> --json
```

A minimal Ticket path is:

```bash
pulse --repo-root <target-repo> work create --kind ticket --title "Fix token errors" --risk low --json
# edit works/TK-001/ticket.md
pulse --repo-root <target-repo> work sync TK-001 --expected-revision 1 --actor human:<name>
pulse --repo-root <target-repo> work transition TK-001 --to shaped --expected-revision 2 --actor human:<name>
pulse --repo-root <target-repo> work transition TK-001 --to ready --expected-revision 3 --actor human:<name>
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
