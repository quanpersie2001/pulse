# Handoff: Pulse — Bước 6, đóng mốc v0.1 và sửa theo dogfood

## Bối cảnh

Repo: `/Users/quannv.dev/Workspace/Personal/pulse`, nhánh `features/harness-experimental`,
HEAD `845ff01`. Working tree sạch. Ba gate xanh, 504 test.

Đã đạt: golden path PRODUCT.md §7 chạy thật trên `examples/todolist/` với hai
Ticket đóng bằng receipt (TK-001, TK-002), worker và reviewer là agent thật,
kill giữa chừng resume được, learning LRN-001 được capture, promote và inject
vào packet của Ticket sau (đã kiểm chứng lại độc lập ngày 2026-09-06).

Đọc trước khi sửa: `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md`,
`examples/todolist/works/ST-001/` (log lần chạy thật), và Git log
`257ef76..845ff01` (30 commit của Bước 4 và 5, commit message ghi rõ từng fix).

Quy tắc: mỗi mục một commit, có test, ba gate xanh
(`cargo fmt --check`, `cargo clippy --all-targets --quiet -- -D warnings`,
`cargo test --all-targets`, default threading). Không chạy Pulse với
`--repo-root .` ở gốc; chạy thật chỉ trong `examples/todolist/`. Commit kết bằng
`Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## Quyết định đã chốt

1. **Isolation: từ chối thay vì auto-worktree.** Finding 12 của Bước 5: lease
   zombie đẩy Ticket sau vào worktree, việc của worker rơi vào worktree và
   phải cứu tay. Rule mới: `pulse run` **từ chối** khi có Ticket khác đang
   `active`, in ra Ticket đó và gợi ý `--isolation worktree` để ép. Worktree
   chỉ được tạo khi cờ này có mặt. Cập nhật PRODUCT.md §3 nguyên tắc 10, §5.3
   "Isolation rule", §5.7 "Chạy song song".
2. **Learning có `scope`.** `harness` (bài học về cách dùng Pulse, ví dụ LRN-001
   "đừng sửa file sau handoff") và `repository` (bài học về codebase). Harness
   learning không inject theo path; nó đi vào bootstrap prompt của runner role
   tương ứng và có thể promote vào `AGENTS.md` của target. Repository learning
   inject theo path/tag/symbol như hiện tại.
3. **Promote phải đổi nội dung đích.** `knowledge promote --document <id>` chỉ
   được ghi relation `promoted_to` khi content hash của doc **khác** hash lúc
   bắt đầu; nếu không đổi thì lỗi `promotion_target_unchanged`. Cách làm: lệnh
   in ra đoạn text đề xuất và vị trí (heading) rồi chờ developer/agent sửa
   file, hoặc nhận `--insert-after <heading>` để tự chèn đoạn text từ
   `guidance` của learning. Chọn cách thứ hai làm mặc định, có `--dry-run`.
4. **Một key cho rationale.** `## Documentation impact` và `## QA impact` cùng
   nhận `Rationale:` (và chấp nhận `Reason:` như alias, không lỗi). Template
   dùng `Rationale`.

## Việc cần làm, theo thứ tự

### 6.1 Đóng mốc v0.1

- Xoá `examples/todolist/Oops.rej.orig` (rác từ lúc salvage TK-002).
- README Status: "Golden path đã chạy thật trên `examples/todolist/`, hai
  Ticket đóng bằng receipt; xem `examples/todolist/works/ST-001/`". Bảng What
  works today thêm `pulse run`, `events tail`, `note`, `knowledge capture|
  promote|applicable`.
- PRODUCT.md §7 thêm dòng "Đạt ngày 2026-09-05, HEAD 845ff01". §8 cột hiện
  trạng cho 5.3, 5.6, 5.7. §13 ghi ba quyết định mới ở trên.
- `git tag v0.1.0` tại commit này. Không push.

### 6.2 Isolation rule mới (quyết định 1)

Sửa runner: khi có lease `active` của Ticket khác trong cùng repo-root, từ chối
với `run_isolation_required` kèm Ticket id và lệnh gợi ý. `--isolation
worktree` giữ nguyên hành vi tạo worktree và dọn khi terminal. Test: hai
Ticket, `run` thứ hai bị từ chối; với cờ thì tạo worktree; close Ticket đầu
thì `run` thứ hai không cần cờ. Xoá `auto_isolation` config nếu có.

### 6.3 Rationale/Reason (quyết định 4)

`src/graph/model/brief.rs`: docs và QA impact cùng đọc `rationale`, alias
`reason`. Template `work create` dùng `Rationale`. Sửa `ticket.md` của TK-001,
TK-002 trong `examples/todolist/` chỉ nếu parser mới không đọc được chúng
(không nên phải sửa vì có alias). Test parser cả hai key.

### 6.4 Story close chạy thật

`scripts/qa-run.mjs` trong todolist nhận `qa_scope` từ input; `pulse run qa
--scope story_close --story ST-001` (hoặc cú pháp tương đương đã có) tạo input
với toàn bộ case required của baseline, ghi `qa_checkpoint` scope `story_close`.
Rồi `pulse work close-story ST-001` trong `examples/todolist/`, commit kết quả
(node, receipt, event). Đây là tiêu chí phụ của §7.

### 6.5 Reviewer output được phân loại

`classify_outcome` cho role `reviewer`: JSON cuối có `disposition`
(`pass|rework`), `acceptance` map, `findings`. `rework` phải tạo verification
receipt disposition rework thật và Ticket sang `rework`, không chỉ "completed".
Thiếu `disposition` hoặc acceptance không cover đủ AC thì `inconclusive`. Test
với fake reviewer script trả ba trường hợp.

### 6.6 Artifact ingest trong `pulse run`

Output JSON `artifacts[] {path, role, case_id?}`: hash SHA-256, copy vào
`.pulse/evidence/artifacts/sha256/`, ghi vào receipt tương ứng. Path ngoài
repo-root hoặc không tồn tại thì receipt `inconclusive` với lý do. Test với qa
runner ghi log file.

### 6.7 Ratchet hoàn thiện (quyết định 2 và 3)

- Thêm `scope: harness | repository` vào learning schema; `knowledge capture`
  hỏi hoặc nhận `--scope`, mặc định `repository`. Đổi LRN-001 trong
  `examples/todolist/` sang `harness` bằng `knowledge edit`.
- Injection: repository learning vào packet như hiện tại; harness learning vào
  bootstrap prompt của runner (mục `## Harness learnings` trong
  `worker-prompt.md` tương đương) và không vào packet theo path.
- `knowledge promote --document <id> --insert-after "<heading>" [--dry-run]`
  chèn đoạn text từ `guidance` và `summary`, sau đó mới ghi relation với hash
  mới. Hash không đổi thì `promotion_target_unchanged`. Thêm `--agents-md` làm
  đích cho harness learning. Chạy lại promote cho LRN-001 vào
  `examples/todolist/AGENTS.md`, xoá relation cũ trỏ vào DOC-TODOLIST-BEHAVIOR.
- Handoff ghi `knowledge_usage[] {learning_id, injected, applied, outcome}`;
  `pulse work handoff --learning-used LRN-001=helpful|not_needed|misleading`.
  `knowledge show` in usage count. Packet của TK-001/TK-002 không có usage là
  bình thường (trước khi có field).
- Đổi tên `validate-learning` thành `knowledge validate <id> --evidence` và
  lệnh store check thành `knowledge check`. Thêm `knowledge applicable --work
  <id> --json` trả `required|recommended|suggested|excluded` với
  `why_applicable`, dùng cùng logic packet đang dùng.
- QA input gửi nguyên văn `intent`, `steps`, `expected` của case, không chỉ id.

### 6.8 Docs sau cùng

`ARCHITECTURE.md` (runner isolation, learning scope, promote), `README.md`,
`AGENTS.md` nếu rule đổi, `docs/GLOSSARY.md` (Learning scope, Isolation).
`examples/todolist/AGENTS.md` nhận mục harness learnings.

## Kết quả mong đợi

- `examples/todolist/`: ST-001 `done` với receipt story_close; LRN-001 scope
  `harness`, promote thật vào `AGENTS.md` của todolist với hash mới; không file
  rác.
- `pulse run worker --ticket X` khi Y đang active: bị từ chối, thông báo rõ.
- Reviewer trả `rework` làm Ticket sang `rework`.
- `knowledge applicable --work TK-00x` in đúng bucket.
- Tag `v0.1.0`.

## Sau Bước 6

Dùng thật thêm 5 đến 10 Ticket trên todolist với vai đảo (Codex worker, Claude
reviewer), Ticket R0 và R2, một `decision_work`. Ghi ma sát vào
`examples/todolist/works/friction.md`. Chỉ sau đó mới xét v0.2: MCP server,
`pulse doctor`, self-hosting cho chính repo Pulse. Cuối session thay file này
bằng handoff cho đợt dùng thật đó.
