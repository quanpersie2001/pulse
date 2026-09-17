# Pulse — Architecture (v3, current code)

> Status: rewritten at v3.0.0 (P3.4). Describes the **code that exists
> today**; `SPEC.md` says what the product is, this file says how the code
> is shaped. Layer rules are guard-tested:
> `tests/architecture_guards.rs` (source-tree scan) and
> `tests/public_api_contract.rs` (stable public paths).

## 1. Shape

One Rust library plus a thin CLI binary. No daemon, no agent runtime, no
network service — the one exception is `pulse serve`, a local read-only
HTTP server (Decision 0023) that binds `127.0.0.1` only. Mutations take a
repository-scoped write lock and end with an atomic store write plus one
appended event.

## 2. Layers (bottom-up, never reach up)

- `src/bin/pulse.rs` — parse, run, render errors. Delegates to `pulse::cli`.
- `src/cli/` — thin transport/renderer per command family (`args`, `work`,
  `run`, `learn`, `docs`, `events`, `init`, `serve`, `doctor`, `packet`,
  `checkpoint`, `completion`, `output`). Owns no domain semantics; resolves
  the repo root, renders JSON or text, maps failures to exit codes.
- `src/kernel/` — cross-domain composition, one module per capability:
  `issues` (new/update/dep/transition/note), `ready` (the ready gate),
  `roles` (actor authorization matrix), `reservation` (lease),
  `run` (worker continue loop + lane spawn), `lane` (§8.4 validation +
  seal), `completion` (handoff/close/close-story gates), `checkpoint`,
  `packet`, `profile` (PULSE.md), `learn`, `docs`, `init`, `serve`,
  `doctor`, `registry` (`~/.pulse/projects.json`).
- `src/store/issues.rs` — the one JSONL store: strict read (never skips a
  bad line), embedded JSON-schema validation, atomic read-validate-write
  under the lock.
- `src/storage/` — generic primitives, zero domain knowledge:
  `atomic` (single-target atomic replace), `lock` (repo write lock),
  `append` (append + fsync), `paths` (repo-relative path safety).
- `src/evidence/` — `receipt.rs`: one receipt family (handoff/checkpoint/
  lane/docs), append-only monthly JSONL, artifact hashing, redaction.
- `src/runner/` — process-execution contract: argv split (never a shell),
  `{input}/{ticket}/{repo}/{artifact_dir}` placeholders, timeout,
  process-group kill, bounded output capture, final-JSON-line parse. No
  graph truth — callers map outcomes onto lifecycle and receipts.
- `src/identity/`, `src/event.rs`, `src/source.rs` — actor vocabulary and
  authorization, the append-only event log, the git source fence
  (commit + dirty hash; `.pulse/**` and root `PULSE.md`/`AGENTS.md`
  excluded, `fence_ignore` for host policy).
- `src/serve/` — the board server: registry + workspace discovery
  (`registry.rs`), lenient read-only API (`api.rs`), tiny HTTP layer
  (`http.rs`), the self-contained UI (`assets/board/board.html`).

## 3. Embedded assets and skills

- `templates/` is everything `pulse init` writes into a target repo
  (seeds, prompts, qa scripts, host hooks, schema), embedded with
  `include_str!` — a template change is a code change and is tested.
- `assets/` is this repository's own media (the board UI).
- `skills/` are the four guidance skills (`pulse-shape`, `pulse-plan`,
  `pulse-review`, `pulse-learn`); a guard test parses every `pulse …`
  command they name against the real CLI, same as the AGENTS block.

## 4. Tests

`tests/*.rs` are flat integration-test crate roots; a crate needing more
than one file wires them with `#[path]` (`tests/storage.rs` →
`tests/storage/storage_primitives.rs`). Shared helpers live in
`tests/common/`, included per crate with `#[path]`. Crates:
`architecture_guards`, `communication`, `golden_path`, `public_api_contract`,
`run`, `runner`, `storage`, `target_repo`, `doctor`, `serve`.

The suite must pass at default thread count; races are fixed, never hidden
behind `--test-threads`.

## 5. Error codes

Every kernel/runner error carries code + mandatory hint. The audited list
(what surfaces, what was deleted) lives in
[`docs/plans/0022-error-code-audit.md`](docs/plans/0022-error-code-audit.md).
