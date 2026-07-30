# Decision 0005: Rust daemon runtime control plane

## Status

Accepted.

## Context

Pulse Core already owns durable repository semantics: work contracts, graph
relations, documentation, knowledge, readiness, reservations, evidence and
proof-gated lifecycle transitions. Runtime concerns have different ownership
and failure modes:

- projects and workspaces outlive individual commands;
- one workspace may host several Agent sessions, terminals and services;
- providers expose different session, interruption and tool capabilities;
- processes must remain observable and cancellable after the invoking CLI exits;
- clients need both live updates and durable catch-up;
- daemon restart must recover without treating chat memory as state.

The implemented Slice 3 runner placed workspace, run, attempt and hidden
supervisor state behind CLI/Core paths. That was a useful process-control
prototype, but it is not the target architecture. Keeping it beside a new
daemon would create two lifecycle authorities.

Paseo provides the relevant structural lesson: a long-lived daemon owns
workspace, session, provider and timeline runtime while clients remain adapters.
Pulse must adopt that ownership model without copying Paseo's product semantics
or introducing a second implementation language.

## Decision

Pulse uses one long-lived **Rust daemon** as the runtime control plane. Pulse
Core and Pulse Daemon ship in the same `pulse` executable and communicate
through a versioned local protocol.

### Ownership boundary

Pulse Core owns:

- work graph and semantic lifecycle;
- documentation and knowledge truth;
- readiness, packet construction and authority policy;
- exclusive assignment reservation;
- evidence, receipts and close/rework/block decisions.

Pulse Daemon owns:

- Project Registry;
- Workspace Manager;
- Session Manager;
- Provider Registry and capability catalog;
- provider/helper process ownership;
- runtime timeline and live subscriptions;
- client transport, permissions and runtime recovery;
- assignment provisioning saga state.

The daemon may call Core application services. It does not duplicate or replace
Core work truth.

### Stable identities

`project_id`, `workspace_id`, `session_id` and provider-native handles are
distinct opaque identities. A Workspace is a stable container; a local
directory or Git worktree is one isolation implementation. Workspace creation
and Session creation are separate operations, and one Workspace can contain
multiple Sessions.

A Session persists at least its Pulse identity, project, workspace, provider,
provider-native handle, runtime parentage, lifecycle, archive posture and
timeline cursor. Runtime parentage supports control and cleanup only; it does
not imply work-graph authority.

### Session and cancellation semantics

The lifecycle is:

```text
initializing -> idle <-> running -> error -> closed
```

Archive is orthogonal to lifecycle. Cancellation does not report success or
move a Session to idle merely because Pulse sent a signal. It commits only
after provider acknowledgement or an observed terminal event; otherwise the
Session remains conservatively non-idle with an actionable error.

### Provider and tool boundary

The daemon exposes a capability-oriented provider contract rather than forcing
all providers into a lowest-common-denominator command line:

- create, attach, send and observe a session;
- interrupt and close with provider-specific acknowledgement;
- normalize provider events into Pulse runtime events;
- advertise capabilities and provider-native tools.

Codex-native is implemented first. Claude-native and ACP-generic follow only
when their actual contracts are proven.

The tool catalog belongs to the daemon application layer. CLI, HTTP/WebSocket,
MCP and native client integrations are adapters over the same authorization,
idempotency and behavior.

### Process ownership

The daemon's `ProcessOwner` retains the useful mechanics proven by Slice 3:
descriptor/nonce validation, startup handshake, heartbeat, bounded logs,
timeout, process identity, graceful interruption and forceful whole-tree
termination. It owns each managed process from spawn and records it in a
recoverable ledger.

Linux, macOS and Windows are Tier-1 targets:

- Linux uses a dedicated process group and `/proc` boot/start identity.
- macOS uses a dedicated process group and a proven public process-start marker
  plus executable identity; formatted `ps` output is not identity proof.
- Windows assigns the child to an owned Job Object before execution continues,
  records creation/job/executable identity and uses the Job Object for
  whole-tree termination.

Insufficient identity proof fails closed before launch.

### Timeline and recovery

Live WebSocket/PubSub deltas provide immediacy. Correctness comes from an
authoritative, paged timeline ordered by daemon epoch and sequence cursor.
Reconnect always resumes from a cursor; presence or successful delivery is not
a work-state gate.

Core reservation and daemon provisioning form an explicit idempotent saga:

```text
reserve exact Core assignment
  -> create or bind Workspace
  -> create Session
  -> deliver workflow bootstrap
  -> receive typed acknowledgement
  -> activate through Core gate
```

Every external boundary records an idempotency key and compensation/recovery
posture. There is no pretend distributed transaction across repository files
and provider processes.

### Replacement rule

No compatibility mode is required during pre-release implementation. Once the
daemon path passes the replacement acceptance suite, Pulse deletes the hidden
per-run supervisor and duplicate Core-owned workspace/run/process runtime
paths. It does not retain dual-write, schema translation or two launch modes.

## Consequences

- Pulse remains a Rust-first single-toolchain product.
- Core commands continue to work offline without a daemon.
- Runtime commands require or start the local daemon and use its protocol.
- Runtime persistence can use a daemon-appropriate store, but it never becomes
  canonical work truth.
- A daemon crash cannot imply Ticket completion or cancellation success.
- Native Ubuntu, macOS and Windows process/recovery tests are required.
- The historical Slice 3 runner is migration evidence, not the target contract.
- Multi-Agent orchestration composes independent daemon sessions only after the
  single-Agent assignment saga and proof-driven close are reliable.
