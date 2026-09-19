# Decision 0023: `pulse serve` multi-project thay board tĩnh

## Status

Accepted, 2026-09-17 (owner: quan).

**Sửa [Decision 0022](0022-thin-harness.md) §13 và §14:** board tĩnh
(`.pulse/cache/board.html`) không build nữa; thay bằng `pulse serve` —
một HTTP server đọc-only cục bộ, đọc **mọi project dùng Pulse** trong một
workspace, UI dạng Jira-lite: chọn project, kanban issue, drawer evidence
đầy đủ, event trace. `pulse serve` được xoá khỏi danh sách cơ chế đóng băng
ở §14.

## Context

Board tĩnh trong §13 được thiết kế trước ST-1 và chưa từng build (P3.1
chưa tick). Điều kiện để làm server — "mở board > 20 lần/ngày" — giả định
board tĩnh đã tồn tại và gây phiền; thực tế operator dùng `work show` +
`events tail` + mở evidence bằng tay xuyên suốt ST-1/ST-2 (report ST-2:
vẫn phải mở evidence dir sau lane run). Owner chốt 2026-09-17: nhu cầu
thật là **nhìn thấy mọi project một chỗ** — board tĩnh một repo không
đáp ứng được vì một file HTML không biết các repo khác.

Chấp nhận trả giá: thêm dependency HTTP và một domain mới, đổi lại dừng
trả interest cho flow đọc thủ công.

## Decision

1. **`pulse serve` thay thế `pulse board`.** Một UI duy nhất:
   `src/serve/board.html` — self-contained (CSS/JS inline, không CDN),
   fetch dữ liệu từ API dưới. `pulse board` tĩnh không được build; nếu sau
   này cần export-offline, đó là quyết định mới.
   — amended 2026-09-20: UI dời từ `assets/board/` về `src/serve/`
   (ngay cạnh `http.rs` nhúng nó) để giữ ranh giới A8.4 — `assets/`
   chỉ là media của chính repo này. Icon mark
   (`assets/logo-icon.svg`, bị xoá ở `db33d26` vì unreferenced) được
   khôi phục và serve tại `/favicon.svg` làm favicon + brand mark.
2. **HTTP stack: `tiny_http`.** axum/hyper/tokio bị loại — dep tree quá
   nặng cho thin harness. `tiny_http` threaded, đủ cho read-only local.
3. **Read-only tuyệt đối.** Không endpoint ghi, không lock, không lease.
   Server là một reader: mỗi request đọc file từ disk (atomic reads của
   store không tái sử dụng vì serve cần **lenient** — dòng JSONL dở đại
   khi append đang chạy bị skip và đếm, không bao giờ 500). Không mutation
   path = không cần actor/auth; thêm ghi là một decision mới.
4. **Bind 127.0.0.1** mặc định; `--port` (mặc định do bước implement chốt,
   ghi vào `--help`), `--open` gọi `open`/`xdg-open`.
5. **Discovery — amended same day (2026-09-17, pre-dogfood), registry
   primary:** `pulse init` registers the repo in a user-level registry
   (`~/.pulse/projects.json`, `PULSE_REGISTRY` override for tests),
   written best-effort by the CLI layer (kernel::init stays pure; tests
   that call it never touch the real registry) with `--no-register` to
   opt out. `pulse serve` lists registered repos — entries whose
   `.pulse/issues.jsonl` vanished are filtered silently, no state kept
   between requests — and `--workspace <dir>` additionally scans a tree
   (depth ≤ 4, same skip list) merged with the registry, deduping by id.
   No `register`/`unregister` commands: re-running `pulse init` on a
   clone registers it; dead entries self-hide. The original walk-only
   design lost because the operator must know the scan root up front —
   registration belongs at the enrollment moment, which `pulse init`
   already is.
6. **API:**
   - `GET /api/projects` — danh sách project (id, tên, path hiển thị,
     counts theo kind/status, event mới nhất).
   - `GET /api/p/<pid>/board` — toàn bộ records + learnings; đủ cho kanban.
   - `GET /api/p/<pid>/issue/<id>` — record đầy đủ + receipts + evidence
     manifest (handoff, checkpoints, lane outputs, shots, logs) + event
     trace (mọi event có subject id khớp).
   - `GET /favicon.svg` — icon mark của repo (`assets/logo-icon.svg`),
     same-origin để board không phụ thuộc CDN hay data-URI trùng lặp.
   - `GET /p/<pid>/evidence/<rel path>` — file evidence tĩnh (ảnh, log);
     canonicalize + prefix check chống path traversal.
7. **UI:** project picker; kanban cột theo status với bộ chọn loại
   Epic/Story/Ticket; Ticket có thể nhóm theo Story; filter Epic; drawer record
   ba tab — Detail (record đầy đủ), Evidence (manifest + ảnh + logs), Events
   (timeline trace).

## Consequences

- Dependency mới: `tiny_http` (kèm `ascii`, `chunked_transfer`, `httpdate`).
- **Đợt cắt bù:** Decision này ban đầu nêu xoá
  `storage/transaction.rs` + `evidence/artifact.rs` làm cột mốc cắt bù —
  **sai thời điểm**: đợt cắt đó đã landing từ trước ở `d363734` (A7,
  0022-open), chỉ là ghi chú trong 0022-open chưa kịp gắn nhãn Done. Cột
  mốc cắt bù thực tế cho serve là **gộp `learn list` vào `learn show`**
  (gợi ý sẵn của plan §2; CLI leaves 27 -> 26), thực hiện cùng commit đợt
  serve. Net `src/` kỳ vọng tăng ~một nghìn dòng sau serve; đích < 10.000
  phụ thuộc các lần gộp CLI còn lại (`work dep rm`, gộp `events`) — ghi
  rõ ở 0022-metrics khi Phase 3 khép.
- §14 bỏ `pulse serve` khỏi điều kiện dừng; tám cơ chế còn lại giữ nguyên.
- Metrics (`0022-metrics.md`) ghi lại sau khi serve landed.
