# Daemon application decomposition checkpoint

**Status:** Implemented working-tree architecture record.

This record describes the private, behavior-preserving daemon application
movement now present in the working tree. It is an internal Daemon
reorganization, not an Orchestration implementation or a change to runtime
contracts. Core owns repository semantics and proof; Daemon Runtime owns
host-local lifecycle and external process effects; Core never imports daemon.
See [Decision 0005](../docs/decisions/0005-rust-daemon-runtime-control-plane.md)
and [Decision 0006](../docs/decisions/0006-peer-agent-assurance-topology.md).

## Implemented ownership tree

```text
src/daemon/application/
  mod.rs            one DaemonApplication, one StateStore, facade/composition
  effects.rs        external-effect ledger mechanics
  project.rs        project identity and lifecycle
  workspace.rs      host-local workspace lifecycle and worktree effects
  communication.rs  explicit communication grants and mailbox messages
  timeline.rs       durable timeline reads and provider-event persistence
  turn.rs           durable provider-turn protocol
  session.rs        host-local session/process lifecycle
  assignment.rs     complete assignment provisioning/acknowledgement saga
  recovery.rs       startup recovery ordering
  dispatch.rs       transport-neutral authorization/idempotency/routing spine
```

There is no `support.rs`. All child modules are private implementation details;
public daemon paths remain `pulse::daemon::application::DaemonApplication` and
`pulse::daemon::DaemonApplication`. The single daemon integration crate remains
[`tests/daemon.rs`](../tests/daemon.rs).

## Responsibility and change reason

| Module | One-sentence responsibility and reason to change |
| --- | --- |
| [`mod.rs`](../src/daemon/application/mod.rs) | Composes one facade and one store; it changes only when application composition or stable entrypoints change. |
| [`effects.rs`](../src/daemon/application/effects.rs) | Records, updates, classifies, and fences durable external effects; it changes when ledger mechanics change, never to own caller I/O ordering or compensation. |
| [`project.rs`](../src/daemon/application/project.rs) | Owns daemon project identity/lifecycle and lookup; it changes when project-scoped host lifecycle behavior changes. |
| [`workspace.rs`](../src/daemon/application/workspace.rs) | Owns host-local workspace isolation and concrete worktree effects; it changes when workspace lifecycle or Git effect ordering changes. |
| [`communication.rs`](../src/daemon/application/communication.rs) | Owns explicit communication grants, mailbox messages, and atomic events; it changes when runtime communication policy or message durability changes. |
| [`timeline.rs`](../src/daemon/application/timeline.rs) | Owns ordered timeline reads/subscriptions and provider notification persistence/requeue; it changes when durable event or cursor behavior changes. |
| [`turn.rs`](../src/daemon/application/turn.rs) | Owns the reusable provider-turn protocol and its guard/effect ordering; it changes when provider request correlation or turn commit classification changes. |
| [`session.rs`](../src/daemon/application/session.rs) | Owns host-local session/process create, attach, resume, interrupt, close, archive, and logs; it changes when lifecycle, handle, or no-false-idle behavior changes. |
| [`assignment.rs`](../src/daemon/application/assignment.rs) | Owns the complete reservation-to-delivery acknowledgement and typed Core activation saga; it changes only when that crash-sensitive saga contract changes. |
| [`recovery.rs`](../src/daemon/application/recovery.rs) | Owns epoch/process/session/effect startup classification before assignment reconciliation; it changes when host recovery order or fail-closed uncertainty handling changes. |
| [`dispatch.rs`](../src/daemon/application/dispatch.rs) | Owns transport-neutral authorization, replay/idempotency policy, and use-case routing; it changes when request policy or facade routing changes. |

The dependency direction is CLI/local protocol/MCP transport to one
`DaemonApplication` facade, then private use-case modules, then one
`StateStore`, `ProcessOwner`, and provider registry. Daemon may call typed public
Core reservation/proof gates; it does not duplicate Core repository semantics.
Future Orchestration may compose Core and Runtime later, but remains
unimplemented and owns neither side's semantics.

## Durable rules preserved by the movement

- Dispatch keeps request authorization before mutating-key validation,
  idempotency lock, cached fingerprint and principal validation, shutdown check,
  routing, post-mutation failpoint, and response-cache persistence.
- `effects.rs` owns ledger record/update/replay-conflict and uncertainty
  mechanics only. It does not own provider/Git I/O ordering, compensation, or a
  generic effect/saga engine.
- Timeline state mutation and its event remain one store transaction; drained
  provider notifications are requeued when persistence or its failpoint fails.
- The turn `session-operation` guard remains held from validation through durable
  intent, provider I/O, acknowledgement, and final session/timeline commit.
- Assignment remains one intact saga: reservation, workspace/session setup,
  durable delivery intent, provider I/O, acknowledgement, and typed Core proof.
- Recovery classifies and durably records process/session/effect uncertainty
  before invoking assignment-owned reconciliation. Uncertain outcomes fail
  closed: no blind resend, release, or adoption.
- Comments and module docs explain only non-obvious WHY, ordering, idempotency,
  or recovery invariants; there is no hard file-size quota.

## Phase A debt taxonomy and inventory

Observed sizes below are Phase A snapshot evidence, not current refactor
targets. The reasons record why each boundary was deferred or resolved.

| Area | Evidence and current disposition/reason |
| --- | --- |
| Application production cohesion | **4,652 production LOC** in the original application snapshot; resolved by the flat private tree, while `mod.rs` remains composition/facade code so splitting the authority does not create a second runtime owner. |
| Application contract test organization | **3,483 test LOC** in the original `application_contract.rs`; resolved by six private child files under `tests/daemon/application_contract/` while retaining one `tests/daemon.rs` crate and unchanged test names/assertions. |
| Packet | **4,003 LOC; tests began at 2,281 LOC**. Deferred because packet construction and contract shaping belong to kernel/Core ownership, not daemon application lifecycle. |
| Source | **2,319 LOC; tests 1,806 LOC**, with a later snapshot seam. Deferred because source-binding currentness intentionally couples Git mechanics with status policy in `src/source.rs`; that later seam was not part of this boundary. |
| `work_packet` | **2,141 LOC; tests 1,312 LOC** with a cohesive schema. Deferred because it is a public Core value/contract surface whose schema and fingerprint ownership are outside this daemon movement. |
| Process | **1,427 LOC; tests 1,235 LOC**. The ProviderOutputDispatcher versus ProcessOwner is a later seam, deferred because splitting it now carries synchronization risk. Timing-sensitive subprocess and multi-process suites retain the isolated `tests/process` conventions. |
| Readiness | **1,239 LOC**, cohesive. Deferred because readiness is a pure graph/kernel evaluator boundary, not a host-local daemon use case. |
| Reservation | **1,754 LOC** in `tests/graph/reservation.rs`, a test-organization candidate. Deferred because Core owns reservation semantics and proof; assignment calls typed public gates without creating a repository wrapper. |

These are scope/debt classifications, not invitations to add service layers or
new abstractions.

## Four-peer debate record

Four anonymized peer reviews agreed that the runtime needs one concrete facade
and store, private flat use-case ownership, Core/runtime directionality,
behavior-preserving moves, durable uncertainty, and focused architecture/test
guards. They disagreed on the following real choices:

- **Assignment-first vs callees-first:** one view favored moving the
  crash-sensitive assignment saga first; another favored stabilizing its
  workspace/session callees first. The accepted implementation sequence was
  boundary inventory and guards, narrow effects, complete assignment saga
  early/intact, then project/workspace, communication, timeline, turn, session,
  recovery, dispatch, and finally the test split. Descendant privacy minimized
  visibility churn during the crash-sensitive move, and the AGENTS rule requires
  moving the saga intact before phase-specific nesting.
- **Turn separate vs session:** one view kept provider turns inside session
  lifecycle; another separated the reusable protocol. The accepted tree keeps
  `turn.rs` separate while leaving lifecycle/process ownership in `session.rs`.
- **Dispatch conditional:** one view would leave routing in the facade unless
  it became unreadable; the accepted evidence showed a stable seam, so
  `dispatch.rs` owns the policy/routing spine while public facade entrypoints
  remain in `mod.rs`.
- **Support rejected:** a generic `support.rs` namespace was rejected because
  it would hide ownership; helpers stay with their owning module or the facade.
- **Effects narrowed:** ledger mechanics were extracted, but caller I/O order,
  compensation, and saga meaning stayed with use cases.
- **Recovery ordering:** recovery owns host startup classification and invokes
  assignment-owned reconciliation only after durable uncertainty and epoch
  events commit.

Root accepted the evidence-based sequence. Falsifiers/brakes remain: a move
would stop if it required behavior edits, duplicate executable implementations,
broad visibility, changed lock/failpoint order, a second facade/store, or a new
abstraction. Public-path compilation, architecture inventory checks, focused
contract tests, and default-thread behavior tests are the evidence gates.

## Actual migration and test tree

Production movement followed boundary inventory/guards, narrow effects,
complete assignment saga early/intact, project/workspace, communication,
timeline, turn, session, recovery, and dispatch while preserving one
`DaemonApplication` and one `StateStore`. The test split followed afterward;
the test root now contains shared fixture wiring and six private modules:

```text
tests/
  daemon.rs
  daemon/
    application_contract.rs               shared helpers + child declarations
    application_contract/
      dispatch_contract.rs
      project_workspace_contract.rs
      session_lifecycle_contract.rs
      turn_timeline_contract.rs
      assignment_contract.rs
      recovery_contract.rs
```

The session-lifecycle test child was 1,349 LOC at the decomposition checkpoint
and is 1,371 LOC after the later bounded failpoint-observation synchronization
fix. It remains cohesive because keeping close/resume/process-effect invariants
together is safer than fragmenting them by phase.

## Verification record

Observed focused verification includes the per-wave daemon/graph/public-path
checks accepted during the source movement, **118 passing library tests**, and
**48/48 application-contract tests in each of two default-thread runs**. The
full daemon crate was observed at **50/54 passed**; the four failures were
sandbox local-socket startup timeouts (`daemon_start_timeout`) in the local
protocol tests, not application-contract failures:

- `concurrent_daemon_start_has_exactly_one_owner`
- `local_protocol_rejects_mismatch_before_mutation_and_shutdowns_cleanly`
- `local_protocol_rejects_unknown_required_capability_before_mutation`
- `malformed_client_does_not_bypass_shutdown_cleanup`

The focused close/requeue test later passed five default-thread repetitions.
Its fixed sleep was replaced by bounded polling through the public timeline
request path until `injected_failpoint` was observed; this changed test
synchronization only, not production behavior or the asserted invariant.
Formatting and diff checks passed. Full Clippy, all-targets, and complete
repository gates have not been claimed in this documentation record.

No public, protocol, persistence-schema, error, lock, failpoint, or runtime
behavior contract changed in the source movement.

## Authoritative extension workflow

1. Identify the protocol request and its authorization/replay order.
2. Choose one existing owner or add one private flat child only when a distinct
   responsibility/change reason is evidenced.
3. Preserve one facade/store and atomic state/event or durable-intent boundaries.
4. Update the explicit daemon architecture inventory and module rustdoc.
5. Add or move focused tests under the existing `tests/daemon.rs` crate without
   renaming tests or duplicating fixtures.
6. Run default-thread focused tests, `cargo fmt --check`, architecture/public
   path checks, Clippy, and all-targets as required by the repository gate.

This record is authoritative for the implemented checkpoint; later changes must
update it rather than reviving the obsolete proposed tree or status.
