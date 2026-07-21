# Phase 1 — Slice 2: Lifecycle, Supersession + Structural Executability

> Trạng thái: **proposal để review**, chưa phải work contract hay compatibility contract.
> Tiền đề: [`phase1-slice1-storage-graph.md`](phase1-slice1-storage-graph.md) đã hoàn thành và cung cấp storage/transaction/work-graph primitive.
> Sở hữu: implementation strategy cho lát cắt Phase 1 tiếp theo: lifecycle mutation, supersession semantics, dependency-aware structural executability và các graph queries cần để giải thích chúng.
> Tham chiếu normative: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`02-work-graph.md`](../pulse-reboot/02-work-graph.md), [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md), [`06-priority-reconciliation.md`](../pulse-reboot/06-priority-reconciliation.md), [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md), [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md).

## Vị trí của slice trong Pulse Reboot

Slice 1 đã chứng minh nền lưu trữ sharded JSON, CAS, repository-scoped write fence, transaction recovery, immutable semantic events, deterministic graph projection và disposable cache. Slice 2 dùng chính các primitive đó để đưa work graph từ “có thể lưu node/edge” sang “có thể diễn tả và giải thích trạng thái công việc”.

Slice này tập trung vào bốn capability:

1. mutation trạng thái theo một transition table có CAS và audit;
2. supersession/cancellation không xóa lịch sử hoặc giả completion;
3. dependency-aware structural executability với reason codes deterministic;
4. neighborhood/affected queries và roll-up cơ học để human/Agent thấy tác động graph.

Slice 2 **không tuyên bố hoàn thành readiness gate đầy đủ**. Readiness đầy đủ của Pulse còn phụ thuộc approved implementation contract, shaping/authority receipt, documentation impact/applicable docs, QA impact và evidence/receipt validators. Những plane đó chưa tồn tại trong Slice 1 và không được giả bằng boolean hoặc free-text assertion trong Slice 2.

Vì vậy proposal dùng thuật ngữ:

- **structural executability**: graph và lifecycle hiện có cho phép một Ticket tiếp tục về mặt cơ học hay không;
- **dispatch readiness**: toàn bộ gate của Pulse đã pass và Ticket có thể được giao cho Agent.

Slice 2 chỉ sở hữu khái niệm thứ nhất. Mọi output phải ghi rõ `dispatch_authorized: false` cho tới khi các slice gate còn lại được tích hợp.

## Nguyên tắc

- Work graph là canonical source cho work status; event là audit evidence, không phải status store thứ hai.
- Mọi status mutation dùng expected-revision CAS, cùng lock/recovery protocol với Slice 1.
- Transition engine là pure deterministic library logic. CLI không tự invent transition hoặc condition.
- `blocked_by` là hard dependency; `preferred_after` chỉ là soft scheduling signal.
- `superseded` khác `done`; `cancelled` khác `done`; không trạng thái terminal nào được dùng để che acceptance chưa hoàn tất.
- Supersession dùng typed `superseded_by` edge khi có canonical replacement; Decision-only explanation được giữ như owner contract cho trường hợp chưa có replacement node phù hợp. Node không persist một duplicate replacement field.
- Roll-up, executability, affected set và inverse relations là projection, không persist thành child counters hay writable lists.
- `active` cần assignment lease; `verifying` cần frozen source identity; `done` cần close-gate receipts. Slice 2 không cho caller vượt các boundary này bằng một cờ `--force` chung chung.
- Semantic judgment — acceptance đã được hấp thụ chưa, ambiguity đã resolve chưa, rationale có đủ không — thuộc Agent/human/reviewer. Kernel chỉ validate contract và references mà slice hiện sở hữu.
- Không mở rộng storage thành generic database framework. Reuse concrete CAS/transaction helpers và tiếp tục expose typed graph APIs.

## Mục tiêu

Triển khai lifecycle/executability layer để có thể:

- CAS-transition node qua các bước hiện được slice hỗ trợ;
- thêm lifecycle value `cancelled` còn thiếu trong Slice 1;
- từ chối illegal transition bằng stable error code và allowed-target list;
- yêu cầu reason cho `blocked`, `rework`, `cancelled` và `superseded`;
- thực hiện supersession bằng node-status update + deterministic edge khi có replacement, hoặc node-status update + Decision reference khi Decision là canonical explanation;
- derive blocker state, supersession chain và structural executability cho từng Ticket;
- phân biệt hard blocker với soft preference;
- query bounded neighborhood và transitive affected set deterministic;
- derive Epic/Story roll-up mà không persist counters;
- invalid hóa cache đúng khi lifecycle/edge graph đổi;
- emit immutable events cho transition/supersession đã commit;
- cung cấp extension points rõ cho receipt/docs/shaping/lease gates ở các slice sau.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

- **#3, roll-up subset:** Epic → Story → Tickets derive child lifecycle summary từ independent nodes/edges.
- **#4:** hard blocker ngăn structural executability; soft preference chỉ xuất hiện như scheduling signal.
- **#16, graph/lifecycle subset:** node bị hấp thụ chuyển `superseded`, giữ identity/history và trỏ tới canonical replacement hoặc Decision explanation; slice chưa tự đánh giá semantic acceptance coverage.
- **#40, projection subset:** execution-oriented projection được derive từ cùng graph fingerprint và rebuild deterministic; claim/lease không persist thành status giả. Decision frontier vẫn defer tới shaping slice.

### Decisions liên quan

- D-02 đến D-07.
- D-15, D-17 đến D-22.
- D-25.
- D-43/D-44 chỉ ở boundary: Slice 2 không tự đánh giá shaping semantics nhưng phải chừa gate extension cho chúng.

### Slice exit

Slice hoàn thành khi lifecycle mutation, supersession và structural executability deterministic/recoverable, với output đủ giải thích tại sao một Ticket bị chặn hoặc chưa được phép dispatch.

Slice exit **không** đồng nghĩa:

- Ticket đã qua full `ready` gate;
- Agent có thể nhận lease;
- Ticket/Story có thể chuyển `done`;
- Phase 1 hoặc Core v1 hoàn thành.

## Non-goals

- Parse hoặc semantic-review `ticket.md`, `approach.md`, `qa.md` hay Decision prose.
- Shaping receipt, branch disposition, destination, fog hoặc decision frontier.
- Document registry, documentation impact, applicable docs hoặc docs retrieval.
- QA impact, Story baseline, affected case selection hoặc QA receipts.
- Generic receipt/evidence store và close-gate validator.
- Assignment lease, `active` ownership, run state, worktree hoặc handoff.
- Cho phép transition vào `active`, `verifying` hoặc `done` trước khi lease/source/receipt planes tồn tại.
- Full `pulse work packet`.
- Priority ranking hoặc automatic scheduling; Slice 2 chỉ expose hard/soft graph facts.
- Tự quyết định acceptance của node cũ đã được replacement hấp thụ đầy đủ hay chưa.
- Edge removal/delete/tombstone. Slice 2 không cần xóa edge để thực hiện supersession; delete recovery vẫn cần proposal riêng.
- Migration UX cho legacy `.pulse/workgraph/items.jsonl`; reboot Rust graph tiếp tục là implementation mới theo D-18/D-22. Cutover của public legacy workflow cần kế hoạch migration riêng, không được lẫn vào lifecycle semantics.

## Schema evolution

### Node status vocabulary

Slice 1 đã nhận diện:

```text
draft, shaped, ready, active, verifying, done, rework, blocked, superseded
```

Slice 2 bổ sung giá trị normative còn thiếu từ lifecycle owner document:

```text
cancelled
```

Proposal mặc định coi các slice Phase 1 hiện vẫn pre-contract, nhưng repository đã có fixture/schema Slice 1 trên disk. Vì vậy implementation phải có **explicit schema upgrade** thay vì chỉ đổi embedded constant:

1. acquire write fence và recover/drain mọi pending Slice 1 transaction;
2. nhận diện exact Slice 1 node schema hash/version;
3. validate toàn bộ existing nodes parse được theo typed old/new model;
4. atomic replace repository-owned node schema bằng version mới;
5. không overwrite schema khác exact known predecessor;
6. ghi migration event/receipt phù hợp với contract hiện có.

Đề xuất bump node schema lên `schema_version: 2` để thay đổi enum/object shape không bị hiểu nhầm là cùng contract. Nếu review quyết định giữ version 1 vì chưa public release, migration path trên vẫn bắt buộc vì `bootstrap_unlocked()` của Slice 1 không overwrite schema hiện có và validator đang so exact embedded schema. Pending transaction intent v1 phải được recover hết trước upgrade hoặc deserializer phải hỗ trợ cả hai versions; không được làm binary mới mất khả năng đọc recovery evidence cũ.

### Status reason

Node bổ sung optional field:

```jsonc
{
  "status": "blocked",
  "status_reason": {
    "code": "dependency_unavailable",
    "summary": "Waiting for storage recovery prototype",
    "reference": "TK-004"
  }
}
```

Contract:

- `code`: stable machine-oriented slug;
- `summary`: non-empty bounded human explanation;
- `reference`: optional work/document/receipt reference, chưa được hiểu là proof nếu plane tương ứng chưa có validator.

`status_reason` là explanation của **current state**, bắt buộc khi target là:

- `blocked`;
- `rework`;
- `cancelled`;
- `superseded`.

Tách riêng `transition_reason` trong immutable event cho các transition cần rationale nhưng target state không nên giữ explanation. Ví dụ `shaped -> draft` cần `transition_reason`, nhưng node sau commit không persist `status_reason` vì `draft` không phải một paused/terminal exception state.

Với `superseded`, canonical replacement dùng `superseded_by` edge. Khi chưa có replacement node phù hợp, owner contract cho phép `status_reason.reference` trỏ tới một Decision explanation; hai forms phải mutually exclusive và output phải phân biệt `replacement` với `decision_explanation`.

Khi transition sang trạng thái không yêu cầu persisted reason, engine clear `status_reason` để tránh stale explanation. Caller không được sửa trực tiếp status/reason qua generic edit.

### Không thêm duplicate state

Slice không thêm:

- `children` hoặc parent field vào node;
- `blocked_by` list vào node;
- `superseded_by` field vào node;
- roll-up counter;
- `is_ready`, `is_executable` hoặc frontier list persisted;
- actor/authority snapshot trong node.

Các dữ liệu trên thuộc edge, projection, event hoặc receipt plane.

## Lifecycle model

### State classes

```text
preparation: draft, shaped, ready
execution:   active, verifying, rework
paused:      blocked
terminal:    done, cancelled, superseded
```

`blocked` là paused state, không phải terminal. `rework` biểu diễn proof/review thất bại cần sửa, không phải alias của `active`.

### Canonical transition table

Transition engine biết full target lifecycle nhưng CLI Slice 2 chỉ mở các transition có gate support hiện hành.

| From | To | Slice 2 policy |
| --- | --- | --- |
| `draft` | `shaped` | gated/disabled tới khi có minimal source/revision-bound shaping assertion + authority identity; `shaped` không được dùng như synonym của “đã tạo content” |
| `draft` | `cancelled` | allowed, reason required |
| `draft` | `superseded` | chỉ qua supersede operation |
| `shaped` | `draft` | allowed để invalidate shaping cũ, reason required |
| `shaped` | `ready` | gated/disabled cho public CLI tới khi full required gate inputs tồn tại |
| `shaped` | `blocked` | allowed, reason required |
| `shaped` | `cancelled` | allowed, reason required |
| `shaped` | `superseded` | chỉ qua supersede operation |
| `ready` | `shaped` | allowed để invalidate readiness cũ, reason required |
| `ready` | `blocked` | allowed, reason required |
| `ready` | `cancelled` | allowed, reason required |
| `ready` | `superseded` | chỉ qua supersede operation |
| `ready` | `active` | gated/disabled tới Phase 2 lease integration |
| `active` | `blocked` | allowed only khi caller có future lease/run authority; public CLI Slice 2 không mở |
| `active` | `verifying` | gated/disabled tới Phase 2 source snapshot integration |
| `active` | `cancelled` | authority-gated; public CLI Slice 2 không mở |
| `active` | `superseded` | gated/disabled tới Phase 2 redirect, lease release và partial handoff integration |
| `verifying` | `done` | gated/disabled tới receipt/close-gate integration |
| `verifying` | `rework` | gated/disabled tới verification receipt integration |
| `verifying` | `blocked` | gated/disabled tới verification/run integration |
| `verifying` | `superseded` | gated/disabled tới Phase 2 frozen-source, redirect và partial handoff integration |
| `rework` | `shaped` | gated/disabled tới receipt/re-shaping integration |
| `rework` | `ready` | gated/disabled tới full ready gate |
| `rework` | `active` | gated/disabled tới lease integration |
| `rework` | `cancelled` | authority-gated; public CLI Slice 2 không mở |
| `rework` | `superseded` | gated/disabled tới receipt/rework handoff integration |
| `blocked` | `draft` | allowed only khi work contract bị reset, reason required |
| `blocked` | `shaped` | allowed, reason required |
| `blocked` | `ready` | gated/disabled tới full ready gate |
| `blocked` | `active` | gated/disabled tới lease + prior-state/run integration |
| `blocked` | `cancelled` | allowed, reason required |
| `blocked` | `superseded` | supersede operation |
| `done` | bất kỳ | immutable terminal trong normal API; correction cần explicit audited reopen contract ở slice close-gate sau |
| `cancelled` | bất kỳ | immutable terminal trong Slice 2; restore/reopen cần Decision riêng |
| `superseded` | bất kỳ | immutable terminal trong Slice 2; undo supersession cần edge-remove/history contract riêng |

Rationale:

- Slice 2 không dùng `--force` để giả những gate chưa implement.
- Transition engine có stable `transition_gate_unavailable` cho legal direction nhưng thiếu capability plane.
- Illegal direction dùng `illegal_transition`.
- Supersession không đi qua generic transition command vì cần edge + status invariant.

### Transition error contract

Ví dụ:

```json
{
  "schema_version": 1,
  "code": "transition_gate_unavailable",
  "subject": "TK-031",
  "from": "shaped",
  "to": "ready",
  "required_gate_families": [
    "implementation_contract",
    "shaping_authority",
    "documentation_impact",
    "qa_impact"
  ],
  "message": "transition direction is valid but required gate capabilities are not installed"
}
```

CAS conflict giữ contract của Slice 1. Không auto-retry semantic transition trên stale revision.

## Transition mutation protocol

Generic supported transition:

```text
1. acquire repository WriteGuard
2. recover hoặc refuse unresolved transaction
3. load current node + graph
4. compare expected revision
5. validate transition direction và capability gate availability
6. validate target-specific reason/invariants
7. update status, optional persisted status_reason, revision, updated_at trong một operation context; transition_reason chỉ đi vào event
8. validate full affected graph
9. prepare immutable lifecycle event
10. commit node + event qua Slice 1 transaction primitive
11. invalidate/rebuild disposable projection theo fingerprint on demand
12. release guard
```

Event type:

```text
work.node.transitioned
```

Payload tối thiểu:

```jsonc
{
  "from": "draft",
  "to": "shaped",
  "expected_revision": 2,
  "reason": null,
  "graph_fingerprint_before": "sha256:...",
  "gate_coverage": ["transition_direction", "graph_integrity"]
}
```

Fingerprint sau mutation được derive từ graph mới; event không được đưa vào graph fingerprint.

## Supersession operation

### Contract

```text
pulse work supersede <old-id>
  (--by <replacement-id> | --decision <decision-id>)
  --expected-revision <n>
  --reason "..."
  --actor <actor>
```

Preconditions deterministic:

- old tồn tại; caller chọn đúng một trong `--by` hoặc `--decision`;
- old đang ở `draft`, `shaped`, `ready` hoặc `blocked`; `active`, `verifying` và `rework` bị gate tới Phase 2 redirect/handoff integration; terminal states bị reject;
- với replacement form: replacement tồn tại, khác old, không `cancelled`/`superseded`, edge mới không tạo cycle và old chưa có live outgoing `superseded_by` khác;
- với Decision form: target tồn tại, kind là Decision, và old không đồng thời có outgoing `superseded_by` edge;
- expected revision của old khớp;
- reason non-empty;
- full graph valid trước commit.

Semantic precondition do caller/reviewer chịu trách nhiệm và event phải ghi assertion rõ:

- acceptance của old đã được replacement hấp thụ, hoặc
- missing acceptance đã được chuyển thành linked follow-up.

Slice 2 **không tự chứng minh assertion này từ prose**. Không khóa một stable public `disposition` schema trước khi receipt/authority identity được thiết kế. Trong slice này, supersession command nhận một **provisional caller assertion** được version rõ và lưu trong event:

```jsonc
{
  "assertion_version": 1,
  "asserted_by": "human:quannv",
  "source_revisions": ["TK-031@4", "ST-014@7"],
  "claim": "absorbed|follow_up_required",
  "references": ["ST-014", "TK-099"]
}
```

Rules cơ học:

- `follow_up_required` phải có ít nhất một referenced work item tồn tại;
- source work revisions phải khớp snapshot được caller review;
- event/output label rõ đây là assertion chưa receipt-validated;
- field names chỉ là internal/provisional Slice 2 contract và có thể được thay bằng typed receipt ở slice evidence sau.

Kernel chỉ kiểm tra identity/revision/reference existence; không tự nhận semantic coverage là thật. Vì vậy Slice 2 chỉ claim mechanical supersession, không claim hoàn thành acceptance reconciliation của roadmap #16.

### Atomicity boundary

Replacement-form supersession cần thay đổi hai canonical files:

1. old node status/revision;
2. deterministic `superseded_by` edge.

Decision-form supersession chỉ thay old node + event và có thể reuse single-target transaction của Slice 1. Với replacement form, Slice 2 phải **không** tuần tự ghi edge rồi node bằng hai transaction độc lập và gọi đó là một supersession atomic.

Proposal chọn một concrete direction cần prototype: **ordered multi-target roll-forward intent**, không claim rollback nếu không persist before images.

Intent tối thiểu phải giữ đủ dữ liệu để hoàn tất target chưa ghi sau crash:

```jsonc
{
  "schema_version": 2,
  "targets": [
    {
      "path": ".../nodes/TK-OLD.json",
      "before": {"hash": "sha256:...", "revision": 4},
      "after": {"hash": "sha256:...", "revision": 5},
      "after_bytes_base64": "..."
    },
    {
      "path": ".../edges/superseded-by--TK-OLD--ST-NEW.json",
      "before": "absent",
      "after": {"hash": "sha256:...", "revision": 1},
      "after_bytes_base64": "..."
    }
  ]
}
```

`after_bytes_base64` có thể được thay bằng content-addressed durable blob reference nếu prototype chứng minh simpler/safer, nhưng hash-only không đủ để roll forward target chưa được ghi. Requirements:

- intent + mọi referenced payload được fsync trước target đầu tiên;
- targets có deterministic order;
- recovery chỉ tự động khi observed state là all-before, planned prefix-after, hoặc all-after;
- all-before cleanup intent; prefix-after roll forward remaining targets từ durable after payload; all-after hoàn tất event;
- state ngoài planned set, missing/corrupt payload hoặc hash mismatch phải stop và preserve evidence;
- không claim rollback trừ khi một Decision sau bổ sung durable before-images;
- transaction intent v1 phải được drain/recover trước migration hoặc deserializer versioned phải đọc được cả v1/v2.

Phương án này tạo primitive concrete cho logical graph mutation nhưng chưa expose generic public transaction API.

### Read consistency boundary

Multi-target recovery không tự làm readers atomic. Slice 1 có read paths không giữ guard xuyên suốt load/validate/project, nên một reader có thể quan sát half-applied supersession trong process window nếu Slice 2 chỉ khóa writers.

Trước khi claim logical atomicity, Slice 2 phải chọn và test một read-consistency mechanism:

- mọi graph-semantic read (`show`, `list`, `validate`, `export`, executability/traversal) giữ repository guard xuyên suốt recovery + load + validation + projection; hoặc
- publish qua generation/snapshot switch có atomic read visibility.

Proposal mặc định chọn phương án đầu để giữ scope nhỏ, dù v1 có thể serialize readers với writers. Nếu không implement read consistency, acceptance phải hạ xuống “crash-recoverable eventual completion” và không được claim reader-visible atomic supersession.

Không dùng cache hoặc event payload như writable source thay thế cho canonical node/edge.

### Supersession event

Một logical operation emit đúng một event:

```text
work.node.superseded
```

Payload gồm old ID/revision, replacement + edge hoặc Decision explanation identity, status/transition reason, provisional caller assertion, references và graph fingerprints trước/sau nếu available deterministic.

Retry cùng operation khi node đã `superseded` và cùng canonical replacement/Decision explanation + payload có thể trả `unchanged`; retry khác target form, target identity hoặc caller assertion phải conflict rõ, không silently rewrite history.

Từ Slice 2, `superseded_by` trở thành lifecycle-owned relation. Generic `pulse graph edge add --type superseded_by` phải bị reject bằng stable code hướng caller sang `pulse work supersede`; đây là deliberate pre-contract breaking change so với Slice 1 để tránh graph half-valid không thể tạo qua hai independent commands.

## Structural executability

### Mục đích

Projection trả lời câu hỏi hẹp:

> Với canonical graph hiện tại, Ticket này có bị hard dependency, supersession hoặc lifecycle state ngăn tiếp tục hay không?

Nó **không** trả lời:

> Ticket đã đủ semantic context, docs, QA, authority và receipts để dispatch chưa?

### Output model

```jsonc
{
  "subject": "TK-031",
  "graph_fingerprint": "sha256:...",
  "structural_state": "candidate|blocked|paused|terminal|not_executable_kind|invalid",
  "dispatch_authorized": false,
  "lifecycle": {
    "status": "shaped",
    "revision": 4
  },
  "hard_blockers": [
    {
      "id": "TK-029",
      "status": "active",
      "resolution": "unsatisfied",
      "path": ["TK-031", "TK-029"]
    }
  ],
  "soft_preferences": [
    {
      "preferred_before": "TK-030",
      "status": "done"
    }
  ],
  "supersession": null,
  "gate_coverage": [
    "graph_validity",
    "lifecycle_state",
    "hard_dependencies",
    "supersession"
  ],
  "missing_gate_families": [
    "implementation_contract",
    "shaping_authority",
    "documentation_impact",
    "qa_impact",
    "receipts",
    "lease"
  ],
  "reason_codes": ["hard_blocker_open"]
}
```

### Structural state rules

- Chỉ Ticket là executable unit. Epic, Story và Decision trả `not_executable_kind`.
- `cancelled`, `done`, `superseded` trả `terminal`.
- `blocked` trả `paused` kể cả khi mọi graph blocker đã done; explicit status cần được transition lại, không auto-mutate.
- `draft` trả `blocked` với `work_not_shaped`.
- `shaped` hoặc `ready` có thể là `candidate` nếu không còn hard blocker và không bị supersede.
- `active`, `verifying`, `rework` không được đưa vào new-dispatch candidate; chúng trả state/reason phù hợp với run lifecycle nhưng Slice 2 không tự mutate.
- Graph invalid trả error/non-zero thay vì candidate rỗng.
- `preferred_after` không đổi `candidate`; nó chỉ xuất hiện ở `soft_preferences`.

### Hard blocker resolution

Slice 2 dùng **provisional mechanical resolver**, không tuyên bố đây là full normative satisfaction contract:

- target `done` → `satisfied` với `resolution_basis=terminal_done`;
- target `superseded` và canonical replacement chain kết thúc ở `done` → `satisfied` với chain/basis rõ;
- target `superseded` bằng Decision-only explanation hoặc replacement non-Ticket → `unknown_to_slice` cho tới khi typed Decision/outcome resolver tồn tại;
- target `cancelled`, còn open/paused/executing, superseded-to-open hoặc chain invalid → `unsatisfied`;
- authorized waiver, accepted Decision resolution hoặc receipt-based satisfaction → `unknown_to_slice` cho tới khi typed resolver tương ứng tồn tại.

Owner docs cho phép blocker được “terminal/satisfied” hoặc gate waive; vì vậy output phải expose `resolution_basis` và `missing_resolver_families`, không đồng nhất vĩnh viễn satisfaction với `done`. Với blocker là Decision hoặc non-Ticket prerequisite, Slice 2 không đoán completion semantics ngoài explicit status/edge facts.

Rationale: `superseded` không đồng nghĩa outcome hoàn tất, đồng thời Slice 2 không được loại bỏ future waiver/Decision/receipt paths bằng một rule quá hẹp.

### Readiness boundary

Slice 2 không thêm persisted `structurally_ready` status và không tự transition node sang `ready` chỉ vì projection trả `candidate`.

Các slice sau sẽ compose gate families:

```text
structural executability
+ implementation contract validation
+ shaping/authority receipt
+ docs impact + applicable docs
+ QA impact/baseline references
+ required Decisions/content references
= dispatch readiness
```

Full `pulse work ready` public contract chỉ nên được mở khi composition trên tồn tại. Slice 2 dùng command hẹp `pulse work executability` để tránh chiếm sai nghĩa `ready`.

## Roll-up projection

Epic/Story roll-up là read model, không status mutation tự động.

```jsonc
{
  "subject": "ST-014",
  "direct_children": 4,
  "descendant_tickets": 6,
  "by_status": {
    "draft": 1,
    "shaped": 1,
    "done": 3,
    "superseded": 1
  },
  "open_hard_blockers": ["TK-029"],
  "terminal_outcomes": {
    "done": 3,
    "cancelled": 0,
    "superseded": 1
  },
  "completion_claim": "not_evaluated"
}
```

Rules:

- deterministic sorted traversal;
- detect/report hierarchy cycle thay vì truncate silently;
- `superseded` child không được đếm như `done`;
- Story/Epic không auto-transition dựa trên counters;
- `completion_claim` luôn `not_evaluated` trong Slice 2 vì Story close còn cần QA/docs/evidence;
- standalone Ticket không cần roll-up parent.

## Neighborhood và affected queries

### Neighborhood

```text
pulse graph neighborhood <id> --depth <n> [--json]
```

Trả bounded subgraph gồm:

- subject node;
- outgoing/incoming typed edges;
- nodes reachable trong depth;
- direction và traversal path;
- graph fingerprint;
- deterministic ordering.

Defaults/bounds:

- default depth `1`;
- max depth nhỏ và explicit, đề xuất `5`;
- cycles không loop vô hạn;
- result có truncation metadata khi budget chạm giới hạn.

### Affected-by

```text
pulse graph affected-by <id> [--relation <type>] [--json]
```

Mặc định derive nodes có thể bị ảnh hưởng bởi thay đổi subject qua:

- reverse `blocked_by` (`blocks`);
- descendants/ancestors liên quan roll-up qua `parent`;
- nodes trỏ tới subject qua `superseded_by`/`duplicates` khi status/identity thay đổi;
- soft `preferred_after` được label `advisory`, không trộn với hard affected set.

Output phân loại `hard`, `rollup`, `supersession`, `advisory`; không tạo một flat list mất semantics.

Query chỉ là projection. Nó không tự mutate readiness hoặc scheduling.

## CLI surface của slice

```text
pulse work transition <id>
  --to <draft|shaped|blocked|cancelled>
  --expected-revision <n>
  [--reason-code <code>]
  [--reason <text>]
  [--reference <id-or-ref>]
  --actor <actor>
  [--json]

pulse work supersede <old-id>
  (--by <replacement-id> | --decision <decision-id>)
  --expected-revision <n>
  --reason <text>
  --assertion <versioned-json-file>
  --actor <actor>
  [--json]

pulse work executability <ticket-id> [--json]
pulse work rollup <epic-or-story-id> [--json]

pulse graph neighborhood <id> [--depth <n>] [--json]
pulse graph affected-by <id> [--relation <type>] [--json]
```

Existing commands tiếp tục giữ contract ngoại trừ deliberate lifecycle ownership change:

```text
pulse work create|show|list|edit
pulse graph edge add|validate|export|recover
```

`graph edge add` tiếp tục hỗ trợ các relation khác nhưng reject `superseded_by`; relation này chỉ được tạo bởi `work supersede`.

CLI transition target parser có thể nhận toàn lifecycle vocabulary để trả `transition_gate_unavailable` chính xác, nhưng help chỉ quảng bá target đã supported hoặc đánh dấu gated rõ ràng.

Deferred:

```text
pulse work ready
pulse work frontier --kind decision|execution
pulse work packet
pulse work claim|release
pulse work close
pulse graph edge remove
```

## Library/module layout đề xuất

```text
src/
  graph/
    lifecycle.rs          # transition table, gate requirements, status classes
    executability.rs      # hard blocker/supersession analysis + reason codes
    traversal.rs          # bounded neighborhood, affected-by, cycle-safe paths
    rollup.rs             # Epic/Story derived summary
    supersession.rs       # logical mutation validation/plan
    projection.rs         # extend export schema, keep deterministic cache
    store.rs              # CAS mutation orchestration, thin typed adapter
  storage/
    transaction.rs        # minimal multi-target extension for supersession
  schema/
    node.schema.json      # cancelled + status_reason

tests/
  lifecycle.rs
  executability.rs
  supersession_transaction.rs
  traversal.rs
  rollup.rs
  cli_lifecycle_contract.rs
```

Pure functions trong `lifecycle`, `executability`, `traversal`, `rollup` nhận typed graph snapshot; chúng không đọc filesystem hoặc emit CLI text.

`JsonGraphStore` tiếp tục sở hữu lock/recovery/load/validate/commit. Binary chỉ parse, call và render.

## Projection/cache evolution

`graph export` schema cần version bump vì thêm derived fields. Đề xuất:

```jsonc
{
  "schema_version": 2,
  "graph_fingerprint": "sha256:...",
  "nodes": [],
  "edges": [],
  "inverse": {},
  "lifecycle": {
    "status_classes": {},
    "structural_executability": {},
    "rollups": {}
  }
}
```

Rules:

- graph fingerprint algorithm/version không cần đổi chỉ vì projection có field mới; fingerprint vẫn hash canonical manifest/node/edge truth;
- cache key gồm projection schema version nên Slice 1 cache tự stale/rebuild;
- mọi map/list deterministic sort;
- executability reason order stable theo severity, relation type, subject ID và path;
- missing/corrupt old cache bị discard;
- projection không persist lease, claim, frontier list hoặc dispatch authorization giả.

Nếu output size lớn, `graph export` có thể giữ full derived map còn per-node command build bounded result. Benchmark Q2 quyết định optimization; correctness trước.

## Validation extensions

`pulse graph validate` bổ sung:

1. `cancelled` và `status_reason` schema/semantic checks;
2. required/forbidden stale status reason rules;
3. terminal-state invariants;
4. `superseded` node phải có đúng một trong hai forms: một live outgoing `superseded_by` edge, hoặc `status_reason.reference` trỏ tới một Decision hiện hữu; không được có cả hai;
5. node không `superseded` không được có outgoing `superseded_by` edge, trừ versioned recovery state đang được deterministic reconcile; generic edge-add không được tạo relation này;
6. replacement không được self-reference/cycle;
7. replacement chain endpoint phải tồn tại;
8. `done`/`cancelled` node không được supersede trong normal graph;
9. dependency/supersession traversal phải có bounded cycle diagnostics;
10. multi-target pending transaction validation/recovery;
11. projection schema/cache mismatch chỉ làm rebuild, không làm canonical graph invalid.

Validation không tự sửa orphan supersession edge hoặc status mismatch. Auto-recovery chỉ áp dụng planned transaction states đã chứng minh deterministic.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
| --- | --- | ---: | --- |
| L1 | `draft -> shaped` khi chưa có shaping assertion capability | lifecycle boundary | `transition_gate_unavailable`, không đổi node/event |
| L2 | stale expected revision | #5 reuse | `cas_conflict`, không đổi node/event |
| L3 | illegal transition `draft -> done` | lifecycle | `illegal_transition`, allowed targets rõ |
| L4 | legal direction nhưng gate chưa có `shaped -> ready` | readiness boundary | `transition_gate_unavailable`, không có `--force` bypass |
| L5 | blocked/cancelled thiếu reason | integrity | Reject trước commit |
| L6 | transition clear stale reason | integrity | Node mới không giữ explanation cũ |
| L7 | hard blocker open | #4 | Ticket không structural candidate; reason/path đúng |
| L8 | hard blocker done | #4 | Blocker satisfied |
| L9 | soft preferred work open | #4 | Candidate vẫn giữ; preference chỉ advisory |
| L10 | blocker superseded bởi replacement open | #4/#16 | Vẫn blocked |
| L11 | blocker superseded chain kết thúc `done` | #4/#16 | Satisfied, path giải thích chain |
| L12 | blocker cancelled | #4 | Không satisfied |
| L13 | supersede node bằng replacement hợp lệ | #16 subset | Old status + edge cùng commit, một logical event |
| L14 | supersede node bằng Decision explanation hợp lệ | #16 subset | Old status + Decision ref single-target commit, một event |
| L15 | supersede self/cycle hoặc cả replacement lẫn Decision | #16/integrity | Reject |
| L16 | supersede node đã done/cancelled | semantics | Reject |
| L17 | retry same supersession | idempotency | `unchanged`, không duplicate edge/event |
| L18 | retry different target/assertion | integrity | Conflict rõ, preserve history |
| L19 | crash sau node target, trước edge target | recovery | Multi-target recovery roll forward edge từ durable after payload; không claim rollback nếu không có before-image |
| L20 | crash sau cả targets, trước event | recovery | Event hoàn tất idempotent |
| L21 | ambiguous manual edit giữa supersession recovery | recovery | Stop, preserve intent/evidence |
| L22 | reader cạnh tranh với replacement supersession | consistency | Không quan sát half-valid state theo read-consistency contract |
| L23 | Story roll-up có done/superseded/open children | #3/#16 | Counts đúng; superseded không giả done |
| L24 | hierarchy cycle hand-edit | integrity | Validate/query fail với cycle path |
| L25 | neighborhood depth/order | query | Bounded, deterministic, cycle-safe |
| L26 | affected-by hard vs advisory | #4 | Categories không bị flatten |
| L27 | xóa cache sau lifecycle mutations | #7 reuse/#40 subset | Rebuild cùng fingerprint/projection semantics |
| L28 | non-Ticket executability | D-03 | `not_executable_kind` |
| L29 | draft Ticket không blocker | readiness boundary | `work_not_shaped`, `dispatch_authorized=false` |
| L30 | shaped Ticket không blocker | readiness boundary | `candidate`, nhưng `dispatch_authorized=false` và missing gate families đầy đủ |
| L31 | JSON CLI error/output schema | contract | Stable code/fields/non-zero exit |
| L32 | process-level concurrent transitions | concurrency | Một success, một CAS conflict |

Crash tests phải có failpoint tại từng target/event boundary của supersession và ít nhất một kill-process integration test. Pure transition/executability rules cần table-driven tests để mọi `(from, to)` có explicit expectation.

## Definition of Done của slice

- [ ] `cancelled` và `status_reason` được thêm vào typed Node + repository schema mà không phá deterministic serialization.
- [ ] Lifecycle transition table là pure/testable và không nằm rải trong CLI handlers.
- [ ] Supported transition dùng expected revision, operation context, graph validation, immutable event và Slice 1 recovery protocol.
- [ ] Illegal transition và missing capability gate có error code khác nhau.
- [ ] Không có generic `--force` cho `ready`, `active`, `verifying` hoặc `done`.
- [ ] `blocked`, `rework`, `cancelled`, `superseded` yêu cầu reason contract phù hợp.
- [ ] Supersession giữ old identity/history, tạo đúng deterministic edge khi có replacement hoặc Decision reference mutually exclusive, và không coi old là `done`.
- [ ] Supersession node+edge+event có crash-recoverable ordered roll-forward, durable after payload và reader-consistency contract được test; không dùng hai independent commits giả atomic.
- [ ] Retry same supersession idempotent; conflicting retry fail rõ.
- [ ] Structural executability phân biệt lifecycle, hard blockers, soft preferences, cancellation và supersession chains.
- [ ] Output structural executability luôn phân biệt `dispatch_authorized` và liệt kê missing gate families.
- [ ] Epic/Story roll-up deterministic, không persist counters và không claim completion.
- [ ] Neighborhood/affected-by bounded, deterministic, cycle-safe và giữ relation semantics.
- [ ] `graph validate` phát hiện status/edge mismatch, supersession cycle và ambiguous pending transaction.
- [ ] Projection schema/cache versioning làm old cache stale/rebuild an toàn.
- [ ] Process-level concurrent transition tests pass.
- [ ] Rust format, clippy và test suite sạch theo repository policy.
- [ ] Không hard-code docs/QA/shaping/lease assertions thành boolean placeholder có thể bypass authority.
- [ ] CLI/library boundaries vẫn thin CLI + typed core APIs.

## Handoff sang các slice tiếp theo

Sau Slice 2, thứ tự phụ thuộc đề xuất:

1. **Receipt identity + minimal evidence validator** — source/content-bound receipts và typed gate references.
2. **Documentation registry + applicable-doc projection** — document identity, owner, authority, scope và Ticket documentation impact.
3. **Docs section extraction + lexical retrieval** — generated `_index.md`, comrak sections, Tantivy cache và search/get.
4. **Shaping/readiness composition** — branch dispositions, shaping-map revision, authority receipt và full `pulse work ready`.
5. **Knowledge store foundations** — one-learning-per-record, typed relations, applicability/promotion metadata.

Có thể đổi thứ tự 2/3 với receipt foundation nếu implementation spike cho thấy docs receipts cần registry identity trước. Không được mở full `ready` hoặc Phase 2 dispatch cho tới khi gate composition từ các plane trên được chứng minh.

Slice 2 cung cấp extension points cần thiết:

- `TransitionGate`/gate-family result thay vì boolean ready;
- graph fingerprint + node revision binding cho future receipts;
- structural blocker/supersession reasons cho work packet;
- affected set để invalidate shaping/docs/QA readiness khi graph thay đổi;
- multi-target logical transaction cho graph mutations cần node + edge.

## Risks và open questions cho review

1. **Schema version:** proposal khuyến nghị bump node schema v2; review có chấp nhận không, và migration event/fixture policy cụ thể là gì?
2. **Multi-target recovery:** durable after payload + ordered roll-forward có đủ đơn giản và portable không? Phải prototype failpoints, payload corruption và fsync boundary trước khi work contract approve.
3. **Runtime intent loss:** crash model của Slice 1 vẫn phụ thuộc preserved `.pulse/runtime/`. Multi-target mutation làm giới hạn này quan trọng hơn; support boundary phải được nhắc lại, không claim unconditional audit completeness.
4. **Blocked resume:** có cần persist previous status để resume chính xác ở Phase 2, hay run/lease state sẽ sở hữu điều đó? Slice 2 tránh thêm field trước usage thật.
5. **Reopen terminal work:** correction của `done`/`cancelled`/`superseded` cần reopen event hay node mới? Slice 2 giữ terminal immutable để không khóa contract vội.
6. **Supersession target kind:** replacement edge có thể trỏ cross-kind tới Story không, hay Story/new approach chỉ nên là Decision explanation + follow-up Tickets? Structural blocker chỉ tự resolve với replacement outcome có terminal semantics rõ; cần fixture review.
7. **Supersession acceptance assertion:** provisional caller assertion chỉ kiểm tra identity/revision/reference, không semantic coverage. Slice receipt/reviewer sau phải bind authority thế nào và migrate event field ra sao?
8. **Hard blocker satisfaction qua chain:** replacement `done` có luôn đủ, hay cần explicit resolution edge/receipt? Slice 2 chỉ cung cấp provisional mechanical basis và giữ extension point cho waiver/Decision/receipt.
9. **Node status `blocked` vs derived blockers:** explicit blocked status và graph blocker có thể lệch. Proposal không auto-mutate; query phải giải thích cả hai. Có cần doctor finding cho stale blocked state ở Phase 3?
10. **Projection size:** full per-node executability trong `graph export` có thể lớn O(V×E). Benchmark Q2 quyết định precomputed indexes/incremental cache, không thay correctness contract.
11. **Affected-by semantics:** relation nào là hard invalidation và relation nào chỉ advisory cần giữ typed output để future readiness composer không over-invalidate.
12. **Legacy JSONL cutover:** repository hiện còn một số active docs/skills mô tả `.pulse/workgraph/items.jsonl`. Reboot implementation không được tạo hai canonical truths; cần migration proposal riêng trước khi Rust CLI trở thành public replacement.
13. **Actor identity:** Slice 1 đang không nhất quán giữa default `human:unknown` và required edge actor. Slice 2 nên bắt actor explicit cho lifecycle mutation và chuẩn bị typed actor contract, nhưng không mở full Agent Registry.
14. **Status reason privacy/size:** summary phải bounded và không chứa raw prompt/secret. Exact limit/redaction policy cần chốt trong schema/test.

## Không quyết định trong slice này

Slice này không chốt semantic shaping quality, docs applicability, QA coverage, receipt authority, assignment ownership, close gate hoặc scheduling priority. Nó chỉ chốt cơ học để các contract đó có thể dựa vào một lifecycle graph có CAS, audit, supersession đúng nghĩa và explainable structural blockers mà không tạo nguồn sự thật thứ hai.
