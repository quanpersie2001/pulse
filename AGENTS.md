# AGENTS.md — Pulse Reboot Operator Contract

Read this file at every session start. Re-read after context compaction.

## What Pulse Is Now

Pulse is a **local-first harness engineering system** for making a repository understandable, executable, verifiable and improvable by coding agents.

Pulse combines:

1. a local work graph for Epics, Stories, Tickets, Decisions and their relations;
2. durable documentation knowledge with ownership, applicability, authority and validation;
3. repository harness capabilities: scripts, tools, hooks, skills, policies and evals;
4. evidence loops for verification, review, QA and receipts;
5. knowledge compounding loops that promote proven learnings into docs, decisions, checks, skills or evals;
6. optional peer-agent orchestration once single-agent reliability is proven.

Pulse is **not** Jira-lite, a fixed phase workflow, a cloud-first service, or a general-purpose agent framework.

Primary design source: [`PULSE_REBOOT.md`](PULSE_REBOOT.md).
Detailed owners live under [`pulse-reboot/`](pulse-reboot/).

## Core Principle

The repository is the system of record. Important state, decisions, evidence and durable knowledge must be local, inspectable and recoverable.

Message history is not source-of-truth. Runtime state is not durable docs truth. Work prose is not a substitute for evidence or documentation receipts.

## Architecture Map

```text
Human / Agent
  -> Pulse CLI / kernel
  -> Local Work Graph + Documentation System + Evidence Store
  -> Repository Harness + Source Repository
  -> Verify / Review / QA / Compound Learnings
```

Key planes:

1. **Work graph** — `.pulse/workgraph/nodes/*.json`, `.pulse/workgraph/edges/*.json`, projections/cache.
2. **Work content** — `works/` prose/artifacts owned by Epics, Stories, Tickets and Decisions.
3. **Documentation knowledge** — `docs/`, `AGENTS.md`, future `PULSE.md`, registry and generated navigation.
4. **Evidence** — `.pulse/evidence/` receipts, artifacts and validation bindings.
5. **Runtime** — `.pulse/runtime/` locks, transactions, cache and ephemeral coordination state.
6. **Repository harness** — scripts, tests, skills, policies, evals and verification profiles.

## Current Kernel / CLI Surface

Use the local Rust CLI/kernel, not the legacy `pulse:workflow` skill router.

Common work graph commands:

```bash
pulse --repo-root <repo> work create --kind ticket --title "..." --json
pulse --repo-root <repo> work show <id> --json
pulse --repo-root <repo> work list --json
pulse --repo-root <repo> work edit <id> --expected-revision <n> ... --json
pulse --repo-root <repo> work transition <id> --to <status> --expected-revision <n> --actor <actor> --json
pulse --repo-root <repo> work supersede <old-id> --by <new-id> --expected-revision <n> --reason "..." --reconciliation-receipt <receipt-id> --actor <actor> --json
pulse --repo-root <repo> work executability <id> --json
pulse --repo-root <repo> work rollup <id> --json
pulse --repo-root <repo> graph export --json
pulse --repo-root <repo> graph validate --json
pulse --repo-root <repo> graph recover --json
pulse --repo-root <repo> graph neighborhood <id> --json
pulse --repo-root <repo> graph affected-by <id> --json
```

Evidence commands:

```bash
pulse --repo-root <repo> evidence artifact put <path> --json
pulse --repo-root <repo> evidence artifact verify <hash> --json
pulse --repo-root <repo> evidence receipt record <receipt-file> --json
pulse --repo-root <repo> evidence receipt verify <receipt-id> --json
pulse --repo-root <repo> evidence receipt show <receipt-id> --json
```

Documentation commands:

```bash
pulse --repo-root <repo> docs register ... --json
pulse --repo-root <repo> docs edit <doc-id> ... --json
pulse --repo-root <repo> docs retire <doc-id> ... --json
pulse --repo-root <repo> docs supersede <old-doc-id> <new-doc-id> ... --json
pulse --repo-root <repo> docs list --json
pulse --repo-root <repo> docs show <doc-id> --json
pulse --repo-root <repo> docs validate --json
pulse --repo-root <repo> docs applicable --work <work-id> --json
pulse --repo-root <repo> docs impact <ticket-id> --expected-revision <n> --posture <required|none|deferred> ... --json
pulse --repo-root <repo> docs index --json
pulse --repo-root <repo> docs index --check --json
pulse --repo-root <repo> docs status --json
pulse --repo-root <repo> docs search "query" [--work <work-id>] [--limit <n>] [--json]
pulse --repo-root <repo> docs get <doc-id|section-ref|chunk-ref|path:start-end> --json
pulse --repo-root <repo> docs tree [path] --json
```

Prefer `--json` for agent consumption. Treat CLI error codes as the stable contract.

## Work Graph Rules

- Ticket is the executable unit.
- Epic/Story hold durable outcome, behavior baseline and design/approach context.
- Decisions capture accepted hard-to-reverse choices.
- Hierarchy is not dependency. Use edges/dependencies explicitly.
- Priority is a signal, not an absolute sorting law.
- All mutations use CAS via `expected_revision` and emit immutable events.
- Never manually edit canonical graph JSON unless doing deliberate repair with tests/recovery context.
- Run `graph validate` after broad graph mutations.

## Documentation Rules

Durable repository knowledge belongs in:

- `docs/`
- `AGENTS.md`
- future `PULSE.md`
- accepted Decisions / work artifacts when they own the knowledge

Documentation registry controls identity, lifecycle, authority, scope and retrieval policy.

Rules:

1. Public behavior, invariant, architecture or operator procedure changes must update, classify or defer docs through `docs impact`.
2. Generated `_index.md` navigation files are derived; do not hand-edit generated Pulse markers.
3. Section retrieval uses `docs index/search/get/tree`; do not bypass it by raw-scanning the entire docs tree unless debugging the retrieval system itself.
4. `AGENTS.md` is operator guidance. It must not become a long design doc; link to owning reboot docs.
5. Treat stale/retired/superseded docs as historical, not current truth.

Relevant design owners:

- [`pulse-reboot/10-documentation-system.md`](pulse-reboot/10-documentation-system.md)
- [`pulse-reboot/11-documentation-retrieval.md`](pulse-reboot/11-documentation-retrieval.md)

## Evidence And Verification Rules

- Verification claims should be backed by receipts, test output or committed evidence.
- Receipt validation has integrity, binding, registry/policy and authorization dimensions.
- Authorization may remain explicitly unresolved; do not turn structural checks into human approval.
- Supersession through the CLI is receipt-first: use `--reconciliation-receipt`, not inline `--assertion`.
- Source/content-bound receipts are the durable proof boundary for documentation and review claims.

Relevant owner: [`pulse-reboot/07-verification-ratchet.md`](pulse-reboot/07-verification-ratchet.md).

## Shaping / Readiness Discipline

Pulse is not a fixed phase workflow, but it requires readiness discipline:

- Ground before asking: read work context, applicable docs, decisions, code and evidence.
- Ask humans only across authority boundaries: intent, irreversible trade-offs, risk appetite and approval.
- Turn sharp uncertainty into Decision, Discovery/Spike or enabling Ticket.
- Keep bounded fog in `not_yet_specified`; do not invent speculative tickets upfront.
- If execution discovers critical ambiguity outside implementation freedom, stop and re-shape/requeue.

Owner: [`pulse-reboot/04-runtime-harness.md`](pulse-reboot/04-runtime-harness.md).

## Agent Operating Rules

1. Start by orienting from repository artifacts, not conversation memory.
2. Use the Pulse CLI/kernel for canonical reads and mutations.
3. Respect CAS revisions; on conflict, reload current state before retrying.
4. Keep runtime transactions recoverable; if interrupted, run/read `graph recover` before continuing graph work.
5. Prefer small, evidence-backed commits.
6. Do not mark work complete unless tests/verification prove the affected behavior.
7. Use sub-agents/isolated worktrees for parallelizable investigation or implementation, then verify and merge deliberately.
8. Do not let sub-agent summaries substitute for checking actual diffs and tests.
9. Keep generated/cache/runtime outputs out of durable source unless explicitly tracked by design.
10. When context exceeds about 65%, write a handoff with branch, commits, tests run, open blockers and next action.

## Repository Layout Quick Reference

```text
.pulse/workgraph/nodes/          canonical work nodes
.pulse/workgraph/edges/          canonical graph edges
.pulse/runtime/                  locks, transactions, ephemeral runtime state
.pulse/evidence/                 receipts, artifacts and evidence manifest
.pulse/docs/                     docs registry, schemas and retrieval eval fixtures
.pulse/cache/                    disposable generated caches
works/                           human-facing work prose and artifacts
docs/                            durable repository documentation
pulse-reboot/                    reboot design owner documents
proposals/                       implementation slice proposals
src/                             Rust Pulse kernel/CLI implementation
tests/                           integration and contract tests
```

## Validation Commands For This Repo

Before claiming implementation work is done, run:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet
cargo test --all-targets
```

For docs retrieval changes, also consider targeted suites:

```bash
cargo test --test docs_search_get_tree --test docs_index --test docs_retrieval_eval
```

For evidence/receipt changes:

```bash
cargo test --test evidence_receipts --test docs_receipt_registry --test crash_recovery_process
```

For graph/lifecycle changes:

```bash
cargo test --test lifecycle --test workgraph --test workgraph_transaction --test workgraph_read_models
```

## Session Completion

Before ending a substantial work chunk:

1. Ensure working tree state is intentional.
2. Run the relevant validation commands and record them in the final response.
3. Commit coherent changes.
4. Note branch, commits, remaining risks and next action.
5. If using Pulse work items, update their status or leave a clear handoff.

## Optional Memory Tools

CASS (`cass`) and cass-memory (`cm`) are optional recall accelerators. Treat current repo artifacts, events and receipts as source-of-truth when discrepancies appear.
