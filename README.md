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

> **v3 đã ship (tag `v0.0.1` — bản rebuild khởi động lại vạch phiên bản).**
> README này mô tả code hiện tại; [`ARCHITECTURE.md`](ARCHITECTURE.md) mô tả
> cây source, [`AGENTS.md`](AGENTS.md) là hợp đồng vận hành, và plan
> [`docs/plans/0022-thin-harness.md`](docs/plans/0022-thin-harness.md) /
> [`docs/plans/0025-parallel-verified-learning.md`](docs/plans/0025-parallel-verified-learning.md)
> giữ phần suy luận. Nội dung v2 dưới đây chỉ còn giá trị lịch sử cho phần
> không bị thay thế.

---

## What Pulse is

Pulse is a local CLI truth layer for a developer using coding agents in one
repository: a JSONL store of Epic/Story/Ticket/Decision records, gates that
bracket the work (`claim`/`handoff` for a worker, `lane input`/`lane seal`
for a review or qa lane), an evidence gate, and an append-only event log.
It dispatches nothing: the host spawns the agents, Pulse decides what
counts. It does not run agents and has no daemon; the only commands it
executes are the argv a record declares (`pulse verify <id>` runs the
Ticket's `verify[]`), and it records what it observed. All state lives in
`.pulse/` and `docs/` under Git.

## What works today

| Area | Commands | Notes |
|---|---|---|
| Repository init | `pulse init` / `--refresh` | Seeds `.pulse/`, `PULSE.md`, `AGENTS.md` block, prompts, docs seeds; `--refresh` three-way merges template changes (files you edited are never overwritten) |
| Work graph | `pulse work new\|show\|list\|tree\|ready\|update\|dep\|transition` | One JSONL store, JSON-schema validated, append-only events |
| Parallel work | `pulse frontier`, `pulse claim`, `pulse reserve`, `pulse release` | Tickets declare `touches`; disjoint tickets claim side by side in one checkout; a host hook (`pulse hook snippet claude`) makes reservations bind at edit time |
| Evidence | `pulse packet`, `pulse checkpoint`, `pulse handoff`, `pulse verify`, `pulse close`, `pulse close-story` | Handoff reads observed receipts, not self-reported exit codes; `verify` runs exactly the declared argv and records it |
| Review lanes | `pulse lane input\|seal\|reconcile` | Blind seats and finding reconciliation for opt-in panels; a finding's `check.argv` is run by Pulse and beats votes |
| Learning loop | `pulse learn add\|activate\|dismiss\|friction\|applicable`, `pulse metrics` | Friction surfaces at close, active learnings' checks run inside `pulse verify` |
| Docs | `pulse docs applicable\|check` | Broken links, stale generated sections, advisory docs-maybe-stale per ticket |
| Health | `pulse doctor`, `pulse events tail` | Torn store, expired leases, unsealed lanes; follow the event log |

Every command accepts `--json` and the global `--repo-root <path>`.

## Install

```bash
cargo install --path .
pulse --help
```

Enroll an explicit target repository (never this repository's root if you
are developing Pulse):

```bash
cd <target-repo>
pulse init
pulse hook snippet claude   # paste the printed PreToolUse config into .claude/settings.json
```

Then a minimal Ticket path is:

```bash
pulse work new ticket "Fix token errors" --risk low --surface api
pulse work ready <id>
pulse claim <id> --actor agent:worker-1
pulse packet <id>            # the one input the worker reads
# ... edit, checkpoint, verify ...
pulse verify <id> --actor agent:worker-1
pulse handoff <id> --from handoff.json --actor agent:worker-1
pulse lane input <id> review-correctness
pulse close <id>
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
are under [`docs/decisions/`](docs/decisions/).

<div align="center">

MIT License

</div>
