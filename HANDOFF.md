# Handoff: Pulse — Track A xong (A1–A5); Track B là việc tiếp theo

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental`
- HEAD: `004a3a4` (A5), trên 6 commit của phiên này:
  - `e5ad145` test: sửa suite `packet_injects` còn đứng trên lệnh
    `validate-learning` cũ (hỏng sẵn ở HEAD trước phiên, không phải do A)
  - `94c3071` A1: handoff mang claim máy đọc (`--check`/`--proof`,
    `HandoffReceipt.checks/acceptance_proofs`, summary ≤ 300 ký tự),
    `reviewer-input.json` bỏ `summary`, thêm `contract_revision`,
    `reviewers_required`, claims; prompt reviewer bỏ "Do not trust"
  - `e09a35f` A2: `Finding` có shape bắt buộc, `work verify --finding
    "AC|summary|owner|check|severity"`, rework toàn `unverifiable` bị từ
    chối (`findings_unverifiable`), packet liệt kê finding kèm actor
  - `0bbdfe4` A3: Pulse tự ghi `qa_checkpoint` (`build_checkpoint_envelope`),
    drift → receipt inconclusive + run `qa_baseline_drift`, `qa-run.mjs`
    mỏng đi, `runner:qa` mất `evidence.record`, dogfood receipt
    `rcpt_01M1TQK18NY5JGP8DVBEDM20KT` có artifact binding, đã commit
  - `72417a3` A4: `src/evidence/redaction.rs` (secret + absolute path) áp
    năm đường ghi; `src/policy/profile.rs` đọc `reviewers` từ
    `# Verification Profiles` trong PULSE.md; close gate đếm actor phân
    biệt; Passed verification không còn bump revision
  - `004a3a4` A5: `pulse note --work` (alias `--ticket`),
    `NoteRecorded.work_id`, prompt worker có bước `context_exhausted`
- Working tree: sạch.
- Ba gate xanh trên `004a3a4`: `cargo fmt --check`, `cargo clippy
  --all-targets --quiet -- -D warnings`, `cargo test --all-targets`
  (555 test, default threading). Tag `v0.1.0` vẫn ở `16a0ef3`, chưa push.

## Track A: đã xong toàn bộ A1–A5

Không đọc lại mục này để làm code; đọc ba Decision (0012, 0013, 0014) và
code. Những gì còn mở, cố ý:

1. **Profile `reviewers` chưa có binding theo Ticket** — floor của repo là
   profile nghiêm nhất được khai. Binding Ticket → profile là việc riêng
   khi dogfood đòi hỏi; không tự thêm.
2. **Gate `rework → ready` chưa cài** trong lifecycle (`rework_receipt` +
   `ready_gate`), nên sau verdict rework chưa build lại packet qua CLI.
   Mapping finding → `PacketReworkObservation` được pin bằng unit test
   (`src/kernel/packet.rs::rework_observation_tests`); test end-to-end cài
   khi gate được cài.
3. **Verified run:** hai receipt cùng một actor trên một handoff không còn
   ép `close_verification_ambiguous` (gate đếm actor, duplicate là noise).
4. **Note event** payload key đổi thành `work_id` cho event mới; event cũ
   giữ `ticket_id` (log append-only, không rewrite).
5. **Packet khi resume** là packet đã commit theo lease; note ghi giữa run
   thấy qua `events tail`, chưa được inject vào packet. Muốn "note hiện
   trong packet" đúng chữ Decision 0013 thì phải re-commit packet khi
   resume — đợi dogfood chứng minh chậm thật rồi mới làm.
6. Skill `pulse-handoff` + hook mẫu `context-guard.sh`: theo bàn giao cũ,
   làm cùng đợt bảy skill của Decision 0009, không làm ở Track A.

## Track B: dùng thật 5–10 Ticket trên todolist (việc tiếp theo)

1. Tạo 5–10 Ticket thật trên `examples/todolist/`: ít nhất một R0 (việc
   nhỏ), một R2 (cần `plan.md` + approach), một `decision_work`. Đảo vai:
   Codex làm worker, Claude làm reviewer, rồi đổi chiều. Receipt bây giờ
   sinh đúng contract mới (A1–A5).
2. Song song khi cần: run thứ hai phải `--isolation worktree` (run thường
   sẽ bị từ chối — đúng thiết kế).
3. Worker khai `--learning-used` cho mọi learning trong packet; worker
   ghi `--check`/`--proof` vào handoff; reviewer chạy lại check và trả
   JSON đúng contract, `--finding` có `check` khi rework; capture learning
   sau mỗi lần fail đáng nhớ.
4. Ghi mọi ma sát vào `examples/todolist/works/friction.md` (lệnh missing,
   prompt sai, gate phiền, receipt khó).
5. Chỉ sau 5–10 Ticket thật mới xét v0.2: MCP server, `pulse doctor`,
   self-hosting cho chính repo Pulse. Ưu tiên ma sát thật trước feature mới.
6. Khi có quyết định kiến trúc mới: ADR vào `docs/decisions/`, số tiếp
   theo là 0015.
7. Context sắp đầy: flush trước, cập nhật `HANDOFF.md` này (chỉ live
   thread, trỏ path), commit, rồi dừng.

## Quy tắc (không đổi)

Đọc `AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md` trước khi sửa. Mỗi mục một
commit, có test, ba gate xanh. Không chạy Pulse với `--repo-root .` ở gốc;
chạy thật chỉ trong `examples/todolist/` (cwd ở đó hoặc `--repo-root
examples/todolist`). Commit kết bằng `Co-Authored-By: Claude Fable 5.1
<noreply@anthropic.com>`.

## Kết quả mong đợi của vòng Track B

- Todolist thêm 5–10 Ticket `done` bằng receipt, có ít nhất một chu kỳ
  rework thật và một learning mới được promote thật.
- `friction.md` đủ cụ thể để làm backlog v0.2.
- Không thay đổi core trừ khi ma sát bắt buộc; sửa core thì có test.
