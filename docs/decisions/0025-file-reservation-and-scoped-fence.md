# Decision 0025: File reservation và fence theo scope

## Status

Accepted, 2026-09-18 (owner: quan). Triển khai Pha B của
[plan 0025](../plans/0025-parallel-verified-learning.md).

**Trạng thái thực thi:** phiên hiện tại chỉ thi hành phần
claim/ready/scope — `touches` trên schema + ready gate (B1), phép giao
`kernel::scope` (B2), claim theo reservation (B3). Phần fence theo scope
(scoped snapshot, `close` so fence thay vì so HEAD, B6) là việc của phiên
sau; cho tới khi đó, fence vẫn là snapshot cả cây như hôm nay.

## Context

`acquire_lease` cấm song song tuyệt đối (`run_another_active`: một ticket
active duy nhất), nên graph cho phép song song nhưng harness không chạy
được. Muốn mở song song trong **cùng một checkout** (plan 0022 §10.6 đã
cắt worktree mirroring và không mở lại mặc định), cần một khoá thô hơn
lease: biết mỗi ticket định **sửa** những file nào.

Các phương án đặt khoá đã cân nhắc:

- Kéo `context.anchors` làm khoá — sai ngữ nghĩa: anchors là chỗ **đọc**
  (`path: what lives there`, ready gate kiểm tra tồn tại), còn khoá song
  song phải là chỗ **sửa**. Một ticket đọc `src/lib.rs` để hiểu bối cảnh
  không có nghĩa nó được viết vào đó, và ngược lại file mới tạo chưa tồn
  tại để làm anchor.
- Worktree-mỗi-ticket — đúng hơn về cô lập, nhưng đưa trở lại đúng cơ chế
  mirroring plan 0022 §10.6 đã cắt kèm mọi chi phí sync state của nó.

## Decision

1. **`touches` là mảng riêng trên Ticket**, schema song song
   `description`: mỗi entry là đường dẫn repo-relative hoặc glob theo đúng
   ngữ pháp `source::glob_match` (exact, `dir/`, `dir/**`, một `*` trong
   một segment). Anchors = chỗ đọc, touches = chỗ sửa; không trộn.
2. **Song song trong cùng một checkout.** Không worktree. Claim của một
   ticket thành công khi `touches` của nó không giao với tập đang giữ của
   ticket nào khác; giao → `claim_files_reserved`, worker dừng hoặc host
   xếp lại.
3. **Fence của ticket có `touches` chỉ hash nội dung các file khớp
   `touches`** (tracked lẫn untracked, path + bytes), **không** so `HEAD`.
   Một ticket khác commit giữa chừng không làm ticket này stale; chỉ thay
   đổi trong scope của chính nó mới làm. (Phần này là B6 — chưa thi hành
   ở phiên hiện tại, xem Status.)
4. **Ticket không có `touches` = độc quyền**: giao với mọi ticket khác.
   Dữ liệu cũ và lối nhanh `risk: low` giữ nguyên hành vi hôm nay — chạy
   một mình.
5. **File được giữ qua cả `verifying`**, tới khi ticket `done` hoặc lease
   được release. Lý do: close so fence của handoff; nếu ticket khác sửa
   được file đó trong lúc review thì close stale mãi mà không có lỗi nào
   gọi tên nguyên nhân.
6. **Mỗi worker song song phải có actor riêng** (`agent:worker-1`,
   `agent:worker-2`, …): một actor giữ lease sống trên hai ticket bị từ
   chối (`claim_actor_busy`). Danh tính vẫn là tự khai — chống gian lận có
   chủ đích là việc của hook host (plan 0025 Pha G), không phải của luật
   claim.
7. **Loại worktree-mỗi-ticket** ở thời điểm này: chi phí mirroring và sync
   state (0022 §10.6) lớn hơn lợi ích cho quy mô hiện tại. **Ngưỡng xét
   lại:** friction "build gãy chéo giữa hai worker" vượt 1 lần / story
   trong dogfood.
8. **Hai rủi ro chấp nhận, ghi lại đây:**
   - *Build gãy chéo*: reserve theo file không chặn được worker A làm hỏng
     build của worker B qua file B chỉ **đọc** (khớp `touches` của A, chỉ
     nằm trong `context.anchors` của B). Giảm nhẹ: hai ticket đụng chung
     khu vực nên có cạnh `blocked_by` ngay từ lúc plan; về sau `pulse
     verify` chạy lại tại handoff (Pha D).
   - *`lane_mutated_workspace` thu hẹp*: khi fence theo scope, gate phát
     hiện lane sửa workspace chỉ còn **trong scope của ticket đó** — một
     lane sửa file ngoài scope không còn bị fenced bắt. Chấp nhận như một
     phần của quyết định 3; cân nhắc lại cùng ngưỡng ở mục 7.

## Consequences

- Schema thêm một field tùy chọn; dữ liệu cũ không có `touches` vẫn hợp lệ
  và hành vi như cũ (độc quyền).
- `run_another_active` biến mất, thay bằng `claim_files_reserved` +
  `claim_actor_busy`; mọi consumer của code cũ (doctor, skill, template)
  phải nói đúng tiếng mới.
- Ready gate thêm một điều kiện: ticket implementation `risk != low` phải
  khai `touches` — planner giờ chịu trách nhiệm vẽ bản đồ xung đột, không
  phải worker tự phát hiện giữa chừng.
- Đo ở dogfood (plan 0025 B8): số lần `claim_files_reserved` và số lần
  build gãy chéo là hai số quyết định xem quy tắc 7 giữ hay phải xét lại.
