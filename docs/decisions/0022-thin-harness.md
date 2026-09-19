# Decision 0022: Pulse v3 — giữ hợp đồng, bỏ máy móc

## Status

Accepted, 2026-09-16.

Kế hoạch thực hiện đầy đủ: [`docs/plans/0022-thin-harness.md`](../plans/0022-thin-harness.md)
(mục 3–14 vẫn là kế hoạch sống; mục 1–2 của file đó nay là quyết định này).
Số đo baseline/hiện tại: [`docs/plans/0022-metrics.md`](../plans/0022-metrics.md).

**Đánh dấu superseded in scope** (nội dung vẫn đúng cho v2, không còn áp dụng
cho v3; bài học giữ lại, không xoá quyết định):

- [0009 — Workflow trong repo đích, skill theo artifact trên CLI](0009-skill-surface-over-cli.md)
- [0010 — QA baseline là markdown heading, JSON chỉ ở biên runner](0010-qa-baseline-markdown-contract.md)
- [0012 — Thang bằng chứng, reviewer là bằng chứng, lane độc lập và lead hoà giải](0012-evidence-ladder-independent-lanes.md)
- [0013 — Bàn giao phiên theo ngưỡng của host](0013-session-handoff-host-threshold.md)
- [0014 — Pulse ghi `qa_checkpoint`, runner chỉ in output](0014-pulse-records-qa-checkpoint.md)
- [0015 — Worktree dispatch: workspace trong worktree, state về repo chính](0015-worktree-dispatch-workspace-state.md)
- [0016 — Worker sở hữu docs receipt, gate chặn tại handoff](0016-docs-receipt-ownership-at-handoff.md)
- [0017 — Receipt không đọc được phải được báo, không được xoá khỏi danh sách](0017-unreadable-receipts-are-reported-not-erased.md)
- [0018 — Knowledge plane: đường ra của vòng đời và retrieval có corpus riêng](0018-knowledge-plane-retrieval-and-lifecycle-exits.md)
- [0019 — Ba tầng hướng dẫn, và đúng một skill sở hữu việc tạo node](0019-guidance-layers-and-single-node-owner.md)
- [0020 — Gộp workflow thường ngày vào khối AGENTS](0020-collapse-guidance-into-agents.md)
- [0021 — Prose trước graph, và planning vào một lần](0021-prose-before-graph-and-one-planning-entry.md)

Không superseded: 0001–0008 (repo/scope), 0011 (event log JSONL theo ngày —
mục 5.3 giữ nguyên khung, gọn lại type list).

## Context

### Số đo, không cảm giác

Audit 2026-09-16 (`pulse-thin-harness-audit.html`) đo cây hiện tại: 43.106
dòng Rust trong `src/`, ~60 lệnh CLI leaf, 271 mã lỗi riêng biệt. Track B
(`docs/dogfood-friction-track-b.md`, TK-003..TK-008, tháng 9/2026) là dogfood
đo được duy nhất từng chạy: phần lớn friction trong đó là lỗi Pulse core
(parser wrapped-bullet, canonical receipt, deadlock viết-trong-đọc, worktree
dispatch quên mirror workspace, danh sách receipt rỗng-vì-không-đọc-được) —
không phải friction của thiết kế work graph. Bảy quyết định 0012–0018 xây
thang bằng chứng, knowledge plane, worktree dispatch, session handoff để
giải quyết đúng những gì Track B gặp; máy móc đó nay là phần lớn của
43.106 dòng.

### Máy móc không được dogfood xác nhận cần

Không dogfood nào từng chạy: docs registry/index/search/cache/applicability,
knowledge relation graph, materialization R0–R3, authority policy, priority
field, mini-DSL trên flag `--reason-code`, `works/` làm plane cho work item
(khác với `works/_drafts/` mà 0021 vừa thêm cho prose), 8 skill. Mỗi cơ chế
này được quyết định thêm vào để giải quyết một friction *suy luận trước*, không
phải một friction *đo được*. Chi phí giữ chúng là thật (43k dòng, 271 mã lỗi,
~60 lệnh) trong khi lợi ích chưa có bằng chứng.

### Bài học Track B vẫn đúng, tách khỏi máy móc thực hiện chúng

Đọc kỹ từng entry của Track B, cái đúng không phải "cần nhiều cơ chế hơn" mà
"hợp đồng giữa worker/reviewer/gate phải chặt và lỗi phải nêu đúng chỗ":
verifier khác actor với worker (0012), reviewer nhận claim để tự kiểm chứ
không nhận lời kể (0016 phần rework), dirty fence phải nêu đúng path lệch
(entry 2026-09-07), danh sách receipt không đọc được phải báo chứ không rỗng
im lặng (0017), rework phải mang finding có `check` chạy lại được (0012, vẫn
đúng). Những nguyên tắc này giữ nguyên trong 0022 mục 4–8; đổi là *cách thực
hiện* (một schema JSONL thay vì graph node + workgraph + policy file + hai họ
receipt), không phải nguyên tắc.

## Decision

Pulse v3 **giữ hợp đồng, bỏ máy móc**:

- **Giữ**: Ticket là đơn vị một agent một session; `done` chỉ do gate đọc
  evidence quyết; verifier khác actor với worker; reviewer nhận claim chứ
  không nhận lời kể; finding có `owner` + `check` chạy lại được; friction →
  learning → check; ADR cho quyết định giữ nguyên.
- **Bỏ**: authority policy, docs registry/index/search/cache/applicability/
  receipt, knowledge relation graph, materialization R0–R3, priority field,
  mini-DSL trên flag, hai họ receipt (gộp một), `works/` làm plane cho work
  item (record chuyển vào `issues.jsonl`), `work sync`/`brief_hash`, parser
  heading markdown cho Ticket/Story, worktree mirroring/state routing, 8
  skill (gộp còn 4).
- **Thêm đúng ba thứ** máy móc mới: `pulse checkpoint` + vòng `continue`
  trong runner (thay timeout cứng bằng một điểm lưu tiến độ tường minh),
  `pulse board` (một view, thay cho search/index render), store
  `issues.jsonl` có schema (thay workgraph + docs registry + knowledge
  store).

Chi tiết data model, gate, lane, runner, packet, learnings, guidance surface:
mục 4–13 của `docs/plans/0022-thin-harness.md`. Chi tiết đó là thiết kế thực
hiện, sống trong plan (sẽ cập nhật khi implement lệch), không lặp lại ở đây.

### Điều kiện thêm lại

Mọi cơ chế bị bỏ ở trên chỉ được thêm lại khi **một dogfood đo được** — tức
chạy Pulse thật trên một repo đích, ghi qua `pulse note --friction` hoặc
tương đương — cho thấy **≥ 2 friction cùng loại** mà đúng cơ chế đó giải
quyết. Suy luận trước ("agent sẽ cần tìm docs bằng ngữ nghĩa") không đủ; đó
là cách 43k dòng hiện tại hình thành.

## Alternatives Considered

1. **Sửa từng cơ chế tại chỗ (giữ kiến trúc, giảm lỗi/flag).** Loại: audit
   cho thấy phần lớn 271 mã lỗi và độ phức tạp CLI đến từ *số lượng cơ chế*,
   không phải chất lượng từng cơ chế. Sửa tại chỗ giữ nguyên 43k dòng.
2. **Viết lại từ đầu, không giữ hợp đồng cũ.** Loại: hợp đồng (evidence gate,
   verifier độc lập, finding có check) là phần đã được Track B xác nhận hoạt
   động; bỏ nó cùng máy móc là bỏ luôn cái đã đúng.
3. **Giữ nguyên, chờ dogfood thứ hai trước khi cắt.** Loại: dogfood thứ hai
   trên cây 43k dòng sẽ tốn chi phí vận hành ngang Track B trong khi phần lớn
   friction dự đoán được sẽ lặp lại (cùng lớp lỗi mà 0022 đã xác định qua
   audit tĩnh, không cần chạy lại để biết).

## Consequences

- Số đo mục 2 của plan là gate: Phase 0/1 không coi là xong nếu
  `src/` không xuống dưới ngưỡng theo từng phase; xem `0022-metrics.md`.
- Toàn bộ code, test, skill của các cơ chế bị bỏ bị xoá (không `#[ignore]`,
  không giữ "để tham khảo") — git history là nơi tham khảo. Xem plan mục 14
  cho số phận từng file.
- `PRODUCT.md`, `ARCHITECTURE.md`, `ROADMAP.md` mô tả v2; đến Phase 3 (P3.4)
  mới thay bằng `SPEC.md` ≤ 300 dòng. Cho tới lúc đó, khi ba file này và plan
  0022 mâu thuẫn, plan 0022 thắng (banner thêm vào `PRODUCT.md` ở P0.4).
- Không có dogfood target nào chạy Pulse hiện nay; cho tới khi Phase 2 tạo
  `~/Workspace/Personal/todolist` (P0.2, đã chốt), điều kiện thêm lại máy móc
  không thể được thoả — đây là lý do Phase 1 (xoá) đi trước Phase 2 (chạy
  thật): không cơ chế nào được giữ lại "phòng khi cần" trong lúc chờ dogfood.

## Verification

- `docs/plans/0022-metrics.md` cập nhật cuối mỗi phase với số đo thật, không
  ước lượng.
- `cargo fmt --check && cargo clippy --all-targets --quiet -- -D warnings &&
  cargo test --all-targets` xanh sau mỗi commit của plan mục 14.
- `tests/graph/architecture_guards.rs` và `tests/public_api_contract.rs` cập
  nhật cùng commit đụng tới layer chúng canh.
- Điều kiện dừng (plan mục 14, "Điều kiện dừng"): sau v3.0, không cơ chế nào
  trong danh sách đó quay lại nếu không có ≥ 2 friction cùng loại ghi trong
  `.pulse/events` của dogfood.

## Required changes

- `docs/decisions/README.md`: gắn nhãn "superseded in scope by 0022, lessons
  retained" cho 0009, 0010, 0012–0021.
- `PRODUCT.md`: banner 3 dòng đầu file trỏ về plan 0022 (P0.4).
- Từng file nguồn theo bảng "Số phận từng file hiện tại" ở plan mục 14.
