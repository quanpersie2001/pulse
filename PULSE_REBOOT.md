# Pulse Reboot

> Trạng thái: target design pre-release; không phải compatibility contract.
> Cập nhật: 2026-07-31.
> Bản đồ chi tiết: [`pulse-reboot/README.md`](pulse-reboot/README.md).

## Pulse là gì?

Pulse là một **local-first harness engineering system** giúp repository trở nên
dễ hiểu, dễ sửa, dễ chạy, dễ kiểm chứng và tự cải thiện đối với coding agents.

Pulse có hai bounded contexts cùng viết bằng Rust:

1. **Pulse Core** quản lý repository semantics: local work graph,
   documentation/knowledge, readiness, bounded context, policy, evidence và
   proof-driven gates.
2. **Pulse Daemon** quản lý agent runtime: projects, workspaces, sessions,
   providers, processes, permissions, timelines, clients và orchestration.

Pulse không phải Jira thu nhỏ, fixed phase workflow, cloud-first service hoặc
general-purpose agent framework.

## Product thesis

> Pulse biến repository thành một môi trường nơi Agent chọn đúng việc, lấy đúng
> context, chạy trong đúng workspace, dùng đúng provider capability, tạo proof
> đáng tin và làm harness tốt hơn sau mỗi failure.

North star:

> **Correct completion với ít can thiệp của human hơn, trong khi work truth,
> runtime state, authority và evidence vẫn local, inspectable và recoverable.**

## Kiến trúc một trang

```text
Clients
  +-- pulse CLI
  +-- Web/Desktop
  +-- HTTP + WebSocket
  `-- Agent-facing MCP
              |
              v
        Pulse Rust Daemon
  +-----------+------------+----------------+
  |           |            |                |
Project    Workspace     Session         Provider
Registry   Manager       Manager         Registry
              |            |                |
          local/worktree timeline        Codex native
          services       permissions     Claude native
          terminals      parentage       ACP generic
              |            |
              +------ Runtime Store ------+
                         |
                 Timeline + PubSub
                         |
                         v
                    Pulse Core
   Work Graph -> Packet/Policy -> Evidence/Gates
       |              |                |
       +-- Docs/Knowledge              +-- Verify/Review/QA
                         |
                  Source Repository
```

Paseo là reference cho daemon runtime shape: daemon-managed Project/Workspace/
Session/Provider, transport-neutral tool catalog và timeline sync. Symphony/App
Server là reference cho dispatch, workspace isolation và native Codex transport.
Pulse không phụ thuộc các dự án đó và giữ local graph/proof semantics riêng.

## Các quyết định nền tảng

- Repository + `.pulse` là system of record cho work, docs, knowledge, policy,
  semantic events và evidence.
- Daemon là source of truth cho host-local Project, Workspace, Session,
  provider process, permissions, timeline và presence.
- Một field/state chỉ có một writer; Core và Daemon không cùng sở hữu lifecycle.
- Core và Daemon cùng viết bằng Rust và dùng chung typed library contracts.
- Một executable `pulse`: Core commands chạy offline; runtime commands kết nối
  local daemon.
- Canonical work graph là sharded `nodes/*.json` + `edges/*.json`; work prose ở
  top-level `works/`.
- Ticket là executable unit. Epic/Story giữ outcome, approach và behavioral
  baseline; hierarchy không thay dependency graph.
- Workspace là stable container; worktree chỉ là isolation mode. Workspace có
  thể chứa nhiều Worker/Reviewer/QA sessions, terminals và services.
- Stable Pulse `session_id` khác provider-native thread/session handle.
- Session parentage, workspace placement và business role là ba relation riêng.
- Work graph/lease/gate quyết định business authority; conversation/parentage
  không phải authority.
- Assignment là Core reservation bind exact Ticket contract revision, committed
  WorkPacket fingerprint và opaque daemon runtime references.
- Session prompt chỉ là role-specific workflow bootstrap. Exact contract, docs
  và knowledge được load qua typed Core query.
- Daemon trực tiếp sở hữu provider/helper processes. Hidden per-run supervisor
  không còn là target architecture.
- Process ownership vẫn fail closed và dùng platform adapters cho Linux,
  macOS, Windows identity/tree cancellation.
- Live WebSocket/PubSub phục vụ immediacy; authoritative paged timeline fetch
  theo cursor phục vụ correctness.
- Tool catalog thuộc daemon application layer; MCP, CLI, HTTP/WS và native
  provider tools là adapters.
- Codex native là provider đầu tiên; Claude native và ACP generic thêm sau khi
  contract đã được chứng minh.
- Deterministic mechanism thuộc Core/daemon code; semantic judgment thuộc Agent
  capabilities.
- Critical ambiguity phải disposition trước execution bằng shaping
  repo-grounded và risk-adaptive.
- Developer verification, independent review, Ticket QA checkpoint và Story
  qualification là các assurance purposes riêng.
- Worker submit handoff; Core gate quyết định `done|rework|blocked`.
- Durable docs/current Decisions không bị runtime timeline hoặc learning record
  override.
- Failure có evidence phải đi vào knowledge/harness ratchet.
- Single-agent Core-Daemon vertical slice và recovery phải pass trước
  multi-Worker concurrency.

## State ownership

| Plane | Examples | Owner |
|---|---|---|
| Durable repository knowledge | `docs/`, `AGENTS.md`, `PULSE.md` | Core/repository |
| Canonical work | nodes, edges, contracts, Decisions | Core/repository |
| Runtime control | projects, workspaces, sessions, providers, presence | Daemon |
| Runtime timeline | messages, tool calls, permissions, turn status | Daemon |
| Evidence | receipts, diffs, screenshots, verification artifacts | Core/evidence store |
| Client presentation | tabs, focused session, cached tail | Client replica only |

Session `closed` không làm Ticket `done`. WebSocket disconnect không hủy
assignment. Message “passed” không thay receipt. Daemon restart không được cần
chat memory để recover.

## Bản đồ đọc

| Khi cần hiểu | Tài liệu sở hữu |
|---|---|
| Product direction và references | [`01-foundations.md`](pulse-reboot/01-foundations.md) |
| Work graph và lifecycle | [`02-work-graph.md`](pulse-reboot/02-work-graph.md) |
| Story QA và behavioral proof | [`03-story-qa.md`](pulse-reboot/03-story-qa.md) |
| Core/Daemon/runtime/provider architecture | [`04-runtime-harness.md`](pulse-reboot/04-runtime-harness.md) |
| Multi-agent orchestration và deliberation | [`05-cross-agent-coordination.md`](pulse-reboot/05-cross-agent-coordination.md) |
| Priority và semantic reconciliation | [`06-priority-reconciliation.md`](pulse-reboot/06-priority-reconciliation.md) |
| Verification, doctor và ratchet | [`07-verification-ratchet.md`](pulse-reboot/07-verification-ratchet.md) |
| Phases, migration và acceptance | [`08-implementation-roadmap.md`](pulse-reboot/08-implementation-roadmap.md) |
| Decisions và milestone DoD | [`09-decisions-and-dod.md`](pulse-reboot/09-decisions-and-dod.md) |
| Documentation system | [`10-documentation-system.md`](pulse-reboot/10-documentation-system.md) |
| Documentation retrieval | [`11-documentation-retrieval.md`](pulse-reboot/11-documentation-retrieval.md) |
| Knowledge compounding | [`12-knowledge-compounding.md`](pulse-reboot/12-knowledge-compounding.md) |

## Milestone boundary

### Pulse Core

Work graph, docs/knowledge, shaping/readiness, packet/reservation, policy,
evidence và gates.

### Pulse Runtime

Rust daemon, local protocol, Project/Workspace/Session managers, Codex provider,
timeline, permissions, process ownership và single-Agent recovery.

### Pulse Orchestration

Orchestration Agent, typed mailbox, parentage/communication graphs, Reviewer/QA
peers, deliberation, multiple Workers, human takeover và semantic
reconciliation loop.

Thiết kế identities/protocol phải hỗ trợ cả ba từ đầu, nhưng implementation phải
prove theo thứ tự Core -> Runtime -> Orchestration.

## Frontier hiện tại

Phase 1 Core foundations và phần lớn Phase 2 packet/reservation đã được
implemented. Phase 2 Slice 3 cũng đã implement một CLI-owned hidden supervisor,
workspace/run store và Codex process path.

Direction mới thay per-run supervisor bằng long-lived Rust daemon theo
Project/Workspace/Session/Provider architecture. Những implementation hiện có
không phải compatibility target; primitive đúng được salvage, ownership sai
được move/rewrite và obsolete path bị xóa sau replacement proof.

Execution plan duy nhất:
[`proposals/phase2-rust-daemon-realignment-implementation-gap.md`](proposals/phase2-rust-daemon-realignment-implementation-gap.md).

Không bắt đầu multi-Worker orchestration trước khi vertical slice này pass:

```text
Core reservation
  -> Daemon Workspace
  -> Daemon Codex Session
  -> timeline + handoff
  -> Core verification/review gate
  -> done|rework|blocked
  -> restart recovery
```
