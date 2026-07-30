# Phase 2 — Slice 3: Single-Agent Runner + Cancel/Resume

> **Historical implementation baseline, no longer the target architecture.**
>
> This slice implemented and exercised the CLI-owned run/attempt model, hidden
> per-run supervisor, bounded logs, timeout/cancel, workspace-level resume and
> conservative recovery. It remains useful evidence for process mechanics and
> failure cases, but its ownership boundary has been superseded by
> [Decision 0005](../docs/decisions/0005-rust-daemon-runtime-control-plane.md).
>
> New runtime work must follow the
> [Rust daemon realignment gap](phase2-rust-daemon-realignment-implementation-gap.md).
> Do not extend this proposal as a compatibility contract. Reuse proven
> mechanics behind daemon ownership, then delete the hidden supervisor,
> Core-owned run store and duplicate runner surfaces after replacement tests
> pass.
>
> Tiền đề:
> [`phase2-slice1-work-packet-dispatch-foundation.md`](phase2-slice1-work-packet-dispatch-foundation.md)
> and
> [`phase2-slice2-atomic-reservation-workspace-binding.md`](phase2-slice2-atomic-reservation-workspace-binding.md)
> are implemented and verified through final Slice 2 verifier commit `428f149`.
> Slice 1 owns the read-only `WorkPacketV1` preview. Slice 2 owns
> `PreparedAssignmentV1`, exclusive prepared leases, workspace binding and the
> gated `ready -> active` transition. This Slice 3 proposal must not reinterpret
> those contracts.
> Sở hữu dự kiến: deterministic bounded run input, one local Codex process
> adapter profile, durable run/attempt/supervisor records, safe process start,
> observation, timeout, cancellation, interruption classification,
> workspace-level resume, run recovery, runtime logs and run-state projection.
> Tham chiếu normative:
> [`PULSE_REBOOT.md`](../PULSE_REBOOT.md),
> [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md),
> [`05-cross-agent-coordination.md`](../pulse-reboot/05-cross-agent-coordination.md),
> [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md),
> [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md),
> [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md).

## Executive summary

Slice 2 ends at an exclusive prepared assignment:

```text
WorkPacketV1 preview
  -> atomic claim
  -> exclusive prepared lease
  -> bound workspace
  -> capability match
  -> Ticket ready -> active
  -> PreparedAssignmentV1(dispatch_authorized=true, runner_status=not_started)
```

Slice 3 turns that prepared assignment into one observable and recoverable local
single-Agent run:

```text
PreparedAssignmentV1
  + revalidate assignment/lease/Ticket/workspace under repository fence
  + build deterministic bounded RunInputV1 control envelope
  + render a small versioned Pulse Worker bootstrap prompt
  + durably record run + first attempt as starting
  + launch one Pulse-owned supervisor and one Codex process adapter
  + capture bounded stdout/stderr runtime logs
  + record process identity and run.started
  + observe exit, timeout, cancellation or interruption
  + preserve workspace and record WorkspaceSnapshotV1
  + allow safe workspace-level resume as a new attempt
  -> RunRecordV1 with reproducible local process lifecycle
```

The Ticket remains `active` throughout this slice. A process exiting with code
zero is not proof that acceptance passed. Slice 3 does not create a handoff
receipt, run developer verification, transition `active -> verifying`, decide
`done|rework|blocked`, promote documentation changes or claim Phase 2 complete.
Those are later Phase 2 slices.

The public runner is Codex-first, consistent with D-08, but the kernel/process
boundary is intentionally narrow. Slice 3 installs one current adapter profile,
`codex_process_v1`, which invokes a configured Codex executable without a shell.
Fixture-only internal process adapters may be used by tests. This is not the
Phase 5 independent Codex task/thread transport contract and does not create an
Agent Registry, mailbox, delivery receipt or native thread identity guarantee.
It also does not introduce a mandatory daemon: the supervisor is a short-lived
Pulse-owned child process of the same installed `pulse` binary, invoked through
a hidden internal command surface.

---

## Baseline from Slice 2 and current seams

Implemented Slice 2 provides:

- `pulse::assignment::PreparedAssignmentV1` and strict schemas/fingerprints;
- one live exclusive `AssignmentLeaseRecordV1` in state `prepared`;
- one bound `AssignmentWorkspaceRecordV1` in mode `in_place` or
  `isolated_worktree`;
- one committed `PreparedAssignmentRecordV1` that embeds the exact
  `WorkPacketV1` preview and final assignment transaction fields;
- `JsonGraphStore::claim_work`, `release_work`, `list_leases` and
  `recover_leases` orchestration;
- runtime assignment records under `.pulse/runtime/assignment/`;
- shared recoverable `MultiTargetTransactionIntent` support under
  `.pulse/runtime/transactions/`;
- workspace source validation and conservative cleanup in `src/workspace.rs`;
- source/repository identity and clean exact-base checks in `src/source.rs`;
- event envelope correlation fields including `run_id`, `lease_id`,
  `transaction_id` and `receipt_id`;
- a direct public `ready -> active` rejection unless claim supplies the internal
  prepared-assignment gate context;
- runtime-derived execution frontier claim state.

Current constraints that Slice 3 must preserve:

- `WorkPacketV1` remains a read-only preview and never becomes a bearer token;
- `PreparedAssignmentV1.dispatch.dispatch_authorized=true` means only that a
  runner may start;
- `PreparedAssignmentV1.dispatch.runner_status` remains `not_started` in the
  immutable Slice 2 record; Slice 3 does not rewrite old prepared assignment
  bytes to say `running`;
- assignment lease state remains `prepared`; run liveness is owned by run
  records, not by inventing a lease state named `active`;
- Ticket graph nodes do not gain run IDs, PIDs, logs, heartbeat or workspace
  snapshot fields;
- pure `graph::read` modules remain runtime-independent;
- external process spawn/exit cannot be made atomic with JSON transaction writes;
  recovery choreography must explicitly handle that boundary;
- no command may run against this Pulse development repository as a target;
  integration tests use external temporary repositories copied from tracked
  fixtures.

---

## Goals

Implement enough single-Agent execution that a caller can:

1. start exactly one run for a current live prepared assignment;
2. launch a configured Codex process in the assignment's bound workspace without
   shell interpolation;
3. give Codex a small workflow bootstrap that loads the committed assignment
   through `pulse work packet <ticket-id> --lease <lease-id> --json`, rather than
   copying the Ticket, docs and knowledge corpus into the process prompt;
4. receive a durable `RunStartReportV1` after the supervisor/process start
   handshake succeeds;
5. inspect run and attempt state without mutating repository state;
6. capture bounded stdout/stderr logs in gitignored runtime storage;
7. classify process exit, timeout, cancellation and unexpected interruption;
8. cancel only the process tree proven to belong to the run;
9. recover after Pulse CLI/supervisor/process interruption without relying on
   chat memory or PID alone;
10. resume interrupted work as a new attempt in the same workspace only when the
   recorded workspace snapshot still matches;
11. block duplicate start/resume for the same assignment/workspace;
12. preserve partial source changes and logs across cancel/interruption/recovery;
13. expose run state as a runtime projection while keeping canonical graph truth
   unchanged;
14. provide the run result/source/log bindings needed by a later typed handoff
   slice;
15. verify start/cancel/resume/recovery on native Linux, macOS and Windows hosts
    under real subprocess, process-tree and crash tests.

---

## Non-goals

Slice 3 does not implement:

- Phase 5 independent peer-Agent task/thread creation or Agent Registry;
- stable native Codex thread/session identity mapping;
- typed mailbox, assignment delivery receipt or acknowledgement messages;
- direct transport `send/wait/archive` APIs;
- multi-worker scheduling or orchestration loops;
- reviewer or QA Agent dispatch;
- worker handoff receipts;
- developer verification profiles or review execution;
- `active -> verifying`;
- `verifying -> done|rework|blocked`;
- proof-driven close gate;
- documentation impact promotion or knowledge compounding;
- automatic merge, branch publication, deploy or release authority;
- generic public arbitrary-shell execution;
- PTY/interactive terminal streaming;
- full replayable dirty-worktree archive format;
- cross-machine/shared-network process supervision;
- changing `WorkPacketV1`, `PreparedAssignmentV1` or Slice 2 lease schema
  semantics in place.

If implementation discovers that native Codex task resume, PTY support, handoff
or verification semantics are required for correctness, they must become a
follow-up proposal rather than being silently folded into Slice 3.

Implementation may update this proposal while code is being built, but it must
not flip the proposal to implemented/accepted status or update Phase 2 roadmap
completion evidence until source changes, tests and independent verification
land.

---

## Key decisions for this proposal

### P2S3-D1 — Run state is separate runtime coordination, not lease or graph state

Slice 3 introduces `RunRecordV1` and `RunAttemptRecordV1` under
`.pulse/runtime/run/`. The assignment lease continues to prove exclusive work
ownership. The run record proves process lifecycle and liveness.

Do not add the following to graph node/edge schemas:

- `run_id`;
- `attempt_id`;
- `pid` or process group;
- heartbeat;
- log paths;
- process exit status;
- workspace dirty fingerprint;
- cancellation state.

Do not rewrite the committed Slice 2 lease to a new live state such as `active`.
A live run plus its prepared lease blocks duplicate work. A lease expiring after
`run.started` does not make the assignment claimable; active run state takes
precedence until run recovery resolves it.

### P2S3-D2 — Slice 3 is Codex-process-first, not generic shell and not Phase 5 transport

The only documented public adapter kind is:

```text
codex_process_v1
```

It runs a configured Codex executable with an argv array. Pulse never invokes a
shell, never accepts a free-form command string and never performs variable or
command substitution. The adapter config may choose executable path and a
closed set of adapter arguments, but the kernel owns required run-input,
workspace, logging, timeout and cancellation arguments.

Tests may use an internal `fixture_process_v1` adapter that launches the test
binary or a controlled fixture script. It is not accepted by production CLI
config and is absent from public schemas/help.

This slice does not claim the Codex task/thread lifecycle API is stable. It
supervises a local Codex process. `native_thread_id` and native resume fields are
explicitly `null`/`not_installed` unless the process emits a validated adapter
result supported by a later proposal.

### P2S3-D3 — Runner configuration is tracked operational config, not a CLI command blob

The target repository declares the current runner profile in tracked JSON:

```text
.pulse/run/runner-profiles.json
```

This is an explicit D-68 pre-release contract choice for Slice 3, not evidence
that `.pulse/config.yaml` stops owning broader operational config in the
normative roadmap. Slice 3 does not introduce YAML parsing or reinterpret
`.pulse/config.yaml`; the profile registry is a narrow pre-Core-v1 contract
owned by the runner subsystem and may later be composed into the broader
operational config system through a separate Decision. It is intentionally
tracked because changing the executable/argv/env allowlist changes what code a
runner may execute.

Minimum profile:

```json
{
  "schema_version": 1,
  "default_profile": "codex-local",
  "profiles": [
    {
      "profile_id": "codex-local",
      "adapter": "codex_process_v1",
      "executable": "codex",
      "fixed_args": ["exec", "--json"],
      "environment_allow": ["PATH", "HOME", "CODEX_HOME"],
      "environment_set": {},
      "start_timeout_seconds": 30,
      "run_timeout_seconds": 7200,
      "cancel_grace_seconds": 10,
      "force_kill_after_seconds": 10,
      "max_stdout_bytes": 16777216,
      "max_stderr_bytes": 16777216,
      "log_redaction_patterns": []
    }
  ]
}
```

Rules:

- `schema_version` must be 1;
- profile IDs are unique and filesystem-safe;
- adapter must be `codex_process_v1`;
- executable is one program path/name, not a shell command;
- executable resolution must be deterministic for the local machine: absolute
  paths are allowed only if they are normalized regular files, bare program
  names are resolved with the supervisor's inherited `PATH`, and the resolved
  path plus executable metadata available on the platform are recorded in the
  attempt identity; relative paths containing separators are rejected in Slice 3
  unless a later Decision defines repository-relative tool resolution;
- args are strings with no template expansion except Pulse-owned appended args;
- environment is allowlisted; repository secrets are not copied implicitly;
- inherited environment values are never fingerprinted or reported, so changing
  `PATH`/`CODEX_HOME` between attempts is an operator-controlled local input and
  not a reproducible semantic field;
- timeouts and log limits are bounded by kernel minima/maxima;
- profile bytes have a canonical `profile_fingerprint` bound into the run;
- non-enrolled or missing profile registry fails without runtime bootstrap;
- test-only adapters are injected by internal APIs, not by public JSON.

### P2S3-D4 — `RunInputV1` is control state; Codex receives a small workflow bootstrap

Slice 3 adds `RunInputV1`, built from the committed prepared assignment and
current source/workspace preconditions. The canonical input is the run identity
source and may embed the exact prepared assignment for recovery. It is not a
second semantic Ticket contract and its rendered prompt is not a compiled copy
of the WorkPacket.

`RunInputV1` contains:

- run/attempt IDs;
- exact prepared assignment ID/fingerprint;
- embedded exact `WorkPacketV1` preview;
- lease/workspace binding summary;
- assignee and actor;
- source base and workspace path;
- adapter profile identity;
- resume context when starting a later attempt;
- bootstrap protocol/template version;
- exact Ticket/lease/run identifiers needed to load committed context;
- explicit authority boundary: implementation work only, no close/merge/deploy.

The Codex process receives a small versioned bootstrap prompt that tells it to:

```text
1. Load the committed packet with
   pulse work packet <ticket-id> --lease <lease-id> --json.
2. Read every required documentation section returned by the packet through
   pulse docs get and respect its content hash.
3. Load applicable prior learning through pulse knowledge applicable for the
   Worker/execution audience and moment.
4. Inspect source only after required context is loaded.
5. Stay inside packet scope and stop on declared hard stops.
6. Run the packet verification profile and submit the later typed handoff.
7. Never close, merge, deploy or mutate planning truth.
```

The prompt carries identifiers and workflow only. It does not inline objective,
acceptance, invariants, Story/Epic prose, Decision bodies, docs excerpts, QA
cases or knowledge entries. `pulse work packet --lease` returns the exact packet
embedded in the prepared assignment and validates Ticket, assignee, lease,
workspace and packet fingerprint. It does not rebuild a live preview from a
newer Ticket revision.

The bootstrap prompt and canonical input are repository-sensitive runtime data,
never semantic event payloads and never evidence by default. The run record
stores their hashes, while full bytes live only under the protected, gitignored
run directory. CLI `show/list` must not print prompt/input bytes. An explicit
later diagnostic command may expose bounded redacted input only after a separate
policy review.

`RunInputV1` does not mutate the nested `WorkPacketV1` or
`PreparedAssignmentV1`. It is a control wrapper around committed assignment
identity, not another writable source of Ticket meaning.

### P2S3-D5 — Public CLI is an explicit `run` namespace

Canonical commands:

```bash
pulse --repo-root <repo> run start \
  --lease <lease-id> \
  --actor <principal> \
  [--profile <profile-id>] \
  [--timeout-seconds <seconds>] \
  [--json]

pulse --repo-root <repo> run show <run-id> [--json]
pulse --repo-root <repo> run list [--ticket <ticket-id>] [--state <state>] [--json]

pulse --repo-root <repo> run cancel <run-id> \
  --actor <principal> \
  --reason <reason> \
  [--grace-seconds <seconds>] \
  [--no-force] \
  [--json]

pulse --repo-root <repo> run resume <run-id> \
  --actor <principal> \
  [--profile <profile-id>] \
  [--timeout-seconds <seconds>] \
  [--json]

pulse --repo-root <repo> run recover \
  --actor <principal> \
  [--json]
```

Slice 3 does not add `pulse run <ticket-id>` claim+start convenience. Claim and
start remain separate so process side effects are never hidden inside the
assignment transaction and operators can inspect prepared state before launch.

`run start` returns after the supervisor/process start handshake and durable
`run.started` commit, not after the Codex process exits. `run show`, `run list`
and `run recover` provide observation. A hidden internal supervisor command may
exist but is excluded from public help and contracts.

### P2S3-D6 — Pulse uses a supervisor handshake, not a naked background PID

The public CLI does not spawn Codex and immediately trust a PID. It launches a
Pulse-owned supervisor process by re-executing the current `pulse` binary (or a
configured test binary in internal tests) with a hidden command. Slice 3 does
not require a daemon, service manager, background server installation or second
supervisor artifact. If the implementation cannot reliably determine the
current executable path through Rust 1.78-compatible APIs on a supported
platform, that platform is `run_platform_unsupported` until the prerequisite
spike resolves packaging. The supervisor receives:

- run ID and attempt ID;
- random supervisor nonce;
- restricted runtime control directory;
- stdout/stderr file descriptors/paths;
- adapter profile and input paths;
- one-way startup status pipe or inherited descriptor.

The supervisor:

1. validates its run/attempt/nonce control record;
2. creates a dedicated process group/session or platform-equivalent job;
3. launches the Codex process in the bound workspace;
4. reports child identity through the startup handshake;
5. writes heartbeat/control observations only through managed paths with
   restrictive directory/file permissions where supported, no symlink following,
   and create-new/atomic-replace semantics;
6. enforces the run timeout because it owns the child wait loop;
7. waits for exit or cancel request;
8. records an exit observation file using create-new/atomic replace;
9. never mutates graph, assignment or semantic event files directly.

The parent `run start` command owns only the startup handshake timeout and the
semantic transition to `running`; it is not required to stay alive for the run
timeout or terminal finalization.

The parent `run start` command durably commits `run.started` only after the
handshake proves the supervisor and child were created. The supervisor nonce and
process-start proof prevent PID alone from becoming authority.

### P2S3-D7 — Process ownership is platform-adapted and PID alone is never sufficient

The portable supervisor core owns nonce/handshake validation, timeout, bounded
logs, heartbeat, cancel state and exit observation. Platform adapters own
executable resolution, isolated process-tree creation, identity, graceful
termination, force termination, parent-lifetime transport and runtime file
protection.

Process identity is a tagged platform contract rather than one Unix-shaped set
of optional fields:

```json
{
  "supervisor_pid": 41001,
  "child_pid": 41002,
  "supervisor_nonce_hash": "sha256:...",
  "started_at": "2026-07-29T10:00:00Z",
  "platform": {
    "kind": "linux_v1",
    "process_group_id": 41002,
    "boot_id": "...",
    "start_ticks": 982341
  },
  "argv_hash": "sha256:...",
  "executable_identity": "best_effort:...",
  "identity_status": "verified"
}
```

The `platform_start_marker` is not a vague string. The implementation must
choose and test a concrete marker per supported platform before enabling start:
for example process start time/boot ID from `/proc` on Linux, an available
kernel process start timestamp on macOS, or a Job Object/process creation marker
on Windows. If the chosen marker cannot distinguish PID reuse well enough for
conservative cancellation, the platform remains unsupported.

Rules:

- cancellation never signals a PID unless nonce/control record and platform
  start marker match the current attempt;
- process group/job ownership is required for tree cancellation;
- PID reuse or unverifiable identity yields `run_process_identity_mismatch` and
  `stale_needs_operator`; Pulse does not kill the process;
- platform support lives behind one low-level `ProcessPlatform` boundary;
- Linux uses a dedicated process group plus `/proc` boot ID/start ticks;
- macOS uses a dedicated process group plus a proven public kernel/libproc
  process start marker, process group and executable identity; `ps` text output
  is not identity proof;
- Windows creates the child suspended, assigns it to an owned Job Object before
  resume, uses process creation time/job membership/executable identity, attempts
  graceful console-group termination when available and uses the Job Object for
  forceful whole-tree termination;
- generic `cfg(unix)` is not platform proof;
- any crate added for process groups, signals, metadata or job objects must be
  MSRV-audited against Rust 1.78 before coding begins;
- Linux, macOS and Windows are required Tier-1 runner targets before Slice 3 is
  complete;
- any other or still-unproven platform fails before launch with
  `run_platform_unsupported`, not with best-effort unsafe cancellation.

### P2S3-D8 — Run and attempt states are separate

One logical run may have multiple attempts. Public state vocabulary:

Run state:

```text
starting
running
cancel_requested
interrupted
exited
cancelled
failed_to_start
stale_needs_operator
```

Attempt state:

```text
starting
running
cancel_requested
exited
cancelled
interrupted
failed_to_start
stale_needs_operator
```

There is no durable attempt `prepared` state in Slice 3. A new attempt is
created by the same transaction that makes it `starting`, because a persisted
non-starting attempt would create a second lifecycle vocabulary with no process
or recovery semantics.

Resume availability is a derived projection field, not a durable run state:

```json
{
  "resume_eligibility": "available|not_available|blocked|not_evaluated",
  "resume_blockers": []
}
```

Invariants:

- a run has exactly one `current_attempt_id`;
- only one attempt may be `starting|running|cancel_requested`;
- resume can be available only when there is no live process and all resume
  preconditions pass;
- `exited` records a known process exit and may be success or failure;
- process exit zero does not mean Ticket done;
- `interrupted` means the known process is gone or supervisor continuity broke;
- `stale_needs_operator` means Pulse cannot safely classify/kill/resume;
- for duplicate-start and lease recovery,
  `starting|running|cancel_requested|interrupted|failed_to_start|stale_needs_operator`
  are unresolved until a later explicit disposition/release policy exists;
- `exited|cancelled` are process-terminal but still do not release the
  assignment or close the Ticket;
- terminal run records remain available; local tombstones are optional only for
  future retention and are not used to hide history.

### P2S3-D9 — Lease TTL is pre-run TTL; unresolved run state owns execution liveness

Before any `run.starting` transaction is durable, the Slice 2 prepared lease TTL
applies normally. Once a run record exists, even if it is still `starting` and
`run.started` has not committed yet:

- the prepared lease remains the ownership binding;
- lease expiry alone does not permit another claim or auto-requeue;
- duplicate claim/start is blocked by the live or unresolved run record;
- `starting` recovery must deterministically adopt, fail-to-start, or mark stale
  before any later assignment disposition policy may consider release/requeue;
- after `run.started`, run liveness uses supervisor heartbeat and process
  identity rather than lease TTL;
- `work leases recover` must refuse to expire/requeue an assignment with a live
  or unresolved run and report `blocked_by_run`;
- `work release` remains no-run only and rejects once any run record exists;
- cancellation/recovery does not automatically release/requeue the assignment;
  a later explicit disposition/handoff/release policy must resolve partial work.

Slice 3 does not add periodic lease renewal or peer-Agent acknowledgement. The
successful start handshake is the local single-Agent execution acknowledgement.

### P2S3-D10 — Process start is a two-transaction saga with explicit orphan recovery

External spawn cannot join the storage transaction. The safe start choreography
is:

```text
A. under repository fence:
   recover storage/run state
   authorize + validate assignment/lease/Ticket/workspace/profile
   block duplicate live/unresolved run
   build RunInputV1
   commit run + attempt(state=starting) + run.starting event

B. outside repository fence:
   launch Pulse supervisor
   supervisor launches Codex and returns verified identity handshake

C. under repository fence:
   reload and require same run/attempt still starting
   reject/stop if cancellation or conflicting mutation appeared
   commit run + attempt(state=running, process identity) + run.started event
```

Failure rules:

- failure before A commits creates no run;
- crash after A before supervisor launch is recovered to `failed_to_start` after
  startup grace expires;
- spawn failure commits `failed_to_start` and `run.failed_to_start`;
- process spawned but parent crashes before C is an orphan-candidate: recovery
  adopts only when nonce, control record, supervisor heartbeat and platform
  process identity all match; otherwise mark `stale_needs_operator` and do not
  launch a replacement;
- cancellation arriving between B and C causes the parent/recovery path to send
  a verified cancel request before recording final state;
- each semantic state commit uses the shared `MultiTargetTransactionIntent` and
  exactly one event;
- storage failpoints remain mechanical; runner code does not invent semantic
  target-order failpoint names.

### P2S3-D11 — Cancellation is request, verified signal, observation, then final commit

Cancel choreography:

```text
1. validate enrollment and authorize work.run.cancel
2. acquire repository fence and recover transactions
3. load run/attempt and verify cancellable state
4. commit cancel_requested state + run.cancel_requested event
5. release fence
6. send nonce-bound cancellation request to supervisor
7. supervisor sends graceful signal to owned process group/job
8. wait bounded grace; optionally force terminate after policy timeout
9. supervisor writes exit observation
10. reacquire fence and commit cancelled/interrupted/stale result + event
```

`--no-force` forbids escalation. Without it, force termination is allowed only
after configured grace and verified process identity. Cancellation preserves the
workspace and logs. It never transitions the Ticket, releases the assignment or
deletes an isolated worktree.

Repeated cancellation is idempotent:

- already `cancel_requested` returns current state;
- already `cancelled|exited|failed_to_start` returns a non-mutating report;
- identity mismatch does not signal and records/reports operator action.

### P2S3-D12 — Resume is workspace-level new attempt, not fake native-thread resume

`pulse run resume <run-id>` creates a new attempt under the same logical run and
same assignment/workspace. It does not claim to resume a native Codex thread.

Resume eligibility:

- run is `interrupted|failed_to_start|exited` and policy allows another
  attempt; cancellation is not automatically resumable in Slice 3 because a
  user-requested cancel may mean stop-work rather than retry;
- no live/unresolved attempt exists;
- prepared lease, prepared assignment, Ticket active revision, workspace ID and
  repository identity still match;
- workspace is not in merge/rebase/cherry-pick/revert/bisect operation;
- current `WorkspaceSnapshotV1` equals the previous attempt's recorded final or
  interrupted snapshot; for a clean `failed_to_start` with no child identity and
  no final/interrupted snapshot, current preflight snapshot must equal the
  original `workspace_before` snapshot and original start preconditions;
- source scope has not been externally modified since that comparison snapshot;
- adapter profile is current and compatible, or caller explicitly chooses a
  current profile under policy;
- actor has `work.run.resume`.

Resume builds a new `RunInputV1` containing bounded previous-attempt context:
exit/interruption class, workspace snapshot identity, bounded redacted log tail
and explicit instruction to inspect existing partial work. It never embeds full
raw logs by default.

`native_resume_status` is `not_installed`. If future Codex transport supports a
native session, that is a new contract/proposal.

### P2S3-D13 — Workspace snapshot is deterministic identity, not a full archive

Slice 3 introduces `WorkspaceSnapshotV1` for drift detection. It is captured for
the bound workspace path, not implicitly for the repository root, and the record
must state whether the workspace is `in_place` or `isolated_worktree`. In-place
runs are allowed only after excluding Pulse-owned runtime/coordination paths
from source drift identity, otherwise heartbeats/logs/control files would make
the workspace appear changed by Pulse itself. Exclusion rules are fixed and
narrow (`.pulse/runtime/`, `.pulse/cache/`, and other managed Pulse-generated
runtime/cache paths only); `.pulse/workgraph`, `.pulse/docs`, `.pulse/evidence`,
`.pulse/events`, `.pulse/run/runner-profiles.json`, docs, source files and
arbitrary user/project gitignored files are not excluded by this Pulse-runtime
rule. If a worker intentionally edits ignored files that are part of the bounded
source scope, Slice 3 must either include them through an explicit source-scope
rule or mark the snapshot unsupported rather than silently hiding the change.

```json
{
  "schema_version": 1,
  "repository_id": "repo_...",
  "workspace_id": "wt_...",
  "workspace_mode": "isolated_worktree",
  "base_commit": "012345...",
  "head_commit": "012345...",
  "diff_base_commit": "012345...",
  "operation_state": "none",
  "cleanliness": "dirty",
  "tracked_diff_identity": "sha256:...",
  "untracked_manifest_identity": "sha256:...",
  "status_identity": "sha256:...",
  "snapshot_status": "complete",
  "captured_at": "..."
}
```

Canonicalization:

- `base_commit` is the prepared assignment source commit; `head_commit` is the
  current workspace HEAD; `diff_base_commit` is the merge-base/diff base used to
  compute tracked changes and must equal `base_commit` for the first Slice 3
  implementation unless a later Decision permits rebased workspaces;
- tracked changes hash raw bytes from `git diff --binary --full-index
  --no-ext-diff --no-color -z <diff_base_commit> -- <included paths>`, with
  stable environment (`LC_ALL=C`, no external diff driver) and with unsupported
  Git diff features causing `snapshot_status=unsupported` rather than a guessed
  identity;
- status identity hashes normalized `git status --porcelain=v1 -z
  --untracked-files=all -- <included paths>` bytes after applying the same
  Pulse-runtime exclusions;
- all path lists are parsed as NUL-delimited bytes and rejected as unsupported if
  they cannot be represented as safe repository-relative UTF-8 managed paths;
- untracked manifest sorts repository-relative paths and hashes path, file type,
  executable bit, byte length and content digest;
- regular files are hashed by bytes; invalid UTF-8 file content is allowed;
- symlinks hash link target bytes and never follow outside workspace;
- executable-bit changes are included for tracked and untracked regular files;
- Git LFS pointer files are hashed as files as seen in the worktree; Pulse does
  not dereference LFS object storage in Slice 3;
- submodule, nested repository, named pipe, socket, device and other unsupported
  special-file changes yield `snapshot_status=unsupported`;
- each untracked file and total bytes are capped; tracked diff output is also
  capped because a huge binary diff can otherwise make resume hashing
  unbounded;
- exceeding caps yields `snapshot_status=bounded_out` and blocks automatic
  resume with `run_workspace_snapshot_unsupported`;
- snapshot identifies current workspace state but is not a replayable archive;
  raw diff/content is not stored in events.

This resolves enough of prototype question #4 for local resume drift detection
without claiming a general dirty-source evidence format. Handoff/receipt replay
may require a later stronger snapshot contract.

### P2S3-D14 — Logs are runtime data with bounded retention and explicit redaction status

Retained log bytes live under managed per-run log paths, represented as bounded
prefix/tail segments or an equivalent tested ring layout:

```text
.pulse/runtime/run/logs/<run-id>/<attempt-id>.<stream>.<segment>.log
```

Rules:

- paths are managed, validated and gitignored;
- logs are not semantic event payloads;
- per-stream byte limits come from bounded profile settings;
- after the limit, Pulse keeps the first bounded prefix and rolling bounded tail,
  records truncated byte count and continues draining the child to avoid
  deadlock;
- `RunLogRefV1` contains path, byte counts, content hash, truncation and
  redaction status;
- default raw log `redaction_status` is `not_applied_runtime_private`;
- human/JSON CLI output returns only a bounded tail hash/count by default; a
  caller must opt into `--tail-bytes <n>` to render log bytes, and rendered bytes
  remain capped even for JSON;
- when rendering is requested, Pulse applies the current explicit redaction
  profile; if no redaction profile is configured, output is marked
  `redaction_status=not_applied_runtime_private` and remains bounded;
- regex redaction rules must be length/time bounded by implementation tests; an
  unsafe redaction profile fails closed rather than rendering raw output;
- bounded prefix+tail retention cannot be implemented by appending to a single
  flat file and hashing at the end without unbounded storage; the process module
  must either use segmented prefix/tail files plus counters or another tested
  bounded ring strategy, and `content_hash` must explicitly identify whether it
  covers full untruncated content or only retained bytes;
- no raw prompt, environment secret or full log is written into event files;
- promoting logs to evidence is deferred unless explicit redaction or
  caller-asserted policy is supplied through a later receipt flow;
- environment values are never serialized; only allowed variable names and an
  environment-spec fingerprint are recorded; the fingerprint covers variable
  names, source class (`inherited` or `literal_non_secret`) and tracked profile
  structure, not the inherited values themselves.

### P2S3-D15 — Run exit is observation, not handoff or proof

Known exit result:

```json
{
  "kind": "exited",
  "code": 0,
  "signal": null,
  "timed_out": false,
  "cancelled": false,
  "observed_at": "..."
}
```

On exit Pulse records:

- final attempt state;
- run state `exited` or `cancelled`;
- final workspace snapshot;
- log references;
- exit observation and reason codes;
- `run.exited` or `run.cancelled` event.

It does not:

- mark implementation successful;
- create verification receipt;
- transition `active -> verifying`;
- release lease/workspace;
- delete partial work;
- change acceptance/docs impact/QA posture.

A later handoff slice consumes `RunRecordV1`, attempt result, workspace snapshot
and logs to create a typed worker handoff proposal.

### P2S3-D16 — Authority actions are explicit and default-deny

New grant vocabulary:

```text
work.run.start
work.run.cancel
work.run.resume
work.run.recover
```

Rules:

- `work.run.recover` is a mutation grant, not an observation grant: read-only
  stale/invalid classification by `run show/list` remains allowed through the
  normal read surface and must not require mutation authority;
- actor grant is checked independently from assignee capability inventory;
- assignee must equal the prepared lease owner;
- start/resume actor may differ from assignee only if policy grants the action;
- process runtime capability comes from the prepared assignment capability
  match and adapter profile, not from authority grant;
- cancel/recover authority does not grant merge/close/deploy;
- run supervisor is `system:pulse-run-supervisor` in operational observations,
  but semantic mutations use the authorized initiating/recovering actor and
  include supervisor identity in payload;
- no implicit human superuser.

### P2S3-D17 — Run recovery is conservative and idempotent

`pulse run recover` runs under the repository fence for state mutations but may
perform bounded process observations outside the fence. It:

1. validates enrollment before runtime creation;
2. recovers shared storage transaction intents;
3. scans run/attempt/control/exit observation records;
4. classifies without mutation;
5. applies only deterministic repairs authorized by `work.run.recover`;
6. revalidates before each commit.

Classifications:

```text
live
starting_within_grace
starting_orphan_adoptable
starting_without_process
running_process_gone
exit_observation_pending_commit
cancel_requested_live
cancel_requested_process_gone
resume_available_derived
terminal
stale_needs_operator
invalid
orphan_control
orphan_log
```

Safe automatic repairs:

- complete shared transaction recovery;
- adopt a starting supervisor only with full nonce/process identity proof;
- adopt a fast-exited starting supervisor only when a verified exit observation
  proves the same child identity, then complete `run.started` before terminal
  finalization;
- mark expired starting-without-process as `failed_to_start`;
- commit a matching exit observation to `exited|cancelled`;
- classify a verified-gone running process as `interrupted` and report derived
  `resume_eligibility=available` if workspace snapshot succeeds;
- clean abandoned empty control temp files after state is durable.

Never automatic:

- kill an identity-mismatched process;
- delete workspace or logs;
- requeue/release Ticket;
- overwrite drifted workspace snapshot;
- launch a new attempt;
- infer success from code zero;
- repair invalid/non-prefix transaction or contradictory records.

### P2S3-D18 — Events describe process lifecycle only

Slice 3 event types:

```text
run.starting
run.started
run.failed_to_start
run.cancel_requested
run.cancelled
run.interrupted
run.resume_starting
run.exited
run.recovered
```

`run.resume_starting` owns the starting-state commit for attempt number > 1.
The later verified-running commit for that attempt emits `run.started` with the
new attempt ID; Slice 3 does not add a separate `run.resumed` terminal or running
event.

`run.recovered` is emitted only for a recovery mutation that changes durable run
state but has no more specific event type. Recovery that completes a prepared
`run.started`, `run.exited`, `run.cancelled`, `run.interrupted` or
`run.failed_to_start` transition emits that specific lifecycle event instead,
not an additional `run.recovered` audit event.

Semantic event commit owner is deterministic: the public parent command owns
`run.starting`, `run.resume_starting` and the normal `run.started`; the cancel
caller owns `run.cancel_requested`; terminal observation finalization is owned
by the first authorized mutating path that holds the repository fence (`run
recover`, `run cancel` finalization, or a future explicit foreground observer),
and recovery owns adopted/repair transitions. Repeated commands use current
state plus transaction/event IDs to return idempotently rather than emitting a
second semantic event.

Every event uses the shared `EventEnvelope` and correlation:

```json
{
  "run_id": "run_...",
  "lease_id": "lease_...",
  "transaction_id": "txn_..."
}
```

Payloads include IDs, state transition, attempt ID, prepared assignment
fingerprint, profile fingerprint, workspace snapshot identity and bounded exit
metadata. They exclude prompt bytes, environment values, raw logs and source
diffs.

One state commit emits exactly one semantic event. Supervisor heartbeat and raw
process output are operational runtime observations, not semantic events.

### P2S3-D19 — Read-only run projections preserve no-bootstrap behavior

`run show` and `run list`:

- validate repository enrollment in preserve/no-bootstrap mode;
- do not create `.pulse/runtime`, locks, run directories or caches;
- tolerate missing runtime directories as empty state;
- report corrupt/ambiguous records with explicit nullable fields;
- join Ticket/assignment/workspace state in kernel/CLI composition;
- do not modify graph or recover implicitly.

Execution frontier enrichment may add nullable `run_state`, `run_id` and
`attempt_id` to active assignment projection. Pure graph frontier DTOs remain
unchanged.

### P2S3-D20 — Hidden supervisor is an internal adapter surface

A hidden command such as:

```text
pulse __run-supervisor --control <managed-relative-path>
```

may be used internally. It is invoked with the same `--repo-root` parsing path
as public commands so control paths are interpreted relative to the enrolled
target repository, but it is absent from public help. It must:

- be absent from public help and stable docs;
- reject control paths outside `.pulse/runtime/run/control/`;
- authenticate the control record via random nonce passed through an inherited
  descriptor where supported, otherwise a protected environment variable with
  documented same-user threat limitations; the nonce plaintext is never written
  to disk and only its hash is stored;
- never accept arbitrary graph/repository mutations;
- never parse untrusted shell commands;
- return structured internal exit classes for tests/recovery;
- be replaceable without public compatibility obligation.

---

## Public CLI contract

### Start

```bash
pulse --repo-root <repo> run start \
  --lease <lease-id> \
  --actor <principal> \
  [--profile <profile-id>] \
  [--timeout-seconds <seconds>] \
  [--json]
```

Arguments:

- `--lease`: required live prepared implementation lease;
- `--actor`: required principal authorized for `work.run.start`;
- `--profile`: optional runner profile; defaults to registry
  `default_profile`;
- `--timeout-seconds`: optional per-run override bounded by profile/kernel;
- no `--command`;
- no `--shell`;
- no `--allow-dirty`;
- no `--force-source`;
- no `--skip-assignment-checks`.

Human output example:

```text
run run_01J... started
attempt: attempt_01J...
ticket: TK-031 (active revision 9)
lease: lease_01J...
workspace: wt_TK-031_01J... (isolated worktree)
profile: codex-local (codex_process_v1)
supervisor: running
process: running
logs: .pulse/runtime/run/logs/run_01J.../
timeout: 7200s
handoff/verification: not installed by Slice 3
```

### Show/list

```bash
pulse --repo-root <repo> run show <run-id> [--json]
pulse --repo-root <repo> run list \
  [--ticket <ticket-id>] \
  [--state <state>] \
  [--json]
```

Unavailable optional JSON fields are `null`, not omitted.

### Cancel

```bash
pulse --repo-root <repo> run cancel <run-id> \
  --actor <principal> \
  --reason <reason> \
  [--grace-seconds <seconds>] \
  [--no-force] \
  [--json]
```

Cancellation is process-scoped. It preserves Ticket, lease, workspace, source
changes and logs.

### Resume

```bash
pulse --repo-root <repo> run resume <run-id> \
  --actor <principal> \
  [--profile <profile-id>] \
  [--timeout-seconds <seconds>] \
  [--json]
```

Resume creates a new attempt. It does not create a new run ID, lease or
workspace and does not fake native Codex thread resume.

### Recover

```bash
pulse --repo-root <repo> run recover \
  --actor <principal> \
  [--json]
```

Recovery applies safe deterministic repairs and reports unresolved cases.

---

## Runner profile contract

Schema path:

```text
src/schema/run/runner-profiles.schema.json
```

Top-level shape:

```json
{
  "schema_version": 1,
  "default_profile": "codex-local",
  "profiles": []
}
```

Profile rules:

- maximum 32 profiles;
- profile ID length 1..128;
- executable length 1..4096 and contains no NUL;
- executable path resolution follows P2S3-D3: absolute normalized file or bare
  program name only; repository-relative executable paths are deferred;
- the resolved executable identity is recorded in attempts but excluded from
  profile fingerprint because it is local-machine state, not tracked config;
- fixed args maximum 64, each length 0..4096;
- no shell metacharacter interpretation because no shell is used;
- allowlisted environment names match `[A-Z_][A-Z0-9_]*`;
- explicitly set values are not returned in JSON reports/events;
- start timeout range 1..300 seconds;
- run timeout range 60..86400 seconds;
- cancel grace range 1..300 seconds;
- force-kill delay range 0..300 seconds;
- log limit per stream range 65536..67108864 bytes;
- canonical profile fingerprint covers all tracked, non-secret semantics;
  inherited environment values are deliberately excluded and reported through an
  environment-spec fingerprint so reports remain reproducible without leaking
  secrets;
- secret-bearing environment values are disallowed in tracked registry and must
  come from allowed existing environment or a future secret provider.

---

## Runtime filesystem layout

All paths are target-repository local runtime coordination state:

```text
.pulse/runtime/
  locks/
    workgraph.lock
  transactions/
    txn_01J....json
  run/
    runs/
      run_01J....json
    attempts/
      attempt_01J....json
    control/
      run_01J....json
      run_01J....cancel.json
      run_01J....exit.json
      run_01J....heartbeat.json
    inputs/
      run_01J....attempt_01J....json
      run_01J....attempt_01J....md
    logs/
      run_01J.../
        attempt_01J....stdout.prefix.log
        attempt_01J....stdout.tail.log
        attempt_01J....stderr.prefix.log
        attempt_01J....stderr.tail.log
    snapshots/
      attempt_01J....before.json
      attempt_01J....after.json
```

The exact log retention filenames may change during implementation, but they
must represent bounded retained bytes rather than an unbounded append-only raw
log.

Rules:

- commands validate enrolled repository before creating these paths;
- managed relative paths reject traversal and symlink escape;
- runtime paths are gitignored and never evidence by default;
- record JSON is canonical and strict;
- run/attempt/control IDs are unique, filesystem-safe and not derived solely
  from Ticket ID;
- logs/input/snapshots are referenced by records but not canonical graph truth;
- shared transaction intents remain in `.pulse/runtime/transactions`, not a
  run-specific transaction root.

---

## RunRecordV1 contract

```json
{
  "schema_version": 1,
  "run_id": "run_01J...",
  "kind": "single_agent_implementation",
  "state": "running",
  "subject": {
    "kind": "ticket",
    "id": "TK-031",
    "active_revision": 9,
    "contract_revision": 4
  },
  "assignment": {
    "lease_id": "lease_01J...",
    "prepared_assignment_id": "pa_01J...",
    "prepared_assignment_fingerprint": "sha256:...",
    "packet_fingerprint": "sha256:...",
    "assignee": "agent:codex-local"
  },
  "workspace": {
    "workspace_id": "wt_TK-031_01J...",
    "mode": "isolated_worktree",
    "path": ".pulse/runtime/workspaces/wt_TK-031_01J...",
    "repository_id": "repo_...",
    "base_commit": "012345..."
  },
  "runner": {
    "adapter": "codex_process_v1",
    "profile_id": "codex-local",
    "profile_fingerprint": "sha256:...",
    "resolved_executable_identity": "best_effort:...",
    "native_resume_status": "not_installed",
    "native_thread_id": null
  },
  "current_attempt_id": "attempt_01J...",
  "attempt_ids": ["attempt_01J..."],
  "created_by": "human:quannv",
  "created_at": "2026-07-29T10:00:00Z",
  "updated_at": "2026-07-29T10:00:01Z",
  "last_heartbeat_at": "2026-07-29T10:00:05Z",
  "latest_exit": null,
  "latest_workspace_snapshot_identity": "sha256:...",
  "reason_codes": [],
  "run_fingerprint": "sha256:..."
}
```

`run_fingerprint` excludes itself, volatile heartbeat and non-semantic rendering
fields. It includes IDs, assignment/workspace binding, adapter profile identity,
current attempt, state, terminal exit metadata and latest workspace snapshot
identity. Heartbeat currentness is runtime observation, not run identity.

---

## RunAttemptRecordV1 contract

```json
{
  "schema_version": 1,
  "attempt_id": "attempt_01J...",
  "run_id": "run_01J...",
  "attempt_number": 1,
  "state": "running",
  "input": {
    "run_input_identity": "sha256:...",
    "json_path": ".pulse/runtime/run/inputs/run_...attempt_....json",
    "rendered_prompt_identity": "sha256:...",
    "rendered_prompt_path": ".pulse/runtime/run/inputs/run_...attempt_....md"
  },
  "process": {
    "identity": {},
    "started_at": "2026-07-29T10:00:01Z",
    "ended_at": null,
    "exit": null
  },
  "workspace_before": {},
  "workspace_after": null,
  "logs": {
    "stdout": {},
    "stderr": {}
  },
  "timeout_seconds": 7200,
  "cancel": {
    "requested_at": null,
    "requested_by": null,
    "reason": null,
    "grace_seconds": null,
    "force_allowed": null
  },
  "created_at": "2026-07-29T10:00:00Z",
  "updated_at": "2026-07-29T10:00:01Z",
  "reason_codes": [],
  "attempt_fingerprint": "sha256:..."
}
```

Attempt fingerprint excludes itself and volatile heartbeat only. Times, process
identity, input identity, workspace snapshots, log hashes and exit metadata are
semantic.

All public Slice 3 DTOs use `#[serde(deny_unknown_fields)]`, independent JSON
schemas, canonical JSON tests and no floats.

---

## RunInputV1 contract

```json
{
  "schema_version": 1,
  "profile": "phase2_single_agent_run_input_v1",
  "run_id": "run_01J...",
  "attempt_id": "attempt_01J...",
  "attempt_number": 1,
  "mode": "start",
  "prepared_assignment": {},
  "workspace": {},
  "runner_profile": {
    "profile_id": "codex-local",
    "adapter": "codex_process_v1",
    "profile_fingerprint": "sha256:..."
  },
  "bootstrap": {
    "protocol": "pulse_worker_v1",
    "prompt_template_version": 1,
    "ticket_id": "TK-031",
    "lease_id": "lease_01J...",
    "packet_fingerprint": "sha256:...",
    "packet_command": [
      "pulse",
      "work",
      "packet",
      "TK-031",
      "--lease",
      "lease_01J...",
      "--json"
    ],
    "authority_boundary": [
      "do_not_change_acceptance",
      "do_not_close_ticket",
      "do_not_merge_or_deploy"
    ]
  },
  "resume": {
    "previous_attempt_id": null,
    "workspace_snapshot_identity": null,
    "previous_exit_kind": null,
    "redacted_log_tail": null,
    "native_resume_status": "not_installed"
  },
  "input_fingerprint": "sha256:..."
}
```

The embedded prepared assignment is the committed Slice 2 value and retains its
nested preview semantics. The bootstrap block is a retrieval protocol, not a
duplicate of the embedded Ticket contract.

---

## Run start algorithm

### Phase A — preserve-only validation before lock

- validate enrolled repository without bootstrap;
- load/validate runner profile registry without creating runtime paths;
- validate lease/run IDs and CLI bounds;
- parse actor;
- reject unsupported platform/profile before runtime creation where possible.

### Phase B — preflight under repository fence

- acquire `WriteGuard`;
- recover shared transaction intents;
- classify run state and block ambiguous recovery;
- authorize actor for `work.run.start`;
- load lease and require live prepared implementation assignment;
- require no terminal lease tombstone;
- load prepared assignment and require fingerprint/IDs match lease;
- require dispatch authorized and runner status `not_started` in immutable
  prepared assignment;
- load Ticket and require `active` at prepared `revision_after`;
- load workspace and require bound, matching lease/repository/base/path;
- block any live/unresolved run for lease/workspace/Ticket;
- revalidate workspace source identity and expected pre-start cleanliness;
- build `WorkspaceSnapshotV1` before state;
- build `RunInputV1` and bounded versioned Worker workflow bootstrap;
- create run/attempt IDs, control nonce/hash and runtime paths;
- reject if canonical `RunInputV1` or rendered Markdown exceeds configured
  bounded input budgets; do not silently truncate instructions or embedded
  packet bytes;
- commit run + attempt starting records and `run.starting` event in one shared
  multi-target transaction.

### Phase C — launch outside fence

- create managed log/input/control files;
- spawn hidden Pulse supervisor with protected control path/nonce;
- supervisor starts Codex without shell in workspace;
- supervisor returns verified process identity handshake before start timeout;
- if the child exits after the verified handshake but before the parent commits
  `run.started`, the parent/recovery still records the proven `run.started`
  transition first and then finalizes from the exit observation; it must not
  collapse a real started process into `failed_to_start`;
- if timeout/spawn/config/log setup fails before verified child identity exists,
  stop any proven process and proceed to failed-start finalization.

### Phase D — started commit under fence

- reacquire `WriteGuard`;
- recover transactions;
- reload run/attempt/control;
- require same attempt still `starting`;
- revalidate assignment/Ticket/workspace and no conflicting run;
- if cancel requested, do not mark running; request verified cancellation;
- update run/attempt to `running` with identity and heartbeat;
- commit exactly one `run.started` event;
- if a matching exit observation already exists, return a start report that
  includes `terminal_observation_pending=true`; finalization is performed by the
  next observer/recover path so the start command keeps one semantic state commit
  per transaction;
- return committed `RunStartReportV1`.

### Phase E — observation

Supervisor continues independently:

- drains logs within bounds;
- updates heartbeat operational file atomically;
- observes cancel request and timeout;
- signals owned process tree;
- writes create-new exit observation;
- exits.

A later mutating path such as `run recover`, cancel finalization or a future
explicit foreground observer commits semantic terminal state from the observation.
`run show` and `run list` remain read-only: they may report
`terminal_observation_pending=true` and derived classifications, but they never
finalize exit state or write events. The supervisor itself does not write
semantic event or canonical run record files.

---

## Resume algorithm

1. preserve-only enrollment/profile validation;
2. acquire fence, recover storage/run transactions;
3. authorize `work.run.resume`;
4. load run and all bindings;
5. require resumable state and no live attempt;
6. validate assignment still owns active Ticket/workspace;
7. capture current workspace snapshot;
8. require exact identity match with recorded latest snapshot;
9. build bounded resume `RunInputV1`;
10. create next attempt with incremented number;
11. commit run state `starting`, new attempt and `run.resume_starting` event;
12. use the same supervisor start handshake;
13. commit running state;
14. never change run ID, lease, workspace or Ticket state.

If workspace snapshot is unsupported, bounded out or drifted, reject without
launch and preserve all state. A clean `failed_to_start` with no child identity
may resume without requiring a post-run workspace snapshot, but only after
assignment/workspace/source preflight still matches the original start
preconditions.

---

## Recovery behavior

### Crash before starting transaction

No run state or process exists.

### Crash after starting transaction before supervisor launch

After startup grace, recovery commits `failed_to_start` if no control record,
heartbeat or verified process exists.

### Crash after supervisor launch before started commit

Recovery may adopt only when:

- run/attempt are still `starting`;
- control nonce hash matches;
- heartbeat belongs to same run/attempt;
- supervisor/child process identity is verified, or a verified exit observation
  proves the same child started and then exited;
- workspace and assignment still match.

If an exit observation proves the child started, recovery first completes the
`run.started` semantic transition and then commits the terminal transition in a
separate transaction. Otherwise mark `stale_needs_operator`; do not spawn or
kill.

### Crash after started targets before event

Shared transaction recovery completes `run.started` event or blocks on
ambiguity.

### Supervisor exits without exit observation

If process identity proves child gone, recovery marks `interrupted`; if process
may still be live or identity is mismatched, mark `stale_needs_operator`.

### Exit observation written before terminal commit

Recovery validates observation identity/hash, captures final workspace snapshot
and commits terminal run/attempt record plus exactly one event.

### Cancel requester crashes

The durable `cancel_requested` state and control request let supervisor continue.
Recovery observes process/exit state and finalizes idempotently.

### Resume caller crashes

The new attempt follows the same starting/orphan rules. Recovery never creates a
second attempt automatically.

---

## Error codes

All public JSON errors use the global error envelope. Runner-owned failures use
stable top-level codes; lower-layer cause code is preserved structurally or as a
stable `cause_code=<code>` token.

### Start/precondition

| Code | Meaning |
| --- | --- |
| `run_assignment_not_found` | Lease/prepared assignment missing |
| `run_assignment_not_dispatch_authorized` | Prepared assignment is not startable |
| `run_lease_not_live` | Lease is expired/tombstoned/invalid |
| `run_ticket_not_active` | Ticket no longer matches claimed active revision |
| `run_workspace_not_found` | Bound workspace record/path missing |
| `run_workspace_source_mismatch` | Repository/base/workspace binding changed |
| `run_workspace_dirty_unexpected` | First attempt workspace differs from bound base/pre-start snapshot policy |
| `run_already_exists` | Live or unresolved run already owns assignment/workspace |
| `run_profile_missing` | Profile registry/profile unavailable |
| `run_profile_invalid` | Runner profile schema/config invalid |
| `run_platform_unsupported` | Safe process supervision unavailable |
| `run_lock_timeout` | Repository fence unavailable |
| `run_recovery_failed` | Storage/run recovery ambiguous or failed |

### Start/process/logs

| Code | Meaning |
| --- | --- |
| `run_input_invalid` | Run input cannot be built/validated |
| `run_input_too_large` | Bounded input budget exceeded before launch; no silent truncation |
| `run_supervisor_spawn_failed` | Pulse supervisor could not start |
| `run_supervisor_handshake_timeout` | No verified startup handshake |
| `run_command_spawn_failed` | Codex process launch failed |
| `run_command_not_found` | Configured executable absent |
| `run_process_identity_unavailable` | Safe identity proof unavailable |
| `run_log_open_failed` | Managed log files unavailable |
| `run_timeout` | Run exceeded configured timeout |
| `run_transaction_failed` | Run state/event commit failed |

### Cancel

| Code | Meaning |
| --- | --- |
| `run_not_found` | Run ID missing |
| `run_not_running` | State cannot be cancelled |
| `run_process_not_found` | Recorded process is gone |
| `run_process_identity_mismatch` | PID/process marker does not match |
| `run_cancel_signal_failed` | Verified process group signal failed |
| `run_cancel_timeout` | Process did not stop within policy bounds |
| `run_force_kill_disallowed` | Force escalation prohibited |

### Resume/recover/snapshot

| Code | Meaning |
| --- | --- |
| `run_not_resumable` | Current state/policy disallows resume |
| `run_attempt_in_progress` | Another attempt is live/unresolved |
| `run_workspace_drift` | Current workspace differs from recorded snapshot |
| `run_workspace_snapshot_unsupported` | Snapshot cannot be safely canonicalized |
| `run_native_resume_not_supported` | Caller requested native resume not installed |
| `run_stale_needs_operator` | Safe automated action cannot be proven |
| `run_control_record_invalid` | Supervisor control/observation record invalid |

---

## Security and authority boundaries

- Runner profile is code-execution configuration and must be tracked/reviewed.
- Pulse never uses a shell for adapter launch.
- Program/argv/env are bounded and schema-validated.
- Secrets are not serialized into records/events/log summaries.
- Runtime input/logs may contain repository-sensitive content and are gitignored
  with restrictive permissions where supported.
- Supervisor control nonce is never written plaintext into semantic records.
- Managed paths reject traversal, symlink escape and replacement races where
  platform APIs permit.
- Cancellation signals only verified owned process group/job.
- PID existence is not identity.
- Worker process has no graph-close authority merely because it runs in a bound
  workspace.
- Run actor grants do not imply source capability; prepared assignment
  capabilities remain required.
- Exit code zero is not authority, proof or acceptance.
- Logs are not evidence until explicitly promoted under redaction policy.
- No raw prompt/environment/source diff in semantic events.

---

## Architecture and module ownership

Proposed modules:

```text
src/run.rs
  # Public neutral DTOs, enums, normalization, schemas and fingerprints.

src/process/
  mod.rs
    # Portable supervisor state machine and ProcessPlatform boundary.
  linux.rs
    # Linux process group, /proc identity and signal/tree ownership.
  macos.rs
    # macOS process group, libproc/kernel identity and signal/tree ownership.
  windows.rs
    # Windows suspended spawn, Job Object ownership, creation identity and cancel.
  # No graph/policy/assignment in any process module.

src/kernel/run.rs
  # Cross-domain orchestration: authority, assignment/Ticket/workspace/profile,
  # start/cancel/resume/recover, transactions and events.

src/kernel/run_store.rs
  # Runtime run/attempt/control/input/log/snapshot path and record IO.
  # No Git/process/authority/lifecycle semantics.

src/cli/run.rs
  # Thin args/rendering/delegation.

src/schema/run/run.schema.json
src/schema/run/run-attempt.schema.json
src/schema/run/run-input.schema.json
src/schema/run/workspace-snapshot.schema.json
src/schema/run/runner-profiles.schema.json
src/schema/run/run-start-report.schema.json
src/schema/run/run-cancel-report.schema.json
src/schema/run/run-recovery-report.schema.json
```

Ownership constraints:

- `src/run.rs` may depend on canonical JSON, identity, assignment/work packet
  DTOs and error primitives, not graph store/filesystem/process/policy;
- `src/process/` owns OS process mechanics only and does not know Ticket,
  lease, workspace records or events; any crate added for process groups, job
  objects or signal handling must be reviewed against current `Cargo.toml`
  Rust 1.78/MSRV and platform support before the implementation slice claims
  cross-platform support;
- `src/kernel/run.rs` owns cross-domain choreography and is the only layer that
  combines run state with assignment, graph, source, workspace, policy and
  events;
- `src/kernel/run_store.rs` owns runtime paths/record IO and preserve-only
  listing; it does not launch/kill processes or inspect Git;
- `src/source.rs` owns workspace snapshot canonicalization because it owns Git
  source identity/currentness;
- `src/workspace.rs` remains path/worktree materialization/cleanup owner and
  must not gain run state;
- `src/graph/read` remains pure;
- `src/cli/run.rs` owns no business logic;
- transaction extensions, if necessary, remain domain-neutral in
  `src/storage/transaction.rs`;
- hidden supervisor parsing belongs to `src/process/`/binary adapter, not CLI
  public domain semantics;
- do not add broad `src/runtime/` namespace in this slice.

Public exports:

```rust
pub mod run;
pub mod process;
```

Only stable DTOs and narrow process capability/status types needed by consumers
are public. Supervisor internals, hidden command args, control-file formats and
fixture adapters remain crate-private even if the module itself is exported.

Recommended store API:

```rust
impl JsonGraphStore {
    pub fn start_run(&self, request: StartRunRequest) -> PulseResult<RunStartReportV1>;
    pub fn show_run(&self, id: &str) -> PulseResult<RunViewV1>;
    pub fn list_runs(&self, filter: RunFilter) -> PulseResult<RunListReportV1>;
    pub fn cancel_run(&self, request: CancelRunRequest) -> PulseResult<RunCancelReportV1>;
    pub fn resume_run(&self, request: ResumeRunRequest) -> PulseResult<RunStartReportV1>;
    pub fn recover_runs(&self, actor: ActorRef) -> PulseResult<RunRecoveryReportV1>;
}
```

---

## Testing strategy

Follow existing integration crate conventions and target repository boundary.
Suggested files:

```text
tests/graph.rs
  #[path = "graph/run_model.rs"]
  #[path = "graph/run_cli_contract.rs"]

tests/graph/run_model.rs
tests/graph/run_cli_contract.rs

tests/process.rs
  #[path = "process/run_start_process.rs"]
  #[path = "process/run_cancel_process.rs"]
  #[path = "process/run_resume_process.rs"]
  #[path = "process/run_recovery_process.rs"]
  #[path = "process/run_concurrency.rs"]

tests/process/run_start_process.rs
tests/process/run_cancel_process.rs
tests/process/run_resume_process.rs
tests/process/run_recovery_process.rs
tests/process/run_concurrency.rs

tests/target_repo.rs
  #[path = "target_repo/run_workspace.rs"]
  #[path = "target_repo/run_no_bootstrap.rs"]
```

Use internal fixture adapter executables/scripts that:

- emit controlled stdout/stderr;
- wait on a barrier;
- spawn a child process to verify process-tree cancellation;
- ignore graceful signal when force path is tested;
- exit with selected code/signal;
- modify tracked/untracked/symlink workspace content;
- write a startup marker for crash tests.

Process supervision tests must run natively on `ubuntu-latest`,
`macos-latest` and `windows-latest`; a Linux container does not prove macOS
`libproc` behavior or Windows Job Object ownership. Every Tier-1 platform suite
must cover child/grandchild cancellation, graceful and forced termination,
identity mismatch, timeout, fast exit, supervisor interruption and concurrent
runs.

Never require a real Codex installation for normal test suite. A manual/optional
Codex smoke test may be documented separately and skipped when executable/config
is absent.

---

## Acceptance matrix

### A. DTO/schema/determinism

1. Run, attempt, input, snapshot, profile and report DTOs round-trip with
   `deny_unknown_fields`.
2. Public schemas reject unknown native-thread/mailbox/handoff/proof fields.
3. Canonical JSON contains no floats.
4. Fingerprints exclude themselves and volatile heartbeat only.
5. Set-like fields normalize deterministically.
6. Runtime records validate independently.
7. Profile fingerprint is deterministic and does not expose environment values.

### B. Start happy path

1. A live prepared assignment starts exactly one supervisor and adapter process.
2. Process cwd is the bound workspace.
3. Start returns only after verified process identity and durable `run.started`.
4. Run/attempt/input/profile/assignment/workspace fingerprints agree.
5. Ticket remains `active` at same revision.
6. Lease remains prepared and exclusively bound.
7. Exactly one `run.starting` and one `run.started` event exist.
8. Logs are created under managed runtime path.
9. Prepared assignment bytes remain unchanged with `runner_status=not_started`.
10. No handoff/verification/QA/close state is created.

### C. Start rejection and side effects

1. Missing/non-live/tombstoned lease rejects.
2. Prepared assignment fingerprint/binding mismatch rejects.
3. Ticket revision/status drift rejects.
4. Missing/bad workspace rejects.
5. Unexpected dirty first-attempt workspace rejects.
6. Missing/invalid profile rejects.
7. Unauthorized actor rejects.
8. Unsupported platform rejects before process launch.
9. Duplicate live/unresolved run rejects.
10. Failed preflight creates no run/runtime directories beyond pre-existing state.
11. Spawn failure leaves no live process and records deterministic failed start
    only if starting transaction committed.

### D. Process and logs

1. Exit 0 records exited result but does not close/verify Ticket.
2. Nonzero exit records code and preserves workspace/logs.
3. Signal exit records signal where platform supports it.
4. Timeout follows cancel policy and records `timed_out=true`.
5. Large stdout/stderr are drained, bounded and marked truncated.
6. Output rendering redacts configured sensitive patterns.
7. Events contain no raw prompt/env/log/source diff.
8. Child process tree is owned by process group/job.

### E. Cancellation

1. Graceful cancel stops the verified process tree.
2. Force escalation occurs only after grace and when allowed.
3. `--no-force` never force kills.
4. PID reuse/identity mismatch never kills unrelated process.
5. Cancel requested during starting is honored.
6. Repeated cancel is idempotent.
7. Cancel after known exit is non-destructive.
8. Workspace/logs/lease/Ticket are preserved.
9. Exactly one cancel-request event and one final event are emitted.

### F. Interruption and recovery

1. Crash after starting intent before targets recovers by storage rules.
2. Crash after starting commit before spawn becomes failed-to-start after grace.
3. Crash after supervisor spawn before started commit adopts only with full
   proof, including fast-exit proof from a verified exit observation.
4. Unprovable orphan process becomes stale-needs-operator and blocks duplicate.
5. Crash after running targets before event completes event.
6. Process gone without observation becomes interrupted with derived resume
   availability only when workspace snapshot succeeds.
7. Exit observation before terminal commit completes idempotently.
8. Ambiguous/non-prefix/event mismatch blocks with `run_recovery_failed` and
   lower cause code.
9. Recover never releases/requeues/deletes workspace or starts a new process.
10. No duplicate semantic event after repeated recover.

### G. Resume

1. Interrupted, clean failed-to-start, or policy-resumable exited run resumes as
   attempt N+1 under same run ID.
2. Resume uses same lease/prepared assignment/workspace and active Ticket
   revision.
3. Current snapshot must equal the previous recorded final/interrupted snapshot;
   clean failed-to-start without child identity compares against the original
   pre-start snapshot and preflight instead.
4. Tracked change drift rejects.
5. Untracked content/path/mode drift rejects.
6. Git operation in progress rejects.
7. Unsupported/bounded-out snapshot rejects automated resume.
8. Concurrent resume has exactly one winner.
9. Resume input contains bounded prior context and no full raw logs.
10. Native resume remains explicitly not installed.

### H. Read-only/projection boundaries

1. `run show/list` on enrolled repo without run directory returns empty/not found
   without creating runtime paths.
2. Non-enrolled repo rejects without `.pulse/runtime` creation.
3. Corrupt records are reported invalid/ambiguous with nullable fields.
4. Runtime run state is not persisted in node JSON.
5. Pure graph frontier/read modules import no run modules.
6. Active assignment projection may add run fields without changing pure graph
   DTOs.

### I. Security/authority

1. Public CLI exposes no free-form shell command.
2. Profile/env/path bounds reject unsafe inputs.
3. Start/cancel/resume/recover each require the correct default-deny grant.
4. Assignee capability does not imply authority.
5. Authority does not bypass assignment/workspace/source checks.
6. Process identity mismatch is conservative.
7. Managed path symlink/traversal attempts reject.
8. Logs/prompts/env values do not leak into events.

### J. Architecture and reliability

1. CLI handlers are thin.
2. Process module imports no graph/assignment/policy.
3. Run store imports no Git/process/policy.
4. Kernel run owns cross-domain composition.
5. Storage changes remain domain-neutral.
6. Same-assignment start race has exactly one winner.
7. Different assignments can run concurrently without shared-state corruption.
8. Process/failpoint suites pass under default threading repeatedly.
9. Tracked fixtures remain immutable.
10. Full repo reliability commands pass.
11. MSRV audit proves every new dependency builds on Rust 1.78 or the dependency
    is rejected.
12. Linux and macOS platform tests are separate; no generic Unix claim is made
    from one platform.
13. Windows native Job Object tests prove owned tree cancellation and recovery.
14. Linux, macOS and Windows all pass before Slice 3 completion is claimed.

---

## Implementation sequence

### P2S3-I0 — Prerequisite feasibility spikes before runner coding

These spikes are part of making the draft implementable. They do not implement
public runner behavior and should land before claiming P2S3-I5 or later is
feasible:

1. **Process identity/platform spikes:** prove the shared `ProcessPlatform`
   boundary and native adapters for:
   - Linux process-group ownership and `/proc` boot/start identity;
   - macOS process-group ownership plus public libproc/kernel start identity;
   - Windows suspended spawn, owned Job Object, process creation identity,
     graceful console-group cancellation where available and forceful job-tree
     termination.
   Each adapter must prove PID-reuse protection, child-tree cancellation and
   crash behavior with Rust 1.78-compatible APIs/crates.
2. **Hidden supervisor packaging spike:** prove the installed `pulse` binary can
   re-exec itself as `__run-supervisor` in development, test and packaged
   contexts without a daemon or second artifact, and define fallback errors.
3. **Secure control nonce spike:** choose descriptor-vs-environment transport,
   file permissions and same-user threat model; verify plaintext nonce never
   reaches disk or semantic records.
4. **Bounded log retention spike:** implement/prove prefix+tail or ring
   retention with continuous draining, explicit retained/full hash semantics and
   no unbounded flat raw log.
5. **Workspace snapshot spike:** prove in-place and isolated snapshot identity
   with Pulse-runtime exclusions, binary diff caps, file modes, symlinks, LFS
   pointer behavior, submodule/special-file rejection and huge-file caps.
6. **Runner profile threat-model spike:** finalize executable resolution,
   environment inheritance, redaction defaults and prompt/input confidentiality
   before public config help is written.

### P2S3-I1 — Lock run value contracts

- Add run/attempt/input/snapshot/profile/report DTOs and enums.
- Add schemas, fingerprints, normalization, no-float and deny-unknown tests.
- Add public API compile guards.
- Preserve Slice 1/2 DTOs unchanged.

### P2S3-I2 — Add runner profile registry

- Implement preserve-only profile registry load/validate/fingerprint.
- Add production `codex_process_v1` and internal fixture adapter selection.
- Prove public production profile JSON/help rejects `fixture_process_v1` and any
  other test-only adapter; fixture adapters are injectable only through
  crate-private test APIs.
- Add path/env/timeout/log bounds and no-shell tests.

### P2S3-I3 — Add runtime run store and classification

- Add managed runtime paths and canonical run/attempt/control/input/log/snapshot
  IO.
- Add read-only list/show and recovery classification.
- Enforce no-bootstrap and path safety.

### P2S3-I4 — Implement workspace snapshot identity

- Extend `source.rs` with bounded deterministic tracked diff/status/untracked
  manifest identities.
- Add Pulse-runtime exclusions for in-place workspaces without excluding
  canonical graph/docs/evidence/events changes.
- Add symlink, executable bit, untracked cap, tracked binary diff cap,
  operation-state, LFS pointer, submodule/special-file and drift tests.
- Do not claim replayable archive/evidence semantics.

### P2S3-I5 — Implement low-level process supervisor

- Add portable supervisor core, nonce/control handshake, bounded log draining,
  heartbeat, timeout, cancel request and exit observation.
- Add Linux, macOS and Windows `ProcessPlatform` adapters.
- Add internal fixture adapter and native process-tree tests per Tier-1 platform.
- Add explicit unsupported-platform behavior only for targets outside the
  required Tier-1 matrix or adapters that fail their proof contract.

### P2S3-I6 — Implement run start starting transaction

- Validate assignment/Ticket/workspace/profile/authority.
- Build bounded `RunInputV1` and versioned workflow bootstrap without copying
  Ticket/docs/knowledge content into the prompt.
- Resolve `pulse work packet --lease` against the committed prepared assignment.
- Commit starting run/attempt/input/snapshot + event.
- Add duplicate-start and failed-preflight tests.

### P2S3-I7 — Complete supervisor launch and started commit

- Launch outside fence, validate handshake, commit running state/event.
- Implement failed-start finalization and orphan adoption classification.
- Add crash/failpoint tests across both transactions.

### P2S3-I8 — Implement show/list and exit observation finalization

- Add CLI projections with nullable fields.
- Commit known exit results from supervisor observations.
- Add bounded/redacted log rendering.

### P2S3-I9 — Implement cancel

- Add cancel-request transaction, verified supervisor control, grace/force policy
  and final state commit.
- Add PID reuse/process-tree/idempotency/crash tests.

### P2S3-I10 — Implement resume

- Add resume eligibility/snapshot equality/new attempt/input context.
- Add concurrent resume and workspace drift tests.
- Keep native resume not installed.

### P2S3-I11 — Implement run recover and assignment integration

- Add deterministic repair/report flow.
- Make lease release/recovery block on live/unresolved run.
- Enrich active assignment projection with run state.
- Add orphan/control/log/ambiguous recovery tests.

### P2S3-I12 — Concurrency, platform and side-effect hardening

- Stress same-run start/resume/cancel races.
- Run failpoints at storage mechanical boundaries.
- Assert no forbidden graph/docs/evidence/knowledge mutations.
- Assert fixture immutability and non-enrolled no-bootstrap.
- Run tracked/untracked diff inspection.

### P2S3-I13 — Implementation completion documentation after code only

After implementation and independent verification, update proposal/roadmap
completion evidence. Do not create Pulse self-hosting state.

---

## Roadmap scenarios owned by this slice

This slice advances:

- **#8:** prepared assignment is consumed by an actual single-Agent run;
- **#12:** interruption and workspace-level resume become implementable for the
  local single-Agent run path once this slice lands;
- **#40:** active assignment projection gains runtime run state;
- **#67 analogy/subset:** after local process start, prepared-lease TTL no
  longer governs execution liveness; this does **not** implement Phase 5
  delivery acknowledgement expiry or automatic return to ready;
- **#70 analogy/subset:** a local interrupted Worker process can be resumed as a
  new attempt in the same workspace, without typed blocker/mailbox routing or
  stable native thread resume;
- **Core DoD:** “Codex single-agent run dùng bounded context và có thể
  cancel/resume.”

It does not complete:

- full Ticket path to `verifying|done|rework|blocked`;
- typed handoff;
- developer verification/review/QA receipts;
- proof-driven close gate;
- native independent Codex task transport;
- Orchestration v2 scenarios 66–75 as a whole.

Because this proposal remains draft, the roadmap's current Phase 2 status should
continue to say Slice 3 is the next unimplemented runner/cancel/resume slice
until implementation and verifier commits exist.

---

## Definition of Done

- [ ] Run/attempt/input/snapshot/profile/report DTOs and strict schemas exist.
- [ ] `WorkPacketV1` and `PreparedAssignmentV1` bytes/semantics remain unchanged.
- [ ] A live prepared assignment can start exactly one Codex process adapter.
- [ ] Codex receives a bounded workflow bootstrap and loads the exact committed
      packet through `pulse work packet --lease`; the prompt does not duplicate
      Ticket, docs, QA or knowledge content.
- [ ] Prerequisite spikes prove supervisor packaging, process identity, control
      nonce transport, bounded logs, workspace snapshot and runner profile threat
      model before process implementation claims feasibility.
- [ ] Runner uses no shell and no public arbitrary command input.
- [ ] Start commits durable starting and running states with exactly one event
      per state transition.
- [ ] Supervisor/process identity is stronger than PID alone and separately
      tested on each supported platform.
- [ ] Linux, macOS and Windows adapters pass native process-tree, identity,
      timeout, cancellation and recovery suites.
- [ ] Process tree cancellation is verified and conservative.
- [ ] Timeout and cancel grace/force policy are bounded and audited.
- [ ] Runtime logs and run inputs are bounded, gitignored, runtime-private and
      absent from semantic events.
- [ ] Exit zero does not transition or close Ticket.
- [ ] Ticket remains active; lease/workspace are preserved.
- [ ] Crash between durable starting, spawn and started commit recovers safely.
- [ ] Known exit observations finalize idempotently without duplicate event.
- [ ] Workspace snapshots deterministically detect tracked/untracked drift,
      including mode, symlink, LFS pointer, submodule/special-file and huge-file
      edge cases.
- [ ] Interrupted work resumes as a new attempt only on exact snapshot match;
      clean failed-to-start may resume only after original preflight still
      matches.
- [ ] Native Codex thread resume is not fabricated.
- [ ] Duplicate start/resume races have exactly one winner.
- [ ] `run show/list` are read-only and no-bootstrap.
- [ ] `run recover` applies only safe deterministic repairs.
- [ ] `work release`/lease recovery refuse live or unresolved runs.
- [ ] Platforms outside Linux/macOS/Windows, and any adapter whose identity proof
      is unavailable, fail before launch with `run_platform_unsupported`.
- [ ] Run state is not persisted in graph nodes or pure graph read DTOs.
- [ ] No handoff/verification/QA/close state is created.
- [ ] Non-enrolled repositories reject before runtime bootstrap.
- [ ] Tracked target fixtures remain immutable.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets --quiet -- -D warnings` passes.
- [ ] `cargo test --all-targets` passes under default threading.
- [ ] Process/concurrency/failpoint suites pass repeatedly.
- [ ] `git diff --check` passes.
- [ ] Completion docs are updated only after implementation and verification.

---

## Deferred decisions and explicit follow-ups

The following are deliberately not compatibility promises of Slice 3:

1. Native Codex thread/task lifecycle and stable thread ID mapping.
2. Interactive PTY/tool-call streaming.
3. Typed mailbox delivery/acknowledgement.
4. Agent Registry/presence across independent user-visible tasks.
5. Full replayable dirty-worktree evidence archive.
6. Raw log evidence promotion and retention policy.
7. Worker handoff receipt schema.
8. Verification profile execution and `active -> verifying` gate.
9. Automatic assignment release/requeue after run completion/cancel.
10. Generic runner adapters beyond proven Codex process usage.
11. Repository-relative runner executable resolution and secret-provider-backed
    environment injection.
12. Native daemon/service-manager ownership for run supervision.

Expected next proposal after Slice 3:

```text
Phase 2 — Slice 4: Typed Worker Handoff + Developer Verification Gate
```

That slice should consume `RunRecordV1`, final workspace snapshot, exit result
and log references, produce a source-bound handoff/verification candidate, and
open the gated `active -> verifying` path without yet conflating developer
verification with Story QA qualification.
