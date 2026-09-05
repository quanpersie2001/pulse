# Handoff: Pulse — Bước 6 xong, v0.1 chốt; đợt tiếp theo là dùng thật

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `260e021` (18 commit của Bước 6 trên `845ff01`, mỗi mục một commit)
- Working tree: chỉ còn các sửa docs ĐANG LÀM DỞ của luồng khác (xem
  "Cảnh báo working tree" dưới) — không đụng tới, không commit hộ.
- Ba gate xanh trên HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` (530 test, default
  threading). Tag `v0.1.0` đã đặt tại `16a0ef3`, chưa push.

## Bước 6 đã đạt

1. **6.1 Mốc v0.1** — dọn `Oops.rej.orig`; README + PRODUCT.md §7 (dòng Đạt),
   §8, §13 (quyết định 13.2–13.5); tag `v0.1.0`.
2. **6.2 Isolation** — `pulse run` TỪ CHỐI với `run_isolation_required` (nêu
   Ticket giữ lease) khi có lease sống của Ticket khác; worktree chỉ khi
   `--isolation worktree`; xoá `auto_isolation`.
3. **6.3 Rationale/Reason** — hai mục docs/QA impact đều đọc `Rationale`,
   nhận `Reason` làm alias; template dùng `Rationale`. TK-001/TK-002 không
   phải sửa.
4. **6.4 Story close chạy thật** — `pulse run qa --scope story_close --story
   ST-001` (auto-chọn TK-001), receipt `qa_checkpoint` scope `story_close`
   subject ST-001, rồi `pulse work close-story ST-001` thành công. Gate
   readiness giờ cho Story lên `ready` (các family riêng của Ticket báo
   `not_applicable`). Lưu ý thứ tự: qa chạy trên HEAD, receipt ghi xong phải
   close TRƯỚC khi commit (close đòi receipt bind đúng HEAD; `check_cleanliness`
   bỏ qua `.pulse/` metadata nên untracked receipt không cản).
5. **6.5 Reviewer classification** — JSON cuối reviewer phải có `disposition`
   (`pass|rework`), `acceptance` cover đủ AC, `findings`; claim phải được
   chứng bằng receipt `work verify` do `agent:runner:reviewer` ghi trên đúng
   revision. `rework` thật làm Ticket sang `rework` (`run_rework`).
6. **6.6 Artifact ingest** — `artifacts[] {path, role, case_id?}`: hash
   SHA-256, copy vào `.pulse/evidence/artifacts/sha256/`, ghi vào run record
   + event. Path ngoài repo/không tồn tại → run `inconclusive`
   `artifact_ingest_failed`.
7. **6.7 Ratchet** — learning có `scope` (`harness|repository`, mặc định
   repository; `capture --scope`); harness không inject theo path, đi vào
   `## Harness learnings` trong `worker-prompt.md`; `knowledge promote`
   PHẢI đổi nội dung đích (`--document` hoặc `--agents-md` +
   `--insert-after "<heading>"`, có `--dry-run`; không đổi thì
   `promotion_target_unchanged`); re-promote chuyển đích và xoá relation cũ
   trong cùng transaction; `work handoff --learning-used
   LRN-001=helpful|not_needed|misleading` ghi `knowledge_usage[]` (injected
   do Pulse tự kiểm từ packet), `knowledge show` in usage; đổi tên
   `validate-learning` → `knowledge validate <id> --evidence`, store check →
   `knowledge check`; `knowledge applicable --work <id> --json` trả
   `required|recommended|suggested|excluded` + `why_applicable` dùng cùng
   logic packet; QA input gửi nguyên văn `intent`/`steps`/`expected`.
   **Đã chạy thật trên todolist:** LRN-001 → `harness`, promote vào
   `AGENTS.md` dưới `## Constraints`, relation cũ trỏ DOC-TODOLIST-BEHAVIOR
   đã bị xoá, `knowledge check` xanh.
8. **6.8 Docs** — ARCHITECTURE (runner + ratchet đúng hiện trạng), README
   (status, bảng lệnh), GLOSSARY (Learning scope, Isolation, Promotion).
   `examples/todolist/AGENTS.md` đã nhận block harness learnings.

## Cảnh báo working tree

Các file sau đang modified/untracked bởi một luồng làm việc song song
(quyết định 0013 "session handoff" và 0014 "Pulse ghi qa_checkpoint"):
`PRODUCT.md`, `docs/GLOSSARY.md`, `docs/decisions/0009…`, `docs/decisions/
0012…`, `docs/decisions/README.md`, `docs/decisions/0013…`, `docs/decisions/
0014…`. Đó là kế hoạch cho vòng SAU, chủ nhân của chúng sẽ commit. Đừng
commit hộ, đừng revert; nếu xung đột với việc của bạn, hỏi developer.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
chạy thật chỉ trong `examples/todolist/` (cwd ở đó hoặc `--repo-root
examples/todolist`). Commit kết bằng `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>`.

## Việc tiếp theo: dùng thật 5–10 Ticket trên todolist

1. Tạo 5–10 Ticket thật trên `examples/todolist/`: ít nhất một R0 (việc nhỏ),
   một R2 (cần `plan.md` + approach), một `decision_work`. Đảo vai: Codex
   làm worker, Claude làm reviewer, rồi đổi chiều.
2. Song song khi cần: run thứ hai phải `--isolation worktree` (run thường sẽ
   bị từ chối — đúng thiết kế).
3. Worker khai `--learning-used` cho mọi learning trong packet; reviewer
   trả JSON đúng contract; capture learning sau mỗi lần fail đáng nhớ.
4. Ghi mọi ma sát vào `examples/todolist/works/friction.md` (lệnh missing,
   prompt sai, gate phiền, receipt khó).
5. Chỉ sau 5–10 Ticket thật mới xét v0.2: MCP server, `pulse doctor`,
   self-hosting cho chính repo Pulse. Ưu tiên ma sát thật trước feature mới.
6. Khi có quyết định kiến trúc mới: ADR vào `docs/decisions/` (đừng đụng
   0013/0014 đang dở của luồng khác).

## Kết quả mong đợi của vòng này

- Todolist thêm 5–10 Ticket `done` bằng receipt, có ít nhất một chu kỳ
  rework thật và một learning mới được promote thật.
- `friction.md` đủ cụ thể để làm backlog v0.2.
- Không thay đổi core trừ khi ma sát bắt buộc; sửa core thì có test.
