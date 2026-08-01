# Phase 2: Rust Daemon Realignment Implementation Gap

> **Current status (2026-08-01):** Phase 2 realignment implementation is
> substantially present and focused behavior-tested, but acceptance remains
> **Verifying**, not complete. G9 HTTP/WebSocket and desktop adapters remain
> explicitly later client work; the shared MCP adapter and policy-gated session
> mailbox are implemented. Bootstrap delivery now persists a deterministic
> delivery intent (delivery id, exact payload and provider request correlation)
> before provider I/O; restart recovery fails closed in a durable
> `DeliveryPending`/uncertain state when the provider outcome cannot be proven —
> it never blindly re-sends and never releases a possibly-valid assignment.
> Typed acknowledgement remains separate; activation is never inferred from
> provider state. The complete Phase 3 close gate is not installed, no
> full-suite-green result is claimed here, and daemon whole-snapshot boundedness,
> Windows native acceptance coverage and `repository_id` naming remain
> deferred/risk items. Reservation TTL recovery currently covers
> `Reserved`/`Acknowledged`, not `Active`.
>
> **Purpose:** reconcile the implemented Phase 2 runner prototype with the
> accepted Core + Rust Daemon architecture.
>
> **Normative owners:** [runtime harness](../pulse-reboot/04-runtime-harness.md),
> [cross-Agent coordination](../pulse-reboot/05-cross-agent-coordination.md),
> [roadmap](../pulse-reboot/08-implementation-roadmap.md), and
> [Decision 0005](../docs/decisions/0005-rust-daemon-runtime-control-plane.md).
>
> **Replacement policy:** pre-release, one-way replacement. Do not preserve a
> compatibility mode, dual-write old and new runtime state, or keep the hidden
> supervisor as a second launcher after daemon acceptance passes.

## 1. Why this realignment exists

The current Phase 2 implementation proved several hard local-runtime mechanics:
atomic assignment preparation, worktree isolation, bounded logs, process
identity, cancellation, recovery and durable attempt records. Its ownership
boundary is nevertheless wrong for the target product:

- CLI/Core code creates and owns run attempts;
- one hidden supervisor is started per run;
- workspace binding is coupled to assignment preparation;
- provider configuration is modeled as runner profiles;
- runtime records live beside repository state and are composed through Core;
- there is no long-lived Project, Workspace, Session, Provider or Timeline
  manager shared by CLI, desktop, HTTP/WebSocket and MCP clients.

Adding a daemon around that model would preserve two runtime authorities.
Instead, reuse the proven mechanisms behind a new daemon-owned boundary and
delete the obsolete public contracts.

## 2. Target boundary

```text
CLI / Desktop / HTTP+WS / MCP
               |
        versioned local protocol
               |
        long-lived Rust Daemon
               |
   +-----------+-----------+-----------+
   |           |           |           |
Project     Workspace    Session     Provider
Registry    Manager      Manager     Registry
                           |             |
                           +------ ProcessOwner
                           |
                     Runtime Timeline
               |
        Pulse Core application API
               |
 graph / docs / knowledge / readiness / reservation / evidence / gates
```

Core is independently usable for repository queries and mutations. Runtime
clients never bypass the daemon to start or control provider processes. The
daemon never edits work status as if runtime liveness were semantic proof.

## 3. Current implementation disposition

### 3.1 Keep in Core

These responsibilities already align with the target and should retain their
public semantics:

| Area | Current examples | Required action |
|---|---|---|
| Work identity and graph | `src/graph/`, node/edge schemas | Keep |
| Readiness and frontier | `src/kernel/frontier.rs`, graph read models | Keep runtime-independent |
| Work packet | `src/kernel/packet.rs` | Keep as exact Core execution contract |
| Reservation and authority | `src/kernel/assignment.rs`, policy | Keep reservation; remove provisioning ownership |
| Evidence and lifecycle gates | `src/evidence/`, lifecycle validation | Keep proof-driven transitions |
| Docs and knowledge | `src/docs/`, `src/knowledge/` | Keep offline and Core-owned |
| Source identity | `src/source.rs` | Keep repository identity/checking API |
| Generic durability | `src/storage/` | Reuse primitives without importing runtime domains |

### 3.2 Salvage behind daemon ownership

These files contain useful mechanics but must not remain the public ownership
boundary:

| Current area | Reusable mechanics | Target owner |
|---|---|---|
| `src/process.rs` | process descriptor, handshake, identity, tree cancel, logs | `daemon::process` |
| `src/workspace.rs` | safe path checks, worktree create/cleanup, source validation | `daemon::workspace` plus narrow Core source queries |
| `src/run.rs` | attempt/error vocabulary worth reviewing | `daemon::session` and `daemon::timeline` |
| `src/kernel/run_store.rs` | recovery and atomic-record lessons | daemon persistence repositories |
| `src/kernel/runner.rs` | orchestration order and failure cases | daemon assignment/session application service |
| `src/cli/run.rs` | user-facing intent and output cases | thin daemon client commands |
| run/process integration tests | failure scenarios and platform fixtures | daemon protocol/application/native suites |

Salvage means move or re-express behavior with new contracts. It does not mean
wrap existing `RunRecordV1` and expose it indefinitely.

### 3.3 Replace and delete

The following concepts are prototype contracts and must disappear after their
replacement is proven:

- hidden `pulse` supervisor subcommand as the lifecycle authority;
- one supervisor process per attempt;
- public `RunRecordV1`/`RunAttemptV1` as the primary Agent-session model;
- runner-profile registry as the provider abstraction;
- Core-owned runtime run store and run recovery service;
- assignment workspace record as the canonical Workspace identity;
- direct CLI-to-runner launch/cancel/resume paths;
- repository-local runtime records treated as the only cross-client timeline;
- any transition that infers semantic work state from process exit.

Do not add translation layers solely to keep these names alive.

### 3.4 Current capability and evidence matrix

This is the reconciled current snapshot. `Implemented` means the capability is
present in the current tree; `Behavior-tested` identifies focused/source
evidence that was inspected for this snapshot. Neither label is a claim that
the complete Phase 3 close gate or full test suite is green.

| Capability | Current disposition | Source evidence | Test/evidence path |
|---|---|---|---|
| Daemon ownership, local protocol and runtime application boundary | Implemented; behavior-tested | [`src/daemon/application/mod.rs`](../src/daemon/application/mod.rs), [`src/daemon/protocol/mod.rs`](../src/daemon/protocol/mod.rs), [`src/daemon/transport/local.rs`](../src/daemon/transport/local.rs) | [`tests/daemon/application_contract.rs`](../tests/daemon/application_contract.rs), [`tests/daemon/local_protocol.rs`](../tests/daemon/local_protocol.rs) |
| Project/Workspace/Session/Provider/ProcessOwner/timeline ownership | Implemented; behavior-tested | [`src/daemon/project/mod.rs`](../src/daemon/project/mod.rs), [`src/daemon/workspace/mod.rs`](../src/daemon/workspace/mod.rs), [`src/daemon/session/mod.rs`](../src/daemon/session/mod.rs), [`src/daemon/provider/mod.rs`](../src/daemon/provider/mod.rs), [`src/daemon/process/mod.rs`](../src/daemon/process/mod.rs), [`src/daemon/timeline/mod.rs`](../src/daemon/timeline/mod.rs) | [`tests/daemon/application_contract.rs`](../tests/daemon/application_contract.rs) |
| Assignment delivery intent, external-effect uncertainty and delayed provider events | Implemented; behavior-tested | [`src/daemon/assignment/mod.rs`](../src/daemon/assignment/mod.rs), [`src/daemon/persistence/mod.rs`](../src/daemon/persistence/mod.rs), [`src/daemon/process/mod.rs`](../src/daemon/process/mod.rs) | [`tests/daemon/application_contract.rs`](../tests/daemon/application_contract.rs) (`acknowledged_turn_commit_failure_is_unknown_and_retry_is_blocked`, `delayed_provider_event_is_durable_without_session_or_timeline_read`) |
| Core reservation, typed acknowledgement and proof-driven verification state | Implemented; behavior-tested; `Passed` remains `Verifying` until close gate | [`src/kernel/reservation.rs`](../src/kernel/reservation.rs), [`src/kernel/completion.rs`](../src/kernel/completion.rs), [`src/daemon/assignment/mod.rs`](../src/daemon/assignment/mod.rs) | [`tests/daemon/application_contract.rs`](../tests/daemon/application_contract.rs), [`tests/graph/lifecycle.rs`](../tests/graph/lifecycle.rs) |
| Hidden supervisor/direct provider launcher removal and bootstrap/read-side-effect boundaries | Implemented; source/architecture evidence | [`src/bin/pulse.rs`](../src/bin/pulse.rs), [`src/daemon/mod.rs`](../src/daemon/mod.rs), [`src/graph/store/bootstrap.rs`](../src/graph/store/bootstrap.rs) | [`tests/graph/architecture_guards.rs`](../tests/graph/architecture_guards.rs), [`tests/public_api_contract.rs`](../tests/public_api_contract.rs) |
| Reservation TTL recovery for `Reserved`/`Acknowledged` | Implemented; `Active` TTL recovery not implemented | [`src/daemon/assignment/mod.rs`](../src/daemon/assignment/mod.rs), [`src/kernel/reservation.rs`](../src/kernel/reservation.rs) | [`tests/daemon/application_contract.rs`](../tests/daemon/application_contract.rs), [`tests/graph/reservation.rs`](../tests/graph/reservation.rs) |
| Daemon whole-snapshot boundedness, Windows native acceptance coverage and `repository_id` naming | Deferred/risk or still unverified | Current source/test coverage is not sufficient to close these claims | No full acceptance evidence recorded here |

## 4. Historical gap inventory (superseded current-state claims)

The table below preserves the proposal's original gap snapshot for historical
traceability. Its `Current state` column is not a current repository status
claim; use §3.4 for the reconciled status.

| Capability | Current state | Gap |
|---|---|---|
| Daemon host | Absent | Long-lived lifecycle, singleton discovery, local endpoint, shutdown/restart |
| Protocol | Absent | Version handshake, request ID, idempotency, errors, capability negotiation |
| Project Registry | Absent | Stable `project_id`, root identity, open/archive/list |
| Workspace Manager | Assignment-bound worktree helper | Stable `workspace_id`, multiple isolation modes, archive/restore, many sessions |
| Session Manager | Run/attempt records | Stable `session_id`, lifecycle, parentage, archive, attach/recover |
| Provider Registry | Runner profiles | Capability-oriented providers; Codex-native first |
| ProcessOwner | Per-run supervisor | Daemon-owned managed-process ledger and platform adapters |
| Timeline | Events/log files only | Epoch/sequence cursor, durable paging, live subscription, replay |
| Permissions | Core authority only | Client/tool/session runtime authorization without duplicating Core policy |
| Tool catalog | Absent | Shared application behavior for native tools, MCP, CLI and HTTP |
| Assignment saga | Core transaction stops at prepared run | Reservation/provision/delivery/ack/activation with compensation |
| Client adapters | CLI runner only | CLI daemon client, then HTTP/WS, MCP and desktop |
| Operator recovery | Run-specific recovery | Project/workspace/session/process/timeline/saga recovery |

## 5. Target module layout

Names can change during implementation, but ownership and dependency direction
must remain recognizable:

```text
src/
  daemon/
    mod.rs                 host lifecycle and composition root
    protocol/              versioned request/response/event envelopes
    application/           use cases; no transport-specific behavior
    project/               Project Registry
    workspace/             Workspace Manager and isolation adapters
    session/               Session Manager and lifecycle
    provider/
      mod.rs               provider contract and registry
      codex.rs             first native provider
    process/               ProcessOwner and OS adapters
    timeline/              authoritative runtime event log and cursors
    assignment/            Core/Daemon provisioning saga
    permissions/           runtime authorization
    persistence/           daemon repositories and recovery
    transport/
      local.rs             initial local client protocol
      websocket.rs         later live remote/client adapter
      mcp.rs               later Agent-facing adapter
  cli/
    daemon.rs              start/status/stop/doctor
    project.rs
    workspace.rs
    session.rs
```

`daemon::application` may depend on public Core services. Core graph, evidence,
docs, knowledge and pure read modules must not import daemon modules.

## 6. Canonical state ownership

| State | Canonical owner | Durable form |
|---|---|---|
| Ticket contract/revision | Core | repository work graph |
| Readiness and packet fingerprint | Core | canonical inputs + derived projection |
| Reservation/lease | Core | repository assignment state |
| Project | Daemon | daemon runtime store |
| Workspace and isolation handle | Daemon | daemon runtime store |
| Session and provider handle | Daemon | daemon runtime store |
| Managed process identity | Daemon | daemon process ledger |
| Runtime timeline | Daemon | ordered daemon event store |
| Delivery/acknowledgement | Daemon saga | daemon runtime store, correlated to Core IDs |
| Handoff/verification/close proof | Core | immutable evidence and graph lifecycle |

Cross-boundary records carry opaque IDs and fingerprints. They do not duplicate
the other side's full object or silently rebuild it from newer state.

## 7. Delivery sequence

Each step must leave one authoritative path. Temporary compile-time movement is
allowed within a branch; merged behavior must not expose competing launchers.

### G0 - Freeze contracts and characterize reusable behavior

- Mark the historical Slice 3 proposal as implemented prototype evidence.
- Add architecture guards for Core not depending on `daemon`.
- Inventory current runner tests by behavior: retain, rewrite or delete.
- Capture golden error/lifecycle cases worth preserving.

**Exit:** every current runtime surface has an explicit disposition in this
document or a linked implementation ticket.

### G1 - Daemon skeleton and local protocol

- Add daemon composition root and local endpoint discovery.
- Define handshake, protocol version, request ID, idempotency key, error
  envelope and capabilities.
- Implement `pulse daemon start|status|stop|doctor`.
- Make startup/restart deterministic and reject incompatible clients clearly.

**Exit:** two CLI invocations address the same daemon instance; restart recovery
is tested; Core commands still run with no daemon.

### G2 - Project and Workspace ownership

- Implement stable opaque Project and Workspace identities.
- Move safe local/worktree provisioning behind Workspace Manager.
- Separate workspace create/open/archive/restore from session creation.
- Prove multiple Sessions and tools can reference one Workspace.
- Keep Git/repository validation through narrow Core/source APIs.

**Exit:** no new code creates an assignment-owned worktree directly from Core.

### G3 - Session and Codex-native provider

- Define Session lifecycle and orthogonal archive posture.
- Define provider capability contract and registry.
- Implement Codex-native create/attach/send/observe/interrupt/close.
- Persist Pulse `session_id` separately from provider-native handles.
- Normalize provider events without erasing provider-specific detail.

**Exit:** a Codex Session survives CLI disconnect/reconnect and can be attached
through the same stable Pulse identity.

### G4 - Daemon ProcessOwner and native platforms

- Move proven process mechanics from the hidden supervisor into daemon
  ownership.
- Add managed-process ledger, bounded output and conservative recovery.
- Prove graceful interrupt, forced tree cancel, timeout and PID-reuse defense.
- Run native Linux, macOS and Windows suites.

**Exit:** no provider/helper process is launched outside ProcessOwner; failed
interrupt never reports false idle.

### G5 - Authoritative timeline and subscriptions

- Define event envelope, daemon epoch, sequence and cursor.
- Persist lifecycle, provider, process, tool and saga events.
- Implement paged catch-up and live subscription.
- Prove reconnect without gaps or double-applying state.

**Exit:** a client can reconstruct current runtime state from snapshot plus
timeline cursor without chat history.

### G6 - Core/Daemon assignment saga

- Reserve exact Ticket revision and packet in Core.
- Provision/bind Workspace and create Session idempotently.
- Deliver versioned workflow bootstrap.
- Record delivery separately from typed acknowledgement.
- Activate only through the Core gate.
- Implement compensation for every failure boundary and restart recovery.

**Exit:** fault injection at each step yields either one recoverable assignment
or a released reservation, never duplicate active ownership.

### G7 - Handoff and proof-driven completion

- Accept typed Worker handoff correlated to assignment/session/source.
- Run required developer verification and evidence validation.
- Transition `active -> verifying -> done|rework|blocked` only through Core.
- Keep process exit and provider idle as runtime observations only.

**Exit:** one standalone Ticket completes end to end with valid receipts; a
zero-exit provider process without proof cannot close it.

### G8 - CLI replacement and old-path deletion

- Route `run`, cancel, resume, inspect and logs through daemon application APIs.
- Rename user-facing concepts to Workspace/Session where run/attempt wording is
  no longer accurate.
- Delete hidden supervisor, runner profiles, Core run store and direct launch.
- Delete obsolete schemas/tests instead of preserving adapters.
- Update `AGENTS.md` source map and public command docs to the final tree.

**Exit:** repository search and architecture tests show exactly one runtime
lifecycle authority and one provider launch path.

### G9 - Additional clients and orchestration

- Expose HTTP/WebSocket and MCP as adapters over the same daemon use cases.
- Add desktop client only after protocol contracts stabilize.
- Compose independent Worker/Reviewer/QA sessions using the assignment saga.
- Add typed mailbox, ownership tree and policy-governed communication graph.

**Exit:** all clients observe identical session/timeline semantics, and
multi-Agent work does not weaken Core authority or evidence gates.

## 8. Acceptance matrix

| ID | Scenario | Required result |
|---|---|---|
| DA-01 | Core query while daemon is stopped | Succeeds offline |
| DA-02 | Concurrent daemon start | Exactly one owner; other caller attaches or gets deterministic error |
| DA-03 | Client/daemon protocol mismatch | Fails before mutation |
| DA-04 | Repeat request with same idempotency key | Same result; no duplicate resource |
| DA-05 | Workspace create without Session | Workspace remains valid and inspectable |
| DA-06 | Two Sessions in one Workspace | Distinct identities and lifecycle; shared container explicit |
| DA-07 | Provider handle changes on resume | Pulse `session_id` remains stable and history is correlated |
| DA-08 | Cancel transport fails | Session does not become falsely idle/closed |
| DA-09 | Daemon dies with managed process alive | Restart adopts only with valid identity or fails closed |
| DA-10 | PID reused | Cancellation refuses unrelated process |
| DA-11 | Client disconnects during events | Cursor catch-up reconstructs all authoritative events |
| DA-12 | Live event duplicated | Client can de-duplicate by epoch/sequence/event ID |
| DA-13 | Core reserve succeeds, workspace fails | Saga compensates or exposes recoverable pending state |
| DA-14 | Session created, acknowledgement missing | No semantic activation; retry is idempotent |
| DA-15 | Ticket revision changes before activation | Core gate rejects stale assignment |
| DA-16 | Provider exits zero without handoff proof | Ticket remains active/verifying, never done |
| DA-17 | Daemon restart during verification | Core proof state and daemon runtime recover independently |
| DA-18 | CLI and MCP invoke same tool | Same authorization, idempotency and semantic result |
| DA-19 | Runtime parent sends child message | Allowed only by communication policy, not parentage alone |
| DA-20 | Old launcher search after G8 | No executable hidden-supervisor/direct-run path remains |

## 9. Deletion checklist

G8 is not complete until all items are true:

- [x] no hidden supervisor subcommand remains;
- [x] no CLI path spawns Codex/provider directly;
- [x] no Core module owns Session/process lifecycle;
- [x] no runner-profile schema is public or loaded;
- [x] no assignment workspace record acts as stable Workspace identity;
- [x] no run/attempt record is the canonical Agent-session contract;
- [x] no runtime event store competes with the daemon timeline;
- [x] no tests exercise obsolete behavior except explicitly archived fixtures;
- [x] no docs describe the prototype as normative;
- [x] no compatibility flags, dual writes or legacy schema translators remain.

## 10. Verification bar

Every implementation gap closes with:

1. focused unit and integration tests for the new owner;
2. architecture guards for dependency direction and single launch authority;
3. crash/fault injection around persistence and external process boundaries;
4. native process tests on Linux, macOS and Windows;
5. protocol contract tests across CLI and at least one additional adapter;
6. `cargo fmt --check`;
7. `cargo clippy --all-targets --quiet -- -D warnings`;
8. `cargo test --all-targets` at default threading;
9. source search proving the replaced surface is deleted;
10. updated owner docs, ADRs and `AGENTS.md` matching the implemented tree.

The implementation may land in small vertical slices, but Phase 2 realignment
is complete only when the daemon is the sole runtime authority and the old
runner architecture has been removed.
