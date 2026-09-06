# Handoff: Pulse — Bước 6 xong, v0.1 chốt; hai track tiếp theo

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `df24af5` (docs Decision 0012–0014), trên `dfd2041` (Bước 6 xong,
  18 commit trên `845ff01`, mỗi mục một commit)
- Working tree: sạch.
- Ba gate xanh trên `dfd2041` (`df24af5` chỉ sửa docs): `cargo fmt --check`,
  `cargo clippy --all-targets --quiet -- -D warnings`, `cargo test
  --all-targets` (530 test, default threading). Tag `v0.1.0` tại `16a0ef3`,
  chưa push.

## Hai track, chọn một khi mở phiên

| Track | Mục | Khi nào |
|---|---|---|
| A. Code theo Decision 0012, 0014, 0013 | "Track A" dưới | trước khi tạo Ticket dogfood mới, vì A đổi contract handoff và receipt QA; dogfood sau A thì receipt sinh ra đúng contract mới |
| B. Dùng thật 5–10 Ticket trên todolist | "Track B" dưới | sau A, hoặc song song nếu chấp nhận receipt cũ |

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

## Track A: code theo Decision 0012, 0014, 0013

Đọc ba Decision trước, không đọc lại thảo luận. Mỗi mục một commit, có test,
ba gate xanh. Thứ tự cố ý: A1 mở khoá A2 và A3 (shape finding dùng chung).

1. **A1 Handoff claim và reviewer input** (0012 mục "Contract handoff",
   "Contract reviewer", "Thay đổi"):
   - `src/execution.rs`: `HandoffReceipt` thêm `checks: Vec<VerificationCheck>`
     và `acceptance_proofs: Vec<AcceptanceProof>` (serde default để receipt
     cũ trong `examples/todolist/` vẫn load); `summary` ≤ 300 ký tự.
   - `src/cli/work.rs`: `work handoff` nhận `--check` và `--proof` bằng
     `parse_check`/`parse_proof` sẵn có của `work verify`.
   - `src/kernel/completion.rs::submit_execution_handoff`: validate và ghi
     hai trường; `validate_checks` cho worker không đòi exit 0 (đó là claim,
     reviewer mới chạy lại).
   - `src/kernel/run.rs`: `reviewer-input.json` bỏ `summary`, thêm
     `contract_revision`, `reviewers_required` (tạm cố định 1 cho tới A4),
     `checks`, `acceptance_proofs` của handoff; bootstrap prompt worker ghi
     cú pháp `--check`/`--proof`; prompt reviewer bỏ "Do not trust the worker
     summary".
   - Tests: `tests/runner/reviewer.rs` (input không có `summary`, có claim),
     `tests/runner/cli_run.rs` (handoff với `--proof` thiếu AC vẫn ghi).
2. **A2 Shape finding** (0012 mục 3, PRODUCT §5.3):
   - `src/execution.rs`: `Finding {summary, owner, check: Option<String>,
     severity, acceptance_id: Option, case_id: Option, unverifiable: bool}`;
     `VerificationReceipt` thêm `findings: Vec<Finding>`.
   - `work verify --finding "AC-2|<summary>|<owner>|<check>"` hoặc
     `--findings-file`; chọn một, ghi vào help.
   - `classify_reviewer`: validate mảng `findings`, gắn `unverifiable` khi
     thiếu `check`; `rework` mà mọi finding đều `unverifiable` →
     `inconclusive` với lý do `findings_unverifiable`.
   - Packet: finding của verification `rework` hiện trong `rework
     observation` kèm actor.
3. **A3 Pulse ghi `qa_checkpoint`** (0014 toàn bộ):
   - `src/qa/receipt.rs`: `build_checkpoint_envelope(input, output,
     artifacts, source)`.
   - `src/kernel/run.rs::classify_qa`: sau ingest artifact, so hash `qa.md`
     với input (lệch → `inconclusive` `qa_baseline_drift`), dựng envelope,
     `record_receipt_envelope`, ghi `receipt_id` vào run record và event.
   - `examples/todolist/scripts/qa-run.mjs`: bỏ envelope/ULID/manifest/
     `receipt record`; đọc case từ `qa-input.json` (nợ 0010, bỏ parse
     `pulse-qa`); chạy `check` khi có, `inconclusive` khi không.
   - `examples/todolist/.pulse/policy/authority.json`: `runner:qa` bỏ
     `evidence.record`; `src/kernel/init.rs` policy mặc định cũng bỏ.
   - Dogfood: `pulse run qa --ticket <TK done gần nhất>` trong todolist một
     lần để có receipt với `bindings.artifacts` khác rỗng; commit receipt.
   - Tests `tests/runner`: passed, failed, drift, artifact không resolve,
     output thiếu `artifacts`.
4. **A4 Redaction và `reviewers`** (0012 mục 4, 5):
   - `src/evidence/redaction.rs` mới; gọi từ `receipt record`, `work
     handoff`, `work verify`, `note`, `knowledge create|capture`; test từng
     mẫu và path dưới/ngoài repo root.
   - Profile trong `PULSE.md` nhận `reviewers`; close gate đếm receipt
     `verification` theo actor phân biệt; `reviewers_required` trong
     reviewer input đọc từ profile.
5. **A5 Bàn giao phiên phần rẻ** (0013 mục 4, 5):
   - `pulse note --work <id>`, `--ticket` alias; `NoteRecorded.work_id`
     với alias serde `ticket_id`.
   - Bootstrap prompt worker thêm bước `context_exhausted`;
     `classify_worker` nhận `reason` đó như `blocked` thường.
   - Skill `pulse-handoff` và hook mẫu viết cùng đợt với bảy skill của 0009,
     không làm ở đây.

Không làm trong track A: `--kind handoff`, `pulse work resume`, gộp hai họ
receipt, `pulse ratchet bundle`, `doctor`. Tất cả là Later theo PRODUCT §11.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
chạy thật chỉ trong `examples/todolist/` (cwd ở đó hoặc `--repo-root
examples/todolist`). Commit kết bằng `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>`.

## Track B: dùng thật 5–10 Ticket trên todolist

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
6. Khi có quyết định kiến trúc mới: ADR vào `docs/decisions/`, số tiếp theo
   là 0015.
7. Context sắp đầy: làm theo Decision 0013 cho repo này, tức là cập nhật
   file `HANDOFF.md` này (flush trước, chỉ live thread, trỏ path), commit,
   rồi dừng.

## Kết quả mong đợi của vòng này

- Todolist thêm 5–10 Ticket `done` bằng receipt, có ít nhất một chu kỳ
  rework thật và một learning mới được promote thật.
- `friction.md` đủ cụ thể để làm backlog v0.2.
- Không thay đổi core trừ khi ma sát bắt buộc; sửa core thì có test.
