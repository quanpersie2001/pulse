# Cross-Agent Coordination Và Orchestration

[Trang vào](../PULSE_REBOOT.md) | [Runtime và daemon](04-runtime-harness.md) | [Priority reconciliation](06-priority-reconciliation.md) | [Story QA](03-story-qa.md)

**Đọc khi:** cần implement Orchestration Agent, Worker/Reviewer/QA topology,
typed communication, parallel dispatch, deliberation, human takeover hoặc
multi-agent recovery.

**Sở hữu:** business roles, ownership/communication graphs, assignment
acknowledgement, mailbox, handoff, orchestration loop, assurance topology,
semantic deliberation và orchestration failure recovery.

Runtime project/workspace/session/provider lifecycle thuộc
[`04-runtime-harness.md`](04-runtime-harness.md). File này không định nghĩa lại
daemon managers.

## Khẳng định thiết kế

Pulse dùng daemon-managed **independent Agent sessions** làm canonical
orchestration units. Orchestration Agent, Worker, Reviewer và QA có stable
`session_id`, timeline và workspace placement mà human có thể quan sát, tiếp tục
hoặc takeover.

Một session có thể có parent runtime relation, nhưng business authority không
được thừa kế từ parent. Work graph, lease, policy và gate quyết định quyền.

Nguyên tắc:

> **User-equivalent transport, bounded business authority.**

Orchestrator dùng cùng daemon operations như user:
create/resume/send/wait/cancel/archive. Nó không tự có quyền đổi acceptance,
waive proof, merge, deploy hoặc bypass human gate.

## Ba graph không được nhập làm một

### Work graph

Core-owned graph biểu diễn outcome, dependency, priority, supersession và
contracts. Đây là canonical work truth.

### Runtime ownership tree

Daemon-owned relation biểu diễn session nào tạo/own/report về session nào:

```text
Orchestrator session A
  +-- Worker session B
  |   `-- Research session D
  `-- Worker session C
```

Parentage dùng cho reporting, notify-on-finish, detach và cascade archive. Nó
không biểu diễn dependency hoặc write authority.

### Communication graph

Typed policy xác định session nào được gửi message kind nào cho session nào:

```text
A <-> B   assignment/status/blocker/handoff
A <-> C   assignment/status/blocker/handoff
B <-> D   delegated research/status/handoff
B <-> C   disabled by default
```

Communication edge không được suy ra chỉ từ cùng workspace hoặc cùng parent.
Direct Worker-to-Worker messaging mở khi assignment/policy cấp capability và có
correlation rõ. Default route đi qua Orchestrator để tránh N-squared chatter và
conflicting directions.

## Runtime topology và business roles

### Orchestration Agent

- Query execution/decision frontiers và runtime capacity.
- Reconcile priority/dependencies/supersession.
- Chọn workspace/session/provider phù hợp.
- Request Core reservation và drive assignment saga.
- Theo dõi acknowledgement, blocker, handoff và gates.
- Dispatch independent assurance theo risk.
- Escalate human khi vượt authority.

### Worker Agent

- Own một implementation assignment và một writable workspace binding.
- Load exact lease-bound packet.
- Implement trong source/docs scope.
- Chạy developer verification và self-review.
- Submit typed blocker/decision request/handoff.
- Không tự close Ticket/Story.

### Reviewer Agent

- Là independent session khi policy yêu cầu.
- Dùng frozen source snapshot.
- Read-only mặc định.
- Tạo findings/receipt; không mutate acceptance hoặc close.

### QA Agent

- Chạy targeted Ticket checkpoint hoặc full Story qualification theo baseline.
- Dùng frozen source/build/environment identity.
- Tạo typed receipt/finding.
- Không thay expected behavior để làm test pass.

### Specialist Agent

Security, migration, performance, documentation hoặc domain specialist chỉ được
dispatch khi capability/risk yêu cầu. Specialist không tự nhận authority ngoài
assignment.

## Workspace và session placement

Một Workspace có thể chứa nhiều sessions:

```text
Workspace wks_feature
  +-- Worker session (writable)
  +-- Reviewer session (read-only snapshot view)
  +-- QA session (read-only or dedicated QA environment)
  `-- terminals/services/browser
```

Parallel implementation Workers phải dùng distinct writable workspaces.
Read-only Reviewer/QA sessions có thể share frozen snapshot hoặc dùng dedicated
workspace khi environment isolation yêu cầu.

Parentage và placement độc lập: một Orchestrator có thể create Worker ở
workspace khác mà vẫn giữ parent relation. Detach không move workspace.

## Assignment contract

Core reservation bind:

```yaml
assignment_id: asg_01J...
lease_id: lease_01J...
subject:
  kind: ticket
  id: TK-031
  contract_revision: 7
packet_fingerprint: sha256:...
source_snapshot: sha256:...
issued_by: ses_orchestrator_01J...
state: reserved
runtime_binding: null
```

Assignment lifecycle:

```text
reserved
  -> runtime_bound
  -> delivered
  -> acknowledged
  -> active
  -> handed_off
  -> released

reserved|runtime_bound|delivered
  -> expired|cancelled|revoked
```

`runtime_bound` chỉ có nghĩa daemon đã provision workspace/session và Core đã
accept opaque binding gồm `project_id`, `workspace_id`, `session_id` và
`provider`. `delivered` không có nghĩa Worker đã đọc. Chỉ `acknowledged` cho
phép coi session là owner thực thi.

Core CAS ngăn hai live exclusive assignments trên cùng Ticket revision. Daemon
idempotency ngăn retry tạo duplicate workspace/session/turn.

## Typed mailbox

Message envelope tối thiểu:

```yaml
message_id: msg_01J...
kind: assignment
sender_session_id: ses_orchestrator_01J...
recipient_session_id: ses_worker_01J...
correlation:
  assignment_id: asg_01J...
  lease_id: lease_01J...
  ticket_id: TK-031
idempotency_key: assignment:asg_01J...
created_at: 2026-07-31T03:00:00Z
payload: {}
```

Message kinds:

- `assignment`
- `acknowledgement`
- `status`
- `blocker`
- `decision_request`
- `review_request`
- `qa_request`
- `handoff`
- `finding`
- `cancellation`
- `redirect`
- `takeover`

Delivery states:

```text
pending -> delivered -> acknowledged
    |          |
    +-> fallback_stored
    +-> expired|failed
```

`fallback_stored` không được báo thành delivered/acknowledged. Retry giữ cùng
idempotency key. Timeline row và delivery receipt có thể correlate nhưng không
thay nhau.

## Worker handoff

Handoff là proposal chuyển lifecycle, không phải `done` assertion.

Handoff tối thiểu chứa:

- assignment/lease/Ticket identity và contract revision;
- workspace/session/provider/turn identity;
- base/head/dirty diff hash và source snapshot;
- files/areas/durable docs changed;
- acceptance-to-evidence mapping;
- commands và receipts;
- documentation impact result;
- learning candidates và applied/ignored prior learning refs;
- required review/QA requests;
- blockers, remaining risks và partial work;
- suggested next state: `verifying|blocked|rework`.

Core validate hashes, bindings, receipt kinds, authority và currentness. Daemon
chỉ vận chuyển/persist runtime handoff và request gate evaluation.

## Orchestration loop

```text
1. Query Core execution frontier and daemon capacity.
2. Reconcile priority, blockers, foundation value and supersession.
3. Select one or more conflict-free assignments.
4. Request Core reservation.
5. Create/adopt Workspace and create/resume Worker session.
6. Activate opaque runtime binding in Core.
7. Deliver workflow bootstrap and wait for acknowledgement.
8. Observe timeline/status without busy-polling.
9. Route blocker or decision request.
10. Collect handoff and freeze source snapshot.
11. Dispatch Reviewer/docs-review/QA sessions according to risk.
12. Submit receipts and evaluate Core gates.
13. Close/rework/block/requeue work.
14. Release assignment and archive/retain runtime by policy.
15. Capture learning candidates and run reconciliation again when triggered.
```

Nếu session được tạo nhưng Core activation fail, daemon phải close/no-op session
và cleanup owned workspace. Không được cho Agent làm implementation không có
current assignment.

## Parallel dispatch

Multiple Workers chỉ dispatch song song khi:

- hard dependencies đã thỏa;
- exclusive reservations khác nhau;
- writable workspaces distinct;
- allowed write scopes không hard-conflict;
- provider/environment capacity đủ;
- integration/merge order explicit;
- human/policy concurrency budget cho phép.

Conflict advisory phải phân biệt:

- same file/path potential conflict;
- same symbol/domain semantic conflict;
- shared environment/service conflict;
- external side-effect conflict;
- ordering-only preference.

Advisory không tự biến mọi overlap thành blocker. Hard conflict phải có
deterministic/policy reason.

Sau handoff, independent read-only assurance có thể fan out trên cùng frozen
snapshot:

```text
frozen candidate
  +-- code review
  +-- documentation review
  +-- targeted QA
  `-- security/specialist review
```

Source thay đổi làm affected receipts stale theo source/ancestor impact policy.

## Risk-adaptive independence

- R0/R1 low-risk: Worker developer verification; independent review/QA chỉ khi
  profile yêu cầu.
- Behavior-changing R1/R2: independent targeted QA hoặc reviewer.
- R2/R3: independent Reviewer và QA trên frozen snapshot.
- Security/destructive migration/production: specialist và/or human gate.

Không spawn independent Agent cho mọi mechanical check. Deterministic checks
chạy như scripts/executors; peer sessions dành cho independent judgment hoặc
environment-bound work.

## Semantic deliberation

High-stakes architecture/reconciliation có thể dùng independent proposals và
mutual challenge. Đây là risk-adaptive judgment workflow, không phải majority
vote hoặc fixed model pipeline.

### Preconditions

- decision question/destination rõ;
- same immutable evidence packet;
- graph/source/docs revisions fixed;
- authority và exit condition declared;
- R2/R3 hoặc explicit human request.

### Flow

```text
grounded DecisionPacket
  -> Proposal A produced independently
  -> Proposal B produced independently
  -> mutual evidence-based challenges
  -> revised proposals
  -> anonymous compilation
  -> primary + shadow assessment
  -> agreement or explicit disagreement
  -> Decision recommendation packet
  -> authorized accept/reject/defer
```

Provider/model names không được hard-code vào semantic contract. Roles chọn qua
capability policy. Anonymous compilation có thể ẩn author/provider identity để
giảm prestige bias nhưng giữ provenance trong protected metadata.

Arbiters tạo recommendation receipts, không có quyền mutate graph hoặc accept
Decision. Khi assessors bất đồng, evidence thiếu hoặc confidence dưới policy,
output là human `decision_request`, không ép convergence.

Decision recommendation packet gồm:

- decision/destination và revisions;
- options;
- claim-evidence matrix;
- original/revised proposals;
- challenges và responses;
- agreements;
- unresolved disagreements;
- assessor findings/confidence;
- proposed graph/contract mutations;
- required grant/approver;
- revisit trigger/expiry.

Small backlog hoặc reversible choice không dùng debate vì ceremony cost lớn hơn
assurance value.

## Authority matrix

| Action | Human | Orchestrator | Worker | Reviewer/QA | Core |
|---|---:|---:|---:|---:|---:|
| Sửa outcome/acceptance | Có | Chỉ explicit grant | Không | Không | Validate |
| Reconcile/assign | Có | Có theo policy | Không | Không | CAS/lease |
| Tạo workspace/session | Có | Có | Không mặc định | Không | Không sở hữu |
| Sửa source trong scope | Có | Tùy assignment | Có | Read-only mặc định | Validate binding |
| Gửi blocker/handoff/finding | Có | Có | Có | Có | Validate receipt |
| Tạo review/QA receipt | Có | Dispatch | Không tự-review | Có | Validate |
| Close Ticket/Story | Override audit | Request gate | Không | Không | Evaluate |
| Merge/deploy | Repo policy | Explicit grant | Không mặc định | Không | Validate policy |
| Accept Decision | Có | Explicit grant | Đề xuất | Đề xuất | Validate proof |

Transport role không tạo authority. Daemon permission quyết định tool access;
Core policy quyết định business mutation.

## Human takeover

Human takeover là explicit control transition:

```text
orchestrator_controlled
  -> takeover_requested
  -> current turn quiesced/acknowledged
  -> human_controlled
```

Daemon ghi control owner và broadcast. Orchestrator chuyển observe-only, không
gửi conflicting prompt/cancel. Trả control cũng là explicit transition.

## Recovery

### Daemon crash

Restart rebuild từ project/workspace/session registries, process ledger,
timelines, assignment saga records và Core leases. Không dựa vào chat memory.

### Orchestrator session crash

Một Orchestrator mới có thể adopt orchestration run sau khi Core/daemon
reconciliation chứng minh no live control owner. Worker sessions không bị
archive chỉ vì Orchestrator runtime closed.

### Worker crash

Giữ workspace và partial timeline. Resume same provider session khi persistence
handle/currentness cho phép; nếu không, create replacement session sau khi revoke
old runtime binding và preserve handoff snapshot.

### Delivery failure

Retry same idempotency key. Assignment hết deadline trước acknowledgement thì
release/requeue và retire unused runtime.

### Contract drift

Core contract revision/docs hash/source binding đổi thì Orchestrator gửi typed
redirect. Worker acknowledge new binding hoặc handoff rồi dừng. Old receipts
không tự close new revision.

### Supersession

Cancel active turn safely, collect partial handoff, persist
`superseded_by`, transfer uncovered acceptance/evidence và release runtime.

## Operator surfaces

```text
pulse session list|show|send|wait|cancel|archive|detach
pulse workspace list|show|create|archive|restore
pulse assignment list|show|release|recover
pulse timeline fetch|tail
pulse orchestrate start|status|resume|cancel
pulse permission list|respond
```

Operator phải trả lời được:

- Agent nào đang làm gì?
- Thuộc Workspace/Ticket/lease nào?
- Ai đang control?
- Provider turn có thật sự live không?
- Message đã delivered hay acknowledged?
- Gate đang thiếu receipt nào?
- Recovery/cleanup action nào an toàn?

## Acceptance scenarios

1. Orchestrator tạo user-visible Worker session ở explicit workspace và bind
   đúng Ticket revision.
2. Create retry không tạo duplicate workspace/session/assignment.
3. Unacknowledged assignment hết hạn và Ticket trở lại executable.
4. Hai Orchestrators không giữ hai exclusive assignments cùng Ticket.
5. Worker blocker route tới Orchestrator/human và resume same session.
6. Daemon restart recover session, timeline cursor, process và assignment saga.
7. Human takeover ngăn Orchestrator gửi conflicting actions.
8. Parent archive cascade managed children nhưng không mutate Ticket status.
9. Detached child giữ workspace/session mà không còn parent ownership.
10. Multiple Workers chỉ chạy khi dependency/write-scope/workspace policy pass.
11. Reviewer/QA kiểm tra frozen snapshot và receipts stale khi source đổi.
12. Fallback mailbox không bị báo nhầm acknowledged.
13. Active Ticket superseded giữ partial work/evidence và cleanup an toàn.
14. Worker/Reviewer/QA không vượt authority.
15. High-stakes deliberation giữ independent proposals, explicit disagreement
    và human escalation khi assessor không hội tụ.
