# Runtime, Daemon Và Repository Harness

[Trang vào](../PULSE_REBOOT.md) | [Work graph](02-work-graph.md) | [Cross-agent coordination](05-cross-agent-coordination.md) | [Verification ratchet](07-verification-ratchet.md)

**Đọc khi:** cần implement Pulse Core, daemon, CLI, workspace/session/provider
runtime, context loading, process ownership, events hoặc target-repository
harness.

**Sở hữu:** ranh giới Core/Daemon, runtime identities, workspace/session/provider
lifecycle, Core-Daemon protocol, process ownership, timeline, public surfaces và
repository harness capabilities.

## Khẳng định thiết kế

Pulse có hai bounded contexts cùng được viết bằng Rust:

1. **Pulse Core** sở hữu repository semantics: work graph, contracts, readiness,
   bounded packets, policy, evidence, gates, documentation và knowledge.
2. **Pulse Daemon** sở hữu local agent runtime: projects, workspaces, sessions,
   provider processes, permissions, timelines, subscriptions và orchestration.

Hai lớp dùng cùng Rust library contracts nhưng không chia quyền sở hữu field.
Daemon không trở thành nguồn sự thật thứ hai cho Ticket. Core không tạo
worktree, giữ native provider handle hoặc giả làm session manager.

Paseo là reference chính cho daemon runtime shape: project chứa workspace,
workspace chứa nhiều sessions, provider adapter sở hữu native runtime, live
stream phục vụ immediacy và authoritative timeline fetch phục vụ correctness.
Pulse giữ thêm local work graph và proof-driven gates mà Paseo không sở hữu.

## Kiến trúc mục tiêu

```text
Clients
  +-- pulse CLI
  +-- Web/Desktop
  +-- HTTP + WebSocket API
  `-- Agent-facing MCP
              |
              v
        Pulse Rust Daemon
  +-----------+------------+----------------+
  |           |            |                |
Project    Workspace     Session         Provider
Registry   Manager       Manager         Registry
              |            |                |
          local/worktree SessionActor    Codex native
          services       timeline        Claude native
          terminals      permissions     ACP generic
              |            |
              +------ Runtime Store ------+
                         |
                 Timeline + PubSub
                         |
                         v
                    Pulse Core
          graph / packet / policy / evidence
          docs / knowledge / gates / events
                         |
                 Target Repository
```

Daemon là long-lived control plane. Nó thay hidden per-run supervisor làm
lifecycle authority, nhưng vẫn phải thực hiện process supervision bên trong:
spawn ownership, identity, timeout, cancellation, exit observation, orphan
reconciliation và bounded logs.

## Ownership matrix

| Concern | Pulse Core | Pulse Daemon |
|---|---|---|
| Epic/Story/Ticket/Decision và typed relations | Own | Query/mutate qua Core API |
| Contract revision, readiness và frontier | Own | Consume projection |
| Work reservation chống duplicate assignment | Own | Request/release |
| WorkPacket và applicable docs/knowledge | Own | Load/route |
| Evidence, receipts và close gate | Own | Execute/submit |
| Project/workspace identity | Reference opaque IDs | Own |
| Worktree create/adopt/archive/restore | No | Own |
| Session lifecycle và native provider handle | No | Own |
| Provider discovery/capabilities/models/modes | No | Own |
| Provider process, permission và cancellation | No | Own |
| Runtime timeline, subscriptions và presence | No | Own |
| Semantic work events | Own | Correlate/reference |
| High-volume runtime events/logs | Reference artifact IDs | Own |

Một state mutation không được có hai writers. Daemon gọi Core library/API để
reserve, activate, submit handoff hoặc transition work; nó không sửa raw graph
files. Core nhận opaque runtime references và source attestations, không
reconstruct daemon state từ PID/cwd heuristics.

## Runtime identity model

Pulse tách năm identity:

| Identity | Meaning | Owner |
|---|---|---|
| `project_id` | Host-local registered repository/directory | Daemon |
| `workspace_id` | Stable place where work happens | Daemon |
| `session_id` | Stable Pulse-managed agent session | Daemon |
| `assignment_id` / `lease_id` | Exact Ticket revision reserved for execution | Core |
| `turn_id` / `attempt_id` | One provider foreground execution attempt | Daemon |

Provider-native IDs such as Codex thread ID hoặc Claude session ID chỉ là
`provider_handle`. Chúng không thay `session_id`. `workspace_id` là opaque, không
được parse thành path. Filesystem operations dùng explicit `cwd` hoặc
`workspace_root`.

## Project và workspace model

Project là daemon-global registration của một target repository hoặc directory.
Một Project chứa nhiều Workspace.

Workspace là stable product container:

```text
Project
  `-- Workspace
      +-- source directory
      +-- zero or more Agent sessions
      +-- terminals and managed services
      +-- browser/QA surfaces
      `-- workspace-scoped timeline/activity
```

Workspace creation và session creation là hai operation riêng. Workspace có thể
tồn tại khi chưa có Agent và có thể chứa Worker, Reviewer và QA sessions cùng
lúc.

Isolation modes tối thiểu:

- `local`: dùng existing directory, không sở hữu cleanup.
- `worktree`: daemon create/adopt managed Git worktree và sở hữu lifecycle theo
  explicit ownership flag.

Worktree là implementation của isolation, không phải workspace identity.
Workspace archive là lifecycle riêng với session archive. Xóa managed worktree
chỉ hợp lệ khi không còn live ownership/reference và required evidence đã được
content-address.

## Session model và lifecycle

Session là stable Pulse identity bao quanh một resumable provider session.

```text
initializing -> idle <-> running
      |          |        |
      +-------- error ----+
                   |
                 closed
```

- `initializing`: daemon đang tạo hoặc resume provider runtime.
- `idle`: provider session live, không có foreground turn.
- `running`: một foreground turn đang active.
- `error`: attempt gần nhất lỗi nhưng session record còn recoverable.
- `closed`: không còn live provider runtime; identity, persistence handle,
  timeline và workspace binding vẫn giữ.

`archived_at` là orthogonal soft-delete state, không phải lifecycle status.
Unarchive là explicit transition. Closing UI tab không tự mutate canonical
session/workspace state.

Cancellation là protocol:

```text
cancel requested
  -> provider interrupt requested
  -> provider acknowledges or emits terminal turn event
  -> daemon commits cancelled/idle state
```

Nếu provider reject/timeout, session vẫn `running`. Daemon không synthesize
local cancellation rồi nhận prompt mới khi provider còn sở hữu foreground turn.

## Parentage, placement và business role

Ba relation độc lập:

1. `parent_session_id`: runtime ownership/reporting/cascade archive.
2. `workspace_id`: nơi session chạy.
3. assignment role: Worker, Reviewer, QA, Orchestrator hoặc specialist.

Một child session có thể chạy cùng hoặc khác workspace với parent mà không đổi
parentage. Detach xóa parent relation nhưng không move/restart session. Parentage
không truyền authority nghiệp vụ; Worker không được đổi acceptance hoặc close
Ticket chỉ vì được Orchestrator tạo.

Provider-managed child sessions có thể được surface bằng daemon descriptors
nhưng provider vẫn sở hữu underlying runtime. Pulse identity và provider handle
không được nhập làm một.

## Provider Registry

Provider Registry quản lý adapter factories, availability, catalogs,
capabilities và diagnostics. Provider config replacement không được tự spawn
session.

Contract khái niệm:

```rust
trait AgentProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    async fn availability(&self, scope: ProviderScope) -> ProviderAvailability;
    async fn catalog(&self, scope: ProviderScope) -> ProviderCatalog;
    async fn create_session(
        &self,
        config: SessionConfig,
        launch: LaunchContext,
    ) -> Result<Box<dyn ProviderSession>>;
    async fn resume_session(
        &self,
        handle: PersistenceHandle,
        overrides: SessionOverrides,
        launch: LaunchContext,
    ) -> Result<Box<dyn ProviderSession>>;
}

trait ProviderSession {
    async fn start_turn(&mut self, input: TurnInput) -> Result<TurnHandle>;
    async fn interrupt(&mut self) -> Result<InterruptOutcome>;
    async fn respond_permission(
        &mut self,
        request: PermissionResponse,
    ) -> Result<()>;
    fn persistence_handle(&self) -> Option<PersistenceHandle>;
    async fn close(&mut self) -> Result<()>;
}
```

Integration tiers:

1. **Codex native** qua Codex App Server.
2. **Claude native** qua supported native SDK/process transport.
3. **ACP generic** cho provider nói Agent Client Protocol.

Chỉ normalize capability thật sự chung. Provider-specific modes, thinking,
permissions, native tools và resume semantics được giữ qua typed extension
fields; không ép lowest-common-denominator.

Tool catalog thuộc daemon application layer. Provider hỗ trợ native tool
definitions nhận catalog trực tiếp. Provider chỉ hỗ trợ MCP nhận cùng catalog
qua MCP adapter. Không register cả native tools và MCP duplicate cho cùng
session.

## Process ownership

Daemon trực tiếp sở hữu mọi provider/helper process từ lúc spawn. Không giữ
process sống ngoài manager trong readiness future. Daemon ghi managed-process
record gồm provider/kind, PID, process identity, argv/executable fingerprint,
session/turn owner và timestamps.

Portable process owner làm:

- spawn without shell interpolation;
- bounded stdout/stderr capture và redaction;
- timeout/cancel state;
- exit observation;
- orphan ledger reconciliation;
- fail closed khi identity không đủ.

Platform adapters làm:

- Linux: process group và `/proc` boot/start identity.
- macOS: process group và public kernel/libproc start identity.
- Windows: suspended spawn, owned Job Object và process creation identity.

Daemon startup reconcile ledger:

1. dead process record -> mark observed and remove/adjudicate;
2. PID identity mismatch -> không kill, mark stale/operator action;
3. positively matched daemon-owned leftover -> adopt nếu provider protocol cho
   phép, nếu không terminate qua owned tree rồi recover session;
4. uninspectable process -> giữ record, không đoán.

Daemon process itself được desktop hoặc OS service manager quản lý. Không cần
hidden `pulse __run-supervisor` cho mỗi run trong target architecture.

## Timeline và event model

Pulse giữ hai event planes:

### Semantic work events

Core ghi immutable repository-scoped events cho graph/lifecycle/evidence:

```json
{
  "schema_version": 1,
  "id": "evt_01J...",
  "event_type": "ticket.handoff_submitted",
  "occurred_at": "2026-07-31T02:00:00Z",
  "actor": {"kind": "agent", "id": "ses_01J..."},
  "subject": {"kind": "ticket", "id": "TK-031", "revision": 7},
  "correlation": {
    "assignment_id": "asg_01J...",
    "workspace_id": "wks_01J...",
    "session_id": "ses_01J..."
  },
  "payload": {"receipt_id": "handoff_01J..."}
}
```

### Runtime timeline

Daemon ghi ordered timeline rows cho session turns, messages, reasoning, tool
calls, permissions, status và errors. Runtime timeline không phải canonical work
state và không tự tạo evidence receipt.

Delivery có hai đường:

1. WebSocket/PubSub delta cho immediacy.
2. Authoritative paged fetch theo `(epoch, sequence cursor)` cho correctness.

Reconnect với cursor phải catch up tới `has_newer=false`. Presence/heartbeat chỉ
phục vụ liveness/notification routing, không được dùng làm delivery correctness
gate. Tool output được bound trước khi ghi cả live và durable timeline.

## Core-Daemon assignment saga

Core mutation và daemon provisioning không thể là một filesystem transaction.
Chúng tạo một explicit saga có idempotency và compensation:

```text
1. Daemon requests Core reservation for Ticket contract revision.
2. Core validates readiness/policy and returns:
   assignment_id, lease_id, WorkPacket fingerprint, workspace strategy.
3. Daemon creates/adopts Workspace.
4. Daemon creates/resumes provider Session.
5. Daemon activates assignment with opaque runtime binding:
   project_id, workspace_id, session_id, provider, source snapshot.
6. Session receives role-specific workflow bootstrap.
7. Worker loads exact lease-bound WorkPacket and acknowledges assignment.
8. Core records acknowledgement and allows execution lifecycle to continue.
```

Compensation:

- reservation fail: không provision runtime;
- workspace fail: release reservation;
- session fail: cleanup owned workspace, release reservation;
- activation CAS fail: close/no-op session, preserve diagnostics, cleanup/release;
- delivery fail: retry idempotently, không coi fallback storage là acknowledged;
- daemon crash: recover saga từ runtime store và Core lease projection.

Một assignment chưa acknowledged không được coi là active ownership. Ticket
status không được derive từ session status.

## Bounded execution context

Core build `WorkPacket` từ graph, docs, knowledge, policy và source bindings.
Trước reservation là preview; sau reservation, lease-bound query trả exact
committed packet.

Daemon chỉ gửi một versioned role bootstrap:

```text
run/session/assignment identity
exact command/API to load committed packet
required docs/knowledge retrieval procedure
authority boundary
hard stop and handoff protocol
```

Prompt không inline toàn bộ Ticket, Story/Epic, Decisions, docs, QA baseline
hoặc knowledge corpus. Worker load:

```text
pulse work packet <ticket-id> --lease <lease-id> --json
  -> pulse docs get <required-section-ref>
  -> pulse knowledge applicable --work <ticket-id> --audience worker
  -> inspect source
  -> implement, verify, handoff
```

Daemon có thể expose Core context operations qua HTTP/MCP, nhưng semantic result
phải giống CLI/library contract và giữ hashes/revisions.

## Repository harness capabilities

Target repository sở hữu:

- `AGENTS.md`: navigation map.
- `PULSE.md`: human-readable intent và judgment boundaries.
- `.pulse/policy/authority.json`: enforceable default-deny grants.
- scripts build/test/lint/dev/seed/reset.
- skills cho shaping, implementation, debug, review, QA và reconciliation.
- verification profiles.
- environment/executor manifests.
- hooks và evals.

### Skills

Judgment capabilities:

- `pulse-orient`
- `pulse-shape`
- `pulse-plan`
- `pulse-implement`
- `pulse-debug`
- `pulse-review`
- `pulse-qa`
- `pulse-reconcile`
- `pulse-compound`
- `pulse-improve-harness`

Skills hướng dẫn discovery, decisions và output contract. Chúng không copy
repository docs hoặc daemon protocol thành prompt dài.

### Scripts và tools

Deterministic scripts có exit code, timeout và machine-readable output. Tool
manifests khai báo capability, permission, side effects, environment,
artifacts, timeout/cancellation và redaction.

Daemon-owned tools:

- workspace create/list/archive/restore;
- session create/resume/send/wait/cancel/archive/detach;
- provider list/catalog/diagnostic;
- terminal/service lifecycle;
- permissions;
- timeline fetch/subscribe.

Core-owned tools:

- work/graph/docs/knowledge queries và mutations;
- packet/reservation;
- evidence validation;
- verification/QA gate;
- semantic event query.

### Hooks và evals

Hook chỉ enforce guardrail rẻ, deterministic. Eval đo cả Core và daemon:

- context routing;
- assignment saga recovery;
- provider resume/cancel;
- timeline catch-up;
- workspace cleanup;
- receipt validity;
- repeated failure prevention.

## Public surfaces

Một executable `pulse` có hai nhóm command:

```text
# Core, chạy offline không cần daemon
pulse work ...
pulse graph ...
pulse docs ...
pulse knowledge ...
pulse evidence ...
pulse verify ...
pulse qa ...

# Runtime, kết nối local daemon
pulse daemon start|stop|status|run
pulse project list|show|add|archive
pulse workspace create|list|show|archive|restore
pulse session create|show|list|send|wait|cancel|archive|detach
pulse provider list|catalog|diagnostic
pulse timeline fetch|tail
pulse permission list|respond
pulse orchestrate start|status|resume|cancel
```

Web/Desktop, HTTP/WebSocket và MCP gọi cùng daemon application services. MCP là
thin adapter trên transport-neutral tool catalog. CLI runtime commands không
reimplement manager semantics.

Core commands có thể gọi library trực tiếp. Khi daemon gọi Core, nó dùng cùng
library contracts và repository lock/CAS; không shell-out ngược vào CLI.

## Storage boundaries

```text
Target repository
  .pulse/workgraph/       # Core canonical work
  .pulse/docs/            # Core docs registry
  .pulse/knowledge/       # Core learning records
  .pulse/events/          # Core semantic events
  .pulse/evidence/        # immutable receipts/artifacts
  .pulse/cache/           # disposable Core projections

Host-local Pulse home
  projects/               # daemon project/workspace registry
  sessions/               # daemon session metadata/persistence handles
  timelines/              # runtime timeline rows
  processes/              # managed process ledger
  daemon/                 # endpoint, pid/lock, logs, identity
```

Exact runtime persistence engine là implementation decision. Contract bắt buộc:
atomic/idempotent mutations, crash recovery, ordered timeline cursors, bounded
storage, explicit archival và không giả làm repository truth.

## Failure và recovery invariants

- Daemon restart không cần chat memory.
- Core reservation có thể reconcile với missing/stale runtime binding.
- Workspace cleanup không chạy khi ownership chưa proven.
- Session cancel failure không mở turn mới.
- Provider process không được orphan ngoài managed ledger.
- WebSocket disconnect không đổi session hoặc Ticket state.
- Timeline live gap luôn recover bằng authoritative fetch.
- Runtime logs/prompt không tự trở thành evidence.
- Human takeover tạo explicit control-owner transition.
- Core close gate chỉ tin typed, current receipts.

## Acceptance scenarios

1. Core commands vẫn query/mutate work offline khi daemon không chạy.
2. Daemon register Project và tạo opaque Workspace độc lập với cwd identity.
3. Một Workspace chứa nhiều sessions mà lifecycle không nhập làm một.
4. Codex session create, turn, cancel, close và resume giữ stable `session_id`.
5. Cancel timeout không tạo false idle/cancelled state.
6. Daemon crash/restart reconcile provider process ledger và session state.
7. WebSocket client reconnect từ cursor nhận đủ committed timeline rows.
8. MCP/CLI/Web dùng cùng tool/application behavior.
9. Assignment saga fail ở mỗi boundary đều compensate không ghost lease/session.
10. Worker load exact committed packet; newer Ticket revision không silently
    thay assignment đang chạy.
11. Provider-native tools và MCP fallback không bị inject duplicate.
12. Workspace archive không xóa unmanaged directory hoặc unaccepted evidence.
13. Session/Workspace runtime status không tự close Ticket.
14. Linux/macOS/Windows process-tree identity/cancel/recovery pass native tests.
15. Một Ticket đi hết `ready -> reserved -> active -> verifying ->
    done|rework|blocked` bằng daemon runtime và Core proof gate.
