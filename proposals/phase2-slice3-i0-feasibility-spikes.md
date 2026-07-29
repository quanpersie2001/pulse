# Phase 2 Slice 3 I0 Feasibility Spike Evidence

Status: implementation evidence for prerequisite spikes only. This document does
not mark `phase2-slice3-single-agent-runner-cancel-resume.md` implemented and
does not claim public `pulse run` behavior exists.

## Spike 1 — process identity, process group, cancellation

Code evidence: `src/process.rs`, `tests/process/run_feasibility.rs`.

Supported platform boundary is explicit:

- Linux: `/proc/<pid>/stat` process group plus start ticks, salted with kernel
  boot ID when readable (`linux_proc_stat_starttime_process_group`).
- macOS: unsupported for cancellation in I0
  (`unsupported_macos_identity_not_proven`). The earlier `ps ... lstart` probe
  is intentionally not treated as proof because it is not a strong kernel
  process creation marker. macOS must keep returning `run_platform_unsupported`
  until a Rust 1.78-compatible `proc_pidinfo`/`kinfo_proc` or equivalent marker
  is implemented and tested.
- All other non-Linux platforms return `run_platform_unsupported` before launch.
  Windows remains out of scope until a Job Object design and tests land.

Cancellation is process-group scoped and guarded by re-reading the current
process identity. A mismatched start marker returns
`run_process_identity_mismatch` and does not signal.

No new process-control dependencies were added; the probe uses Rust 1.78 std plus
small libc FFI declarations. The package declares `rust-version = "1.78"`, and
new I0 code avoids Rust 2024-only syntax such as `unsafe extern` blocks.

## Spike 2 — hidden supervisor packaging

Code evidence: `src/process.rs::{supervisor_packaging_probe,
hidden_supervisor_probe_dispatch}`, `src/cli/process.rs`, and
`tests/process/run_feasibility.rs`.

The packaging probe resolves `std::env::current_exe()` and records the hidden
command token `__run-supervisor`. The binary now has an actual hidden clap
parse/dispatch path for `pulse __run-supervisor --control <path> --probe`; it is
absent from public help, rejects control paths outside
`.pulse/runtime/run/control/*.json`, requires the protected nonce environment,
and re-execs the same installed `pulse` artifact in tests. This remains an I0
feasibility probe only and does not implement public `pulse run` behavior. If
the executable path is unavailable or not a file, the fallback error is
`run_supervisor_spawn_failed`. No daemon or second binary artifact is introduced
by this spike.

## Spike 3 — secure control nonce

Code evidence: `src/process.rs::{ControlNoncePlaintext, ControlNonceV1}` and
`tests/process/run_feasibility.rs`.

The chosen Slice 3 feasibility transport is
`protected_environment_fallback_descriptor_preferred`: descriptor transport is
preferred when the hidden supervisor command is wired, with a protected
environment fallback documented as same-user visible. The plaintext nonce lives
only in memory, is zeroed on drop, and the persisted control record contains only
`nonce_hash`. Tests assert the plaintext hex never appears in the control JSON.

## Spike 4 — bounded continuously-drained logs

Code evidence: `src/process.rs::{drain_to_bounded_logs, BoundedLogRefV1}`.

The retention strategy continuously drains stdout/stderr readers and writes
separate prefix and tail segment files with create-new semantics. Retention is
bounded by the per-stream byte budget. Full-stream `content_hash` is computed by
incremental SHA-256 updates while draining, not by accumulating the full stream
in memory. `content_hash_semantics` is explicit:
`sha256_full_stream_even_when_retention_truncated` for the feasibility helper,
while retained bytes, total bytes and truncated bytes are recorded separately.
There is no unbounded flat raw log file.

## Spike 5 — workspace snapshot feasibility

Code evidence: `src/source.rs::workspace_snapshot_feasibility` and
`tests/target_repo/run_workspace_snapshot.rs`.

The spike computes bounded identities for:

- tracked diff bytes from `git diff --binary --full-index --no-ext-diff --no-color -z <base>`;
- normalized porcelain-v1 `-z` status after excluding only Pulse runtime/cache
  generated paths;
- untracked manifest entries including path, file type, executable bit, byte
  length and content/link digest.

In-place snapshots exclude `.pulse/runtime/` and `.pulse/cache/` so Pulse-owned
logs/control files do not create drift. Canonical `.pulse/workgraph`, docs,
evidence, events, runner profile config and source changes are not excluded.
Symlinks hash their link target without following. LFS pointer files are hashed
as ordinary worktree bytes. Unsupported/special entries and cap overflows yield
non-`complete` snapshot status instead of guessed identity.

## Spike 6 — runner profile threat model

Code evidence: `src/run.rs`, `tests/graph/run_feasibility_contract.rs`.

The profile contract is narrowed to public `codex_process_v1` only. Executables
must be absolute normalized regular files or bare program names resolved through
inherited `PATH`; repository-relative executable paths with separators are
rejected for Slice 3. No shell command string is accepted. Environment reporting
fingerprints only names and source classes, never inherited values. Prompt input
and raw logs are classified as runtime-private repository-sensitive data with
bounded prefix/tail log retention and `not_applied_runtime_private` as the
redaction default. Native resume remains `not_installed`.
