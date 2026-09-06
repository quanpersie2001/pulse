# Handoff: Pulse — Track B vòng 1 xong (6 Ticket + 1 Story đóng bằng receipt)

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push; tag `v0.1.0` vẫn ở
  `16a0ef3`)
- HEAD round này, 8 commit:
  - `074fabf` fix(evidence): fingerprint bền qua tiến hoá envelope — bỏ
    collection rỗng khỏi canonical form; 8 receipt cổ của golden path đã
    chặn mọi packet build trước đó
  - `f7acd8d` fix(completion): duplicate passed verification cùng actor
    không còn là `close_verification_ambiguous`; anchor = id thấp nhất
  - `c3778da` feat(run): rework dispatch — nhả lease cũ, build lại packet
    với rework observation, `rework -> active`; schema packet khớp shape
    object; status_reason bị chặn ≤500 ký tự khi ghi
  - `40b8c46` fix(packet): ticket `decision_work` dispatch được qua runner
  - `e27b5e4` fix(completion): verify evidence receipts trước khi giữ fence
    (handoff `--evidence-receipt` từng tự-deadlock)
  - Ba commit dogfood: shaping 6 Ticket + ST-002; TK-003; TK-004 (rework
    thật 1); TK-006+TK-007 (song song, worktree); TK-005 (R2, rework thật
    2, qa checkpoint, close-story); TK-008 (rework thật 3); LRN-002
- Ba gate xanh: `cargo fmt --check`, `cargo clippy --all-targets --quiet
  -- -D warnings`, `cargo test --all-targets` (561 test, default
  threading).
- Working tree: sạch.

## Track B vòng 1: kết quả

- `examples/todolist/`: TK-003 (R0 count), TK-004 (R1 corrupt state),
  TK-005 (R2 due dates, plan.md + validation.md), TK-006 (decision_work,
  schema evolution), TK-007 (R1 rename), TK-008 (R0 help) — tất cả `done`
  bằng receipt; ST-002 `done` qua `close-story` với qualification
  full-baseline. Tiêu chí phụ golden path (§7) giờ đạt.
- Đảo vai cả hai chiều: worker=codex/reviewer=claude (TK-003/004) rồi
  worker=claude/reviewer=codex (TK-005); TK-006 chạy `--isolation
  worktree` song song với TK-007 trong checkout; refusal
  `run_isolation_required` được chứng minh trước khi dùng cờ.
- 3 chu kỳ rework thật (TK-004, TK-005, TK-008 — cùng một gap: receipt
  docs không được tham chiếu trong handoff). Shape finding
  `check|owner|severity` hoạt động đúng thiết kế.
- LRN-002: capture (harness) → validate (evidence rcpt) → promote thật
  vào `AGENTS.md` dưới Constraints. LRN-001 được áp dụng thật khi recovery
  TK-007.
- `examples/todolist/works/friction.md`: 15+ mục ma sát cụ thể, đủ làm
  backlog v0.2.

## Việc tiếp theo (đề xuất, theo trọng lượng)

1. **Gap worktree (to nhất, chưa sửa)**: run workspace (worker-prompt.md,
   worker-input.json) chỉ được ghi vào runtime của repo chính nên worker
   worktree không tự định vị được; TK-006 thành công là do accidentally
   ghi vào main checkout. Sửa cần mapping CLI worktree→main (reservations
   nằm ở main). Đây là ứng viên ADR **0015** kèm đề xuất gộp hai họ
   receipt (`evidence/execution/*` vào envelope chung — PRODUCT §11).
2. **Backlog v0.2 từ friction.md**: chụp stdout tail của reviewer vào run
   record (2 vòng chẩn đoán đã mất vì mất text lỗi); phân công ai ghi docs
   receipt (worker hay reviewer — hiện mơ hồ, gây 2/3 rework); `work
   ready` không dispatch; actor syntax `kind:id` bị ngầm hoá; canonical
   hoá `authority.json` không có lệnh; `docs validate --record` cần
   `--actor` báo sau cùng.
3. **Quyết định cần human**: có tính v0.2 ngay (MCP server, `pulse
   doctor`) hay thêm một vòng Track B (7–10 Ticket nữa,并行 nhiều hơn) để
   giải mã sát hơn. PRODUCT §11 ưu tiên ma sát thật trước feature mới —
   friction.md hiện đã đủ nặng để làm doctor/ADR.
4. Nếu có quyết định kiến trúc mới: ADR vào `docs/decisions/`, số tiếp
   theo **0015**.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục
một commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở
gốc; chạy thật chỉ trong `examples/todolist/`. Sửa core chỉ khi ma sát bắt
buộc; mỗi fix có test hồi quy. Commit kết bằng `Co-Authored-By: Claude
Fable 5.1 <noreply@anthropic.com>`.

## Kết quả mong đợi của vòng tới

- ADR 0015 chốt hướng worktree workspace + gộp receipt family (nếu human
  duyệt), hoặc vòng Track B 2 chứng minh thêm ma sát.
- Mỗi mục friction.md nặng chuyển thành Ticket v0.2 hoặc fix core có test.
- Không đổi PRODUCT.md khi chưa có ADR.
