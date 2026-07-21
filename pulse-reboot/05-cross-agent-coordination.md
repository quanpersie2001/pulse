# Cross-Agent Coordination

[Trang vào](../PULSE_REBOOT.md) | [Work graph](02-work-graph.md) | [Priority reconciliation](06-priority-reconciliation.md) | [Documentation system](10-documentation-system.md)

**Đọc khi:** cần hiểu Orchestration Agent giao việc cho các Agent độc lập như user, và Pulse cần bổ sung gì so với Maestro.
**Sở hữu:** peer-agent identity, transport, leases, mailbox, authority, lifecycle, failure recovery và acceptance scenarios.

## Khẳng định thiết kế

Pulse không lấy mô hình “một parent agent gọi các sub-agent nội bộ” làm architecture chính.

Pulse dùng **peer-agent orchestration**:

- Orchestration Agent là một Agent/task độc lập.
- Worker, Reviewer và QA Agent cũng là các Agent/task độc lập.
- Mỗi Agent có thread/session identity ổn định, user có thể mở, quan sát, tiếp tục hoặc takeover.
- Orchestration Agent dùng cùng transport primitives mà user có: create/resume/send/wait/interrupt.
- Work graph, lease và gate quyết định quyền; conversation không phải authority.

Nguyên tắc ngắn:

> **User-equivalent transport, bounded authority.**

Orchestrator được nói chuyện với Agent như user, nhưng không tự có quyền đổi acceptance, merge, deploy hoặc override human gate.

## Peer Agent khác sub-agent

| Thuộc tính | Sub-agent nội bộ | Pulse Peer Agent |
|---|---|---|
| Identity | Thường phụ thuộc parent run | Stable Agent + native thread/session ID |
| Visibility | Có thể chỉ thấy trong trace parent | User nhìn thấy và mở task trực tiếp |
| Lifetime | Thường kết thúc cùng parent | Có thể pause/resume độc lập |
| Workspace | Có thể chia sẻ context/files | Binding rõ với worktree/snapshot |
| Assignment | Prompt tạm thời | Ticket/QA lease có revision và TTL |
| Recovery | Parent phải nhớ state | Rebuild từ registry, lease, mailbox và events |
| Authority | Ngầm kế thừa parent | Capability/policy explicit |

Sub-agent vẫn có thể dùng như optimization bên trong một Agent, nhưng không đại diện cho đơn vị chịu trách nhiệm canonical trong Pulse.

## Maestro đang có gì

Maestro đã hiện thực nhiều coordination primitives đáng tham khảo:

- `maestro active` đọc các session độc lập trên nhiều worktree, Card binding, runtime, progress và presence.
- Session claim dùng native IDs như `CODEX_THREAD_ID` và `CLAUDE_CODE_SESSION_ID`, tạo danh tính kiểu agent/session.
- `maestro link`, `msg`, `conflict` cung cấp related edges, card-scoped channels và conflict advisory.
- `loop work-lease` chọn ready Card, bỏ qua live claim, claim một Card và tạo bounded worker prompt.
- Feature fanout recipe tách worker units khỏi conductor-owned verification/close.
- Current `msg send` đã thử direct delivery tới một Codex thread qua `codex app-server proxy` và JSON-RPC `turn/start`; nếu thất bại thì fallback vào local channel và ghi delivery receipt.

Những primitive này chứng minh coordination local-first là khả thi.

## Maestro còn thiếu gì cho mục tiêu Pulse

- Không có lifecycle đầy đủ để create một independent Codex task/thread từ work graph.
- Chưa chuẩn hóa create, assign, wake, wait, collect, retire thành transport interface.
- `work-lease` claim theo caller; conductor gọi có thể claim cho conductor thay vì reserve cho Worker tương lai.
- Messaging/direct delivery chưa đồng nghĩa với assignment acknowledgement và ownership transfer.
- TUI Dispatch hiện chủ yếu đổi feature status sang `assigned`, không phải real agent launcher.
- Chưa có end-to-end recovery khi orchestrator crash giữa dispatch và handoff.
- Recipe dùng khái niệm subagent ở vài nơi, không đáp ứng identity/lifetime của peer Agent mà Pulse cần.

Kết luận: Maestro là coordination substrate và source of patterns, không phải implementation hoàn chỉnh của Pulse orchestration.

### Evidence map trong Maestro checkout

| Nhận định | Nơi kiểm tra |
|---|---|
| Active sessions, Card binding, progress và presence | [`active.rs`](../references/maestro/src/interfaces/cli/active.rs) và [`README.md`](../references/maestro/README.md) |
| Card-scoped messaging, direct Codex delivery, fallback receipt | [`msg.rs`](../references/maestro/src/interfaces/cli/msg.rs) |
| Ready Card selection, live-claim check, lease prompt và hard stops | [`loop_recipes.rs`](../references/maestro/src/interfaces/cli/loop_recipes.rs) |
| Conductor-owned verify/close và worker units | [`feature-fanout.yml`](../references/maestro/embedded/loop-recipes/feature-fanout.yml) |
| TUI Dispatch chỉ cập nhật assignment state, không launch Agent | [`interactive.tsx`](../references/maestro/src/tui/opentui/app/interactive.tsx) |

Các path này là evidence của current checkout, không phải API contract mà Pulse được phép phụ thuộc trực tiếp.

## Kiến trúc mục tiêu

```text
                    Human
                      |
          observe / override / takeover
                      |
              Orchestration Agent
                      |
    +-----------------+------------------+
    |                 |                  |
Work Graph       Thread Transport   Reconciliation
CAS + gates      create/send/wait   semantic decisions
    |                 |                  |
Assignment Lease      +---------+--------+
    |                           |
    v                           v
Worker Agent A              Worker Agent B
native thread A             native thread B
worktree A                  worktree B
    |                           |
    +---- typed mailbox + handoff receipts ----+
                                                |
                                      Review / QA Agent
                                                |
                                      conductor-owned gate
```

Kernel cung cấp primitives deterministic. Orchestration Agent chỉ đọc graph qua versioned CLI/API projections như `pulse work ready --json` và `pulse work packet`; nó dùng judgment để reconcile/dispatch rồi gọi mutation primitives. Nó không scan raw node/edge files và kernel không giả làm semantic planner.

## Agent Registry

Mỗi peer Agent có record:

```yaml
agent_id: agent_codex_17
role: worker
runtime: codex
native_thread_id: 019a...
status: working
capabilities: [typescript, browser]
bound_work:
  kind: ticket
  id: TK-031
  revision: 7
workspace_id: wt_TK-031
lease_id: lease_01J...
last_seen_at: 2026-07-18T02:10:00Z
created_by: agent_orchestrator_01
```

Registry là runtime index, không phải canonical Ticket state. Presence có TTL. Native thread ID không được dùng làm business identity duy nhất vì runtime có thể migrate.

## Thread Transport Adapter

Codex adapter đầu tiên phải cung cấp:

```text
create(task_spec, workspace) -> AgentHandle
resume(agent_handle) -> status
send(agent_handle, message, idempotency_key) -> DeliveryReceipt
wait(agent_handle, cursor, timeout) -> AgentEvents
interrupt(agent_handle, reason) -> result
archive(agent_handle) -> result
```

Yêu cầu:

- Idempotent theo key để retry không tạo hai turns/tasks.
- Trả native thread ID và durable Pulse mapping.
- Hỗ trợ direct delivery; fallback mailbox không được giả là delivered.
- `wait` dùng cursor để không phát lại vô hạn.
- User có thể mở native task mà không phá binding.
- Transport failure không tự đổi Ticket status.

## Assignment Lease

Claim chung là chưa đủ. Orchestrator cần reserve một Ticket **cho danh tính Worker cụ thể**:

```yaml
lease_id: lease_01J...
subject: {kind: ticket, id: TK-031, revision: 7}
assignee_agent_id: agent_codex_17
workspace_id: wt_TK-031
issued_by: agent_orchestrator_01
issued_at: 2026-07-18T02:00:00Z
expires_at: 2026-07-18T02:30:00Z
heartbeat_at: 2026-07-18T02:10:00Z
state: acknowledged
capabilities: [source.write, test.run]
```

State:

```text
reserved -> delivered -> acknowledged -> active -> handed_off -> released
    |            |              |            |
    +---------- expired/cancelled/revoked ---+
```

Lease acquisition là atomic CAS trên Ticket revision. Ticket không được có hai exclusive implementation leases. QA/review lease có thể song song nếu source snapshot read-only và policy cho phép.

## Typed Mailbox

Message giữa Agents phải có envelope và correlation, không chỉ text:

- `assignment`: objective, scope, acceptance, revision, lease, workspace, hard stops.
- `acknowledgement`: accepted/rejected và capability gaps.
- `status`: progress marker, next action, heartbeat.
- `blocker`: loại blocker, evidence, decision cần ai đưa ra.
- `decision_request`: options, trade-off, recommended option.
- `review_request`: source snapshot và evidence manifest.
- `handoff`: changes, proof, remaining risks, follow-up.
- `cancellation`: reason, revision, required cleanup.
- `redirect`: priority/scope change có authority và new revision.

Mỗi send có delivery receipt: `delivered`, `acknowledged`, `fallback_stored`, `expired` hoặc `failed`. `fallback_stored` không được coi là Agent đã đọc.

## Worker handoff receipt

Handoff tối thiểu chứa:

- Ticket ID/revision và lease ID.
- Source snapshot: base, head, dirty diff hash.
- Files/areas và durable docs changed.
- Acceptance-to-evidence mapping.
- Documentation impact result, promotion candidates và proposed registry mutations.
- Learning candidates, historical learning refs đã áp dụng/bỏ qua, required ratchet checks và usage outcome khi có.
- Commands/receipts đã chạy.
- QA/review requests còn cần.
- Blockers và remaining risk.
- Suggested next state: `verifying`, `blocked`, `rework`.

Handoff là đề nghị transition. Gate evaluator mới quyết định transition.

## Orchestration loop

```text
1. Query `pulse work ready --json`, graph projections và live runtime state.
2. Reconcile priority, dependencies, foundation value và supersession.
3. Chọn executable unit, gọi `pulse work packet <id> --json` và xác định required capabilities.
4. Tạo/resume independent Agent task với workspace binding.
5. Atomic reserve Ticket cho Agent identity.
6. Gửi typed assignment và chờ acknowledgement.
7. Theo dõi heartbeat/status bằng wait cursors, không busy-poll.
8. Xử lý blocker: trả lời, xin human decision, redirect hoặc requeue.
9. Thu handoff và khóa source snapshot.
10. Dispatch Reviewer và targeted Ticket QA Agent nếu Ticket gate yêu cầu; full Story QA chỉ dispatch khi có integrated/frozen Story candidate.
11. Evaluate verification/review/checkpoint receipts để close/rework/block Ticket; evaluate full qualification receipts để close/rework Story.
12. Capture learning candidates/usage feedback; dispatch compound task khi policy/cycle yêu cầu.
13. Release lease/workspace; lưu events; chạy reconcile lại theo cadence.
```

Nếu create Agent thành công nhưng reserve thất bại, Agent phải bị cancel/retire hoặc nhận no-op message; tuyệt đối không cho làm việc không có lease.

## Authority matrix

| Action | Human | Orchestrator | Worker | Reviewer/QA | Kernel |
|---|---:|---:|---:|---:|---:|
| Sửa outcome/acceptance | Có | Chỉ khi policy cấp | Không, chỉ đề xuất | Không | Validate revision |
| Reconcile/assign Ticket | Có | Có | Không | Không | CAS/lease rules |
| Sửa source trong scope | Có | Tùy role | Có theo lease | Mặc định không | Enforce workspace binding |
| Sửa approved product/architecture docs | Có | Chỉ khi policy cấp | Không mặc định | Review/đề xuất | Enforce authority/hash |
| Sửa informational docs trong scope | Có | Có thể | Có theo lease | Review | Enforce docs scope |
| Gửi blocker/handoff | Có | Có | Có | Có | Ghi event |
| Đề xuất QA case/applicability | Có | Có | Có | Có | Validate schema/revision |
| Đổi expected behavior/acceptance | Có | Chỉ khi policy cấp | Không | Không | Validate authority/revision |
| Tạo QA receipt/finding | Có | Có thể | Không mặc định | Có | Validate source/case/artifact |
| Tạo learning candidate/usage feedback | Có | Có | Có | Có | Validate schema/provenance |
| Review/validate/promote learning | Có | Theo policy | Chỉ đề xuất | Đề xuất/review | Validate authority/revision |
| Tạo review/QA receipt | Có | Có thể dispatch | Không tự review | Có | Validate receipt |
| Đóng Ticket | Override có audit | Qua gate | Không | Không | Tính gate/transition |
| Đóng Story | Có | Qua Story gate | Không | Không | Tính gate |
| Merge/deploy | Theo repo policy | Chỉ khi explicit grant | Không mặc định | Không | Enforce policy |

Transport role không quyết định authority. Một prompt gửi từ Orchestrator vẫn có thể bị kernel từ chối nếu vượt capability.

## Workspace isolation

- Một exclusive implementation Ticket gắn với một writable worktree.
- Agent chỉ được ghi vào source/docs scope được assignment manifest cho phép; mở rộng scope cần event/approval.
- Review/QA dùng frozen snapshot hoặc read-only workspace khi có thể.
- Hai Tickets chạm vùng xung đột phải có advisory trước dispatch; hard conflict có thể block.
- Cleanup chỉ xảy ra sau khi evidence đã content-address và handoff accepted.
- User sửa trực tiếp worktree được phát hiện qua snapshot drift, không âm thầm ghi đè.

## Recovery và failure modes

### Orchestrator crash

Orchestrator mới rebuild từ work graph, Agent Registry, leases, delivery receipts và mailbox cursors. Không dựa vào chat memory. Lease chưa acknowledged hết TTL; lease active chỉ được adopt sau presence check.

### Worker crash hoặc mất heartbeat

Mark Agent `stale`, giữ worktree, chờ grace period. Sau đó resume native thread hoặc revoke/requeue. Không dispatch Worker mới trên cùng workspace khi lease cũ chưa được resolve.

### Delivery thất bại

Retry cùng idempotency key. Fallback mailbox chỉ tạo pending delivery. Quá deadline thì revoke reservation và retire Agent chưa nhận việc.

### Duplicate dispatch

CAS theo Ticket revision và exclusive lease ngăn hai Worker cùng sở hữu. Nếu external side effect đã xảy ra, idempotency key phải đi xuyên qua tool adapter.

### Scope/priority/docs contract thay đổi giữa run

Tăng Ticket revision hoặc document contract hash/revision, gửi typed redirect. Worker phải acknowledge new revision/context hoặc handoff current work rồi dừng. Evidence/docs receipts từ revision hoặc content hash cũ không tự động close revision mới.

### Ticket bị hấp thụ

Orchestrator gửi cancellation, thu partial handoff, đánh dấu `superseded_by`, bảo toàn useful diff/evidence và giải phóng lease.

### Human takeover

Human mở native Agent task, có thể gửi message trực tiếp. Pulse ghi `human_takeover` event; Orchestrator chuyển sang observe hoặc dừng điều phối Agent đó để tránh hai người lái.

## Reconciliation trong multi-agent

Reconcile chạy:

- Trước mỗi dispatch batch.
- Sau 3-5 Ticket hoàn tất.
- Khi có blocker/Decision làm đổi graph.
- Khi work mới priority cao xuất hiện.
- Khi Agent capacity/capability thay đổi.

Đang có Worker không có nghĩa Ticket bất biến. Nhưng redirect/cancel phải cân nhắc sunk cost, partial evidence và disruption. Chi tiết semantic model ở [`06-priority-reconciliation.md`](06-priority-reconciliation.md).

## Operator surfaces

Pulse cần ít nhất:

```text
pulse agent list                 # presence, role, Ticket, worktree, last seen
pulse agent show <agent-id>      # thread, lease, mailbox cursor, recent events
pulse work leases                # active/reserved/stale/conflicting
pulse agent send <id> --message  # typed or operator message
pulse agent wait <id>            # bounded wait
pulse orchestrate status         # decisions pending, failures, gates
```

UI có thể đến sau. CLI/event stream phải đủ để human hiểu “ai đang làm gì, vì sao và bằng chứng ở đâu”.

Canonical graph mutation chỉ đi qua Pulse CLI/CAS trong control workspace. Worker worktree không tự sửa graph files; Worker gửi typed status/handoff, sau đó Orchestrator/kernel apply transition. Điều này tách source-code branch conflicts khỏi work-state authority.

## Scope triển khai

### Core-compatible foundations

Ngay v1 cần sharded JSON nodes/edges, stable work IDs/revisions, Agent actor in events, workspace/source identity, immutable receipts, CLI projections và CAS transitions.

### Orchestration v2

1. Codex independent-task transport adapter.
2. Agent Registry và presence.
3. Specific-assignee lease + acknowledgement.
4. Typed mailbox và delivery receipts.
5. Orchestrator skill/loop với single Worker.
6. Reviewer/QA peer Agent.
7. Multiple Workers với conflict detection và recovery.

## Acceptance scenarios

- Orchestrator tạo một Codex task độc lập, user nhìn thấy task, và assign đúng Ticket/revision.
- Worker không nhận được assignment thì Ticket tự trở về `ready`; không có ghost ownership.
- Hai Orchestrators không thể cấp exclusive lease cùng Ticket.
- Worker gửi blocker; Orchestrator nhận, quyết định hoặc escalate human và Worker resume đúng thread.
- Orchestrator restart vẫn recover Agent, cursor, lease và pending gate mà không dùng chat memory.
- Human takeover một Worker task; Orchestrator dừng gửi lệnh xung đột.
- Reviewer/QA Agent kiểm tra frozen snapshot; targeted checkpoint receipt attach đúng Ticket + Story case, full qualification receipt attach đúng Story candidate.
- Worker không thể tự đóng Story, đổi acceptance hoặc merge khi policy không cấp quyền.
- Ticket bị supersede giữa run; partial work/evidence được giữ và lease được giải phóng an toàn.
- Direct delivery fail, fallback mailbox không bị báo nhầm là acknowledged.
