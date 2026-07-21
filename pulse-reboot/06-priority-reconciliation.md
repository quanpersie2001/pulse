# Priority Và Semantic Reconciliation

[Trang vào](../PULSE_REBOOT.md) | [Work graph](02-work-graph.md) | [Cross-agent coordination](05-cross-agent-coordination.md)

**Đọc khi:** cần chọn việc tiếp theo, xử lý foundation work, dependency hoặc Ticket bị hấp thụ.
**Sở hữu:** priority semantics, scheduling inputs, reconciliation loop và decision outputs.

## Priority không phải thứ tự thực thi

`P0`, `P1`, `P2` diễn tả urgency/impact, không phải một phép sort đủ để dispatch.

Ví dụ:

- X là P0 và có thể làm ngay, nhưng implementation hiện tại sẽ tạo debt lớn.
- Y là P2, ngắn hơn và tạo foundation giúp X được giải quyết trọn vẹn.
- Nếu lợi ích foundation lớn hơn chi phí trì hoãn, thứ tự đúng là Y rồi X.

Ngược lại, không được lấy “làm foundation trước” làm lý do vô hạn để trì hoãn incident P0. Quyết định phải ghi cost, expected unlock và deadline.

## Scheduling inputs

Reconciliation xem đồng thời:

- Frontier kind: decision work đang làm rõ đường đi hay execution work đang giao implementation.
- Destination relevance và shaping exit condition của owning effort khi có persisted map.
- Urgency/impact (`P0..P3`).
- Hard dependencies (`blocked_by`).
- Soft ordering (`preferred_after`).
- Foundation value: work nào giảm risk/complexity cho nhiều node khác.
- Supersession/absorption: node nào không còn outcome độc lập.
- Cost of delay và deadline.
- Estimated effort/uncertainty.
- Risk và blast radius.
- Agent capability/capacity và workspace conflict.
- Current partial progress/sunk cost.
- Verification/QA/docs review availability.
- Documentation foundation value và blocking contract gaps.

Một weighted score có thể dùng để lọc, nhưng semantic Agent/human phải giải thích quyết định cuối.

## Hard và soft relation

- `blocked_by`: X không executable nếu Y chưa hoàn thành hoặc gate không waive.
- `preferred_after`: X vẫn executable nhưng Y trước có thể cho shape tốt hơn.
- `related`: chỉ cung cấp context.
- `superseded_by`: X không nên tiếp tục như một outcome độc lập.

Không biến mọi preference thành blocker vì sẽ đóng băng graph. Không bỏ preference khỏi model vì priority flat sẽ tạo giải pháp ngắn hạn kém.

## Reconciliation questions

Agent phải hỏi:

1. Việc nào tạo hoặc bảo vệ giá trị khẩn cấp nhất?
2. Đang chọn từ decision frontier để làm rõ contract hay execution frontier để giao implementation?
3. Node nào thật sự executable ngay?
4. Có foundation hoặc decision work nhỏ nào unlock hoặc làm giảm mạnh risk cho work quan trọng hơn không?
5. Có hai Tickets đang giải cùng outcome không?
6. Ticket nào sẽ bị một Story/Decision/new approach hấp thụ?
7. Resolution mới có graduate fog, invalidated branch hoặc làm đổi destination relevance không?
8. Partial work hiện tại có làm thay đổi chi phí redirect không?
9. Agent/worktree/QA capability nào đang là bottleneck?
10. Có docs foundation/repair nhỏ nào unlock context hoặc ngăn implementation sai lặp lại không?
11. Nếu trì hoãn P0, deadline và trigger quay lại là gì?
12. Quyết định này có cần human vì vượt authority hoặc business judgment không?

## Reconciliation cadence

Không reconcile sau từng tool call. Reconcile tại các điểm có thông tin mới đáng kể:

- Khi backlog hoặc run bắt đầu.
- Sau mỗi 3-5 Tickets hoàn thành.
- Khi blocker/Decision làm đổi dependency graph.
- Sau shaping resolution làm đổi decision frontier, graduate fog hoặc invalidate downstream work.
- Khi xuất hiện P0/P1 mới.
- Khi Ticket bị rework nhiều lần.
- Khi capacity/capability của Agent thay đổi.
- Trước một dispatch batch multi-agent.

## Decision output

Mỗi reconciliation tạo một event/receipt:

```yaml
decision_id: rec_01J...
graph_revision: 42
ordered_candidates: [TK-Y, TK-X, TK-Z]
actions:
  - type: prefer_after
    subject: TK-X
    target: TK-Y
  - type: supersede
    subject: TK-OLD
    target: ST-NEW
rationale:
  - "TK-Y giảm thay đổi auth ở ba Tickets và mất khoảng 2 giờ."
revisit_after:
  completed_count: 3
hard_deadline: 2026-07-18T10:00:00Z
confidence: medium
```

Output phải phân biệt:

- Graph mutations được kernel validate/CAS.
- Proposed mutations cần human approval.
- Scheduling preference chỉ áp dụng cho dispatch window hiện tại.

## Supersession

Nếu outcome của X được Y hấp thụ:

1. So sánh acceptance của X với Y; không supersede chỉ vì title giống.
2. Ghi `X.superseded_by = Y` hoặc Decision trung gian.
3. Chuyển acceptance/evidence còn thiếu sang Y.
4. Nếu X active, cancel/redirect qua typed mailbox và thu partial handoff.
5. Giữ lịch sử X; không xóa như chưa từng tồn tại.
6. Recompute roll-up và scheduling.

`superseded` khác `done`: outcome không được hoàn thành độc lập mà được hấp thụ ở nơi khác.

## Agent tranh luận để reconcile

Có thể dùng hai Agent độc lập đề xuất ordering khi stakes cao:

- Agent A tối ưu cost-of-delay và delivery.
- Agent B tối ưu architecture/risk/foundation.
- Orchestration Agent hoặc human tổng hợp thành Decision.

Đây là review/judgment task có receipt, không phải bỏ phiếu theo số đông. Không dùng debate mặc định cho backlog nhỏ vì đó lại thành ceremony.

## Guardrails

- P0 bị trì hoãn vì foundation phải có timebox và revisit trigger.
- Orchestrator không tự hạ priority business nếu policy không cấp quyền.
- Scheduling decision phải gắn graph revision để tránh dùng trên backlog đã thay đổi.
- Không dispatch work có hard blocker hoặc exclusive lease sống.
- Public/safety documentation gap có thể là hard blocker; cosmetic cleanup thường chỉ là soft preference.
- Reconciliation failure hoặc docs-context failure lặp lại phải trở thành eval/skill improvement.
