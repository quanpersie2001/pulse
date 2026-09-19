# Pulse — Architecture (v3, current code)

> Status: rewritten at the v3 close-out (P3.4, tag `v0.0.1`). Describes the **code that exists
> today**; `SPEC.md` says what the product is, this file says how the code
> is shaped. Layer rules are guard-tested:
> `tests/architecture_guards.rs` (source-tree scan) and
> `tests/public_api_contract.rs` (stable public paths).

## 1. Shape

One Rust library plus a thin CLI binary. No daemon, no agent runtime, no
network service — the one exception is `pulse serve`, a local read-only
HTTP server (Decision 0023) that binds `127.0.0.1` only. Pulse executes
nothing of its own choosing: the only commands it runs are the argv a record
declares (`pulse verify`, decision 0026). Mutations take a
repository-scoped write lock and end with an atomic store write plus one
appended event.

## 2. Layers (bottom-up, never reach up)

- `src/bin/pulse.rs` — parse, run, render errors. Delegates to `pulse::cli`.
- `src/cli/` — thin transport/renderer per command family (`args`, `work`,
  `lane`, `lease`, `frontier`, `hook`, `learn`, `docs`, `events`, `init`, `serve`,
  `doctor`, `metrics`, `packet`, `checkpoint`, `completion`, `output`). Owns no
  domain semantics; resolves the repo root, renders JSON or text, maps
  failures to exit codes.
- `src/kernel/` — cross-domain composition, one module per capability:
  `issues` (new/update/dep/transition/note), `ready` (the ready gate),
  `roles` (actor authorization matrix), `reservation` (lease; the claim is
  also the `touches` file reservation, decision 0025, and `reserve`
  widens it mid-run), `scope` (pure touches-overlap arithmetic behind
  parallel claims), `frontier` (read-only scheduling view: what can run
  now, and what blocks the rest — decision 0025 B5),
  `hook` (the pre-edit gate, plan 0025 G1: given the path a host hook is
  about to let an agent write, decide allow/deny from the path alone —
  the one place a reservation binds on an edit that never runs a `pulse`
  command. Rules in order: fenced-out paths are free, except a review/qa
  lane may only write under `.pulse/evidence/`; a lane never edits
  source; nothing active → the `hook.unclaimed` policy (default allow);
  a touches-less active ticket is exclusive; otherwise the path must fall
  inside a held `touches`, never inside a verifying one, and outside
  everything it is denied with the `pulse reserve` hint. Read-only, no
  lock, no event — it runs on every edit. The decision is deliberately
  NOT identity-based: at edit time no host says which subagent calls, and
  `--actor` is self-declared (decision 0026), so an absent actor only
  weakens the cross-ticket denials, never the scope rule. Honest limits:
  the hook sees only the host's edit tools — a file written through a
  shell (`sed -i`, `>`) bypasses it, and `handoff_unreserved_changes`
  (decision 0025 B6) is the second net; `pulse hook snippet <host>`
  prints verified host configuration for the user to paste (Claude Code's
  PreToolUse; any other host is `hook_host_unknown` rather than an
  invented config), and Pulse never writes a host settings file),
  `lane` (profile check + bounded lane input + §8.4 validation and seal;
  an opt-in profile **panel** runs the same lane `count` times as blind
  `--seat` reviewers and `pulse lane reconcile` merges their findings into
  the one receipt the close gate reads — decision 0027),
  `completion` (handoff/close/close-story gates; close-story also demands a
  Story's `rules`/`exceptions` ids live in a `docs/**` file listed on the
  Story's `docs_written` — plan 0025 F4), `checkpoint`,
  `packet`, `verify` (the one place Pulse spawns anything: it runs the argv
  a record declares and seals what it observed — decision 0026; the handoff
  and review gates read that receipt, and a reconciled finding's
  `check.argv` is run through the same runner), `profile` (PULSE.md —
  `profiles` and their optional `panels` — and the per-record fence — `fence_for`
  picks the scope snapshot for a record with `touches`, the whole tree
  otherwise, and `same_fence` is the one comparison rule), `learn`,
  `docs`, `init` (enrollment — and `--refresh`'s three-way merge, plan
  0025 G2: every template-written file keeps its as-shipped copy in
  `.pulse/base/`, `git merge-file` folds template changes into files the
  user edited, a conflict never touches the user's file and is filed
  under `.pulse/runtime/refresh/` with `--take-new`/`--keep-mine` to
  resolve), `serve`, `doctor`, `metrics` (read-only computation of
  plan 0022's hand-measured numbers from the event log, receipts, store and
  learnings — every number's definition on its field, non-derivable ones
  listed as `not_derivable` with the reason; plan 0025 E6), `registry`
  (`~/.pulse/projects.json`).
- `src/store/issues.rs` — the one JSONL store: strict read (never skips a
  bad line), embedded JSON-schema validation, atomic read-validate-write
  under the lock.
- `src/storage/` — generic primitives, zero domain knowledge:
  `atomic` (single-target atomic replace), `lock` (repo write lock),
  `append` (append + fsync), `paths` (repo-relative path safety).
- `src/evidence/` — `receipt.rs`: one receipt family (handoff/checkpoint/
  lane/docs), append-only monthly JSONL, artifact hashing, redaction.
- `src/identity/`, `src/event.rs`, `src/source.rs` — actor vocabulary and
  authorization, the append-only event log, the git source fence: the
  whole-tree snapshot (`commit` + dirty hash, `.pulse/**` and root
  `PULSE.md`/`AGENTS.md` excluded, `fence_ignore` for host policy) and —
  decision 0025 — `scoped_snapshot`, which hashes only the files a
  record's `touches` name (`scope:sha256:…`, independent of HEAD), so one
  ticket's close survives another ticket's commit landing first.
- `src/learn/` — the learning loop (plan 0022 §11, plan 0025 E1–E4):
  `store` owns the `.pulse/learnings/LRN-<hash>.md` file shape (frontmatter
  + opaque body); `recall` matches a record's anchors/tags to
  `applies_to`/`tags` — `active` learnings for enforcement, plus
  `candidate`s for the packet, capped at `RECALL_LIMIT` for display only;
  `friction` computes a friction note's classification (`unclassified` /
  `learned` / `dismissed`) read-only from the event log and learning
  citations (`"<subject>#<evt id>"`), the stable key being the
  `note.recorded` event id, never a note index (the record keeps only 50
  notes; plan 0025 E1). Frontmatter carries `check_argv`/`check_cwd` (the
  check `pulse verify` enforces for an `active` learning, decision 0026's
  boundary: only human-activated argv runs) and `cites` (`path`,
  `lines`, `sha256` pinning the code the lesson cites — drift is reported
  as stale, never auto-retired). `mod.rs` owns the mutations (`add`,
  `activate`/`retire` human-only, `dismiss` = a `friction.dismissed` event,
  `record_usage`).
- `src/docs/` — the doc layer over `docs/**/*.md`: optional frontmatter
  (`applies_to`, `tags`, `generated_by`), `applicable` (the same match rule
  as `learn::recall`, demoted by plan 0025 F1 to a packet hint — finding
  docs by content is the agent's own grep/glob job), `check` (`pulse docs
  check`: broken links, missing `docs/README.md` paths, stale
  `generated_by.check_argv`; with `--ticket <id>` it adds one advisory
  low-severity finding per doc the ticket's edits may have staled — low
  findings never flip the verdict, so `check --ticket --write <path>` is
  still the whole `check-docs` lane), and `stale` (plan 0025 F3's reverse
  gate: a doc whose `applies_to` — "which code this doc describes" — names
  files the ticket changed without updating the doc is reported as
  `docs_maybe_stale` at handoff and emitted as a `docs.maybe_stale` event;
  advisory only, never a violation).
- `src/serve/` — the board server: registry + workspace discovery
  (`registry.rs`), lenient read-only API (`api.rs`), tiny HTTP layer
  (`http.rs`), the self-contained UI (`board.html` beside it, embedded
  with `include_str!`).

## 3. Embedded assets and skills

- `templates/` is everything `pulse init` writes into a target repo
  (seeds, prompts, qa scripts, schema), embedded with
  `include_str!` — a template change is a code change and is tested.
- `assets/` is this repository's own media only — the logo mark(s).
  `serve` embeds `assets/logo-icon.svg` and serves it at `/favicon.svg`
  (the board UI itself is source and lives in `src/serve/`).
- `templates/skills/` are the three guidance skills (`pulse-shape`,
  `pulse-plan`, `pulse-learn`), embedded like every other template and
  written into a target repo by `pulse skills install` — `.agents/skills/`
  holds the bodies, each chosen host gets a symlink. They are guarded by
  the same `templates_only_name_commands_the_cli_has` parse as the AGENTS
  block, plus a guard that every directory there is one the installer
  ships.
- `scripts/install-pulse.sh` is the Unix distribution bootstrap: install the
  selected Git ref through Cargo, verify `pulse --version`, then delegate
  repository enrollment and skill linking to the installed CLI. It owns no
  init or host-integration semantics and never edits a host settings file.

## 4. Tests

`tests/*.rs` are flat integration-test crate roots; a crate needing more
than one file wires them with `#[path]` (`tests/storage.rs` →
`tests/storage/storage_primitives.rs`). Shared helpers live in
`tests/common/`, included per crate with `#[path]`. Crates:
`architecture_guards`, `communication`, `doctor`, `golden_path`, `hook`,
`parallel`, `public_api_contract`, `qa_templates` (runs the embedded
`templates/qa/*.mjs` lane scripts with `node` — skipped where node is
absent; the scripts are shipped code and the 0025 dogfood found real bugs
in them), `lane`, `metrics`, `storage`, `serve`,
`target_repo`.

The suite must pass at default thread count; races are fixed, never hidden
behind `--test-threads`.

## 5. Error codes

Every kernel error carries code + mandatory hint. The audited list
(what surfaces, what was deleted) lives in
[`docs/plans/0022-error-code-audit.md`](docs/plans/0022-error-code-audit.md).
