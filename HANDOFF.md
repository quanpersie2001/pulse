# Handoff: Pulse — 0010 đóng, mục friction đắt nhất đóng

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push; tag `v0.1.0` ở `16a0ef3`)
- HEAD: `f770587` fix(graph): fold wrapped bullets in the ticket.md contract
- Working tree: **sạch** (trừ `HANDOFF.md` này).
- Ba gate xanh tại HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` — **588 test**,
  default threading.

Bốn commit của vòng này:

```
ce97810 feat(qa): parse qa.md headings and hash cases (0010)
578feaf dogfood(0010): heading baselines and a case-agnostic pulse-check runner
76ac229 docs: record how 0010 landed and correct the status rows
f770587 fix(graph): fold wrapped bullets in the ticket.md contract
```

## Vòng vừa rồi làm gì

### Decision 0010 — qa.md là markdown heading

`src/qa/baseline.rs` viết lại quanh heading contract: tách section biết fence
(nên `#` trong block code không thành heading), dòng `Key:` lạ bị từ chối kèm
nguyên văn dòng, `## Risks` thành `{id, summary}`, `QaCase` thêm `title`,
`preconditions`, `evidence`, `case_hash`, `check`, `surface`/`posture` thành
enum.

Block `pulse-check`: YAML tối giản → `QaCheck { run: argv, cwd, env, stdin,
timeout_seconds, assert }`. `run` tách argv có quote, từ chối toán tử shell
(`qa_check_shell_operator`). Bảy assertion đủ. Block chỉ hợp lệ trên surface
`cli`/`api`.

`case_hash` (sha256 section đã chuẩn hoá) thay `revision` ở receipt payload,
snapshot readiness và cả hai close gate. `baseline_content_hash` **cố ý** giữ
là hash byte nguyên văn: receipt content-bind `works/<STORY>/qa.md` và
`content_source_binding_codes` băm lại file trên đĩa, hash chuẩn hoá sẽ không
bao giờ current. Bốn khác biệt so với bản ADR đã ghi vào §Status của
`docs/decisions/0010-*.md`.

Dogfood: hai baseline chuyển heading, mỗi case có `pulse-check` gọi
`scripts/qa-case.mjs <CASE-ID>` (script mới, giữ assertion in-process);
`scripts/qa-run.mjs` thành executor `pulse-check` hoàn toàn không biết case
nào. Bảy receipt cũ regenerate tại chỗ theo Decision 0003 (payload + content
binding + `receipt_hash` trong event `evidence.receipt.recorded`), cả bảy lại
`integrity: valid`.

### Mục friction đắt nhất: wrapped bullet trong ticket.md

`parse_questions` từ chối mọi dòng không phải bullet dưới `## Open questions`
— 6/6 Ticket fail sync đầu tiên, và error chỉ sai chỗ (nói về disposition
trong khi vấn đề là dòng xuống hàng). Giờ dòng không phải bullet nối vào câu
hỏi phía trên; text không có bullet nào phía trên vẫn là lỗi thật (đó là chốt
chặn để một câu hỏi `blocking` viết dạng đoạn văn không bị nuốt im lặng); mọi
error đều trích nguyên văn dòng.

Mở rộng có chủ ý: `list_section`, `parse_acceptance`, `parse_key_values` cùng
mắc lỗi đó nhưng **im lặng** — một `AC-1:` hay `Rationale:` xuống dòng mất
nửa sau mà không báo gì. Cả ba giờ fold continuation line.

## Đã học, ghi lại

- `list_receipts` deserialize **mọi** file receipt và fail nguyên hàm ở file
  hỏng đầu tiên. Một receipt shape cũ làm `pulse evidence receipt list` chết
  cho cả repo — đó là cách bảy receipt kia lộ ra. `work packet` không đọc
  receipt nên không việc gì; nhưng hai chỗ gọi `list_receipts` trong
  `kernel/run.rs` nuốt lỗi bằng `unwrap_or_default()`, tức một receipt hỏng
  làm proof list của reviewer **rỗng im lặng**. Đó đúng hình dạng bug mà
  Decision 0016 vừa đóng (list rỗng trông như thiếu evidence, tốn ba vòng
  rework thật). **Chưa sửa, chưa có mục friction** — xem §Việc tiếp theo.
- Continuation `\` trong string literal Rust nuốt cả indent của dòng sau. Test
  data cho parser thụt lề phải dùng raw string.
- `cargo test --all-targets` sau `cargo clippy --all-targets` phải build lại
  từ đầu (khác profile): tính ~8-10 phút, đừng đặt timeout 120s.

## Việc tiếp theo

1. **`list_receipts` nuốt lỗi ở reviewer input** (mục "Đã học" ở trên). Không
   phải giả định — đã vấp thật vòng này. Hai nửa: `list_receipts` nên phân
   biệt "không có receipt" với "có receipt không đọc được", và hai callsite
   trong `kernel/run.rs` không được biến lỗi đó thành list rỗng. Cần một mục
   friction ghi lại trước khi sửa, và một test hồi quy dựng đúng cảnh
   reviewer-input-với-receipt-hỏng.
2. **Hai fix nhỏ còn lại từ `friction.md`**: `close_source_stale` nêu path vi
   phạm (tốn một vòng revert, và nó cắn cả ghế operator — mục 2026-09-07 cuối
   file); chụp stdout tail của reviewer vào run record (tốn hai vòng chẩn
   đoán). `pulse policy normalize` thì **xem lại trước khi làm**: reason code
   `readiness_policy_not_canonical` đã tồn tại sẵn, có thể chỉ cần cho nó
   surface thay vì thêm lệnh mới.
3. **Decision 0011** — event log `<date>.jsonl` + `events compact`. Accepted
   nhưng chưa động một dòng (`src/event.rs:214` vẫn ghi `<date>/<id>.json`,
   `pulse events` chỉ có `tail`). Không mục friction nào nhắc tới nó: nợ ADR
   thuần, làm khi tiện, đừng chen lên trước.
4. **Decision 0009** — bề mặt skill, khối lớn nhất còn lại. Bước đầu là gỡ
   guard `tests/graph/architecture_guards.rs:106` đang cấm chính `skills/`.
   **Cần human quyết phạm vi trước:** 0009 viết 2026-09-06, trước dữ liệu
   Track B. Toàn bộ `friction.md` là ergonomics của bề mặt lệnh, không mục
   nào là "agent không biết làm gì tiếp" — dữ liệu đang nói bài toán nằm ở
   CLI, không ở thiếu lớp hướng dẫn.

## Nợ tài liệu nhỏ đã phát hiện, chưa sửa

- `PRODUCT.md` §8 dòng 5.3 vẫn liệt kê `--check/--proof`, `reviewers_required`,
  shape finding là "còn lại" trong khi đã xong; "Artifact ingest đang làm"
  trong khi `ARCHITECTURE.md` khai đã chạy thật.
- `AGENTS.md` liệt kê 8 test crate, thực tế có 10 — thiếu `tests/runner.rs`
  và `tests/communication.rs`.
- Decision 0004 đúng về bản chất (packet lease-bound, không có `run context`)
  nhưng viết bằng ngôn ngữ daemon đã xoá; nên đánh "Superseded in part by
  0008".

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
chạy thật chỉ trong `examples/todolist/`. Sửa core chỉ khi ma sát bắt buộc;
mỗi fix có test hồi quy. Không đổi `PRODUCT.md` khi chưa có ADR.
