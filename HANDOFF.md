# Handoff: Pulse — vào Decision 0010 (QA baseline markdown)

## Trạng thái bàn giao

- Repo: `/Users/quannv.dev/Workspace/Personal/pulse`
- Nhánh: `features/harness-experimental` (chưa push; tag `v0.1.0` ở `16a0ef3`)
- HEAD: `179cc4f` feat(run): worktree dispatch and worker-owned docs
  receipt (0015, 0016)
- Working tree: **sạch**.
- Ba gate xanh tại HEAD: `cargo fmt --check`, `cargo clippy --all-targets
  --quiet -- -D warnings`, `cargo test --all-targets` — **574 test**,
  default threading.

## Vòng vừa rồi làm gì

Audit toàn bộ 15 ADR đối chiếu code, rồi đóng hai cái nặng nhất.

**Decision 0015 — worktree dispatch.** Worktree do Pulse tạo là *workspace*
của repo chính, không phải repo Pulse thứ hai. Marker `.pulse-owned` mang
state root tuyệt đối, Git đối chứng; `state_repo_root` map ở biên CLI nên mọi
lệnh chạy trong worktree hội tụ về một lock của repo chính. Run workspace
mirror vào worktree, prompt nhúng path tuyệt đối, reviewer/qa kế thừa
workspace của worker. Cover: `tests/runner/worktree_dispatch.rs` (8 test) với
fake agent chỉ biết cwd của mình và không truyền `--repo-root`.

**Decision 0016 — worker sở hữu docs receipt.** Khoảng mơ hồ "ai ghi docs
receipt" hoá ra ba tầng: prompt worker im lặng; `handoff --evidence-receipt`
là ngõ cụt (close gate chỉ đọc proof của reviewer); và
`proof_receipts.documentation_validation` lọc theo ticket id nên **luôn
rỗng** (docs receipt subject-bound tới docs registry). Sửa cả ba: handoff từ
chối với `handoff_documentation_receipt_missing`, reviewer input lọc theo
source commit. Cover: `tests/graph/documentation_handoff.rs` (4 test) +
`tests/runner/reviewer.rs::reviewer_input_lists_docs_receipts_bound_to_the_reviewed_commit`.

Ba mục `friction.md` đã đóng kèm mô tả nguyên nhân.

## Việc của session tới: Decision 0010

**Đọc trước:** `docs/decisions/0010-qa-baseline-markdown-contract.md` (toàn
bộ — nó có contract đầy đủ và một ví dụ `qa.md` hoàn chỉnh), rồi
`src/qa/baseline.rs`.

### Vì sao nó gấp

ADR 0010 đã `Accepted` từ 2026-09-06 và `PRODUCT.md` §5.5 **đã sửa theo nó**,
nhưng code thì chưa động. `src/qa/baseline.rs:210` vẫn đòi đúng một block
` ```pulse-qa ` chứa JSON. Nghĩa là agent đọc `PRODUCT.md` rồi viết `qa.md`
theo heading sẽ bị parser từ chối. Đây là ADR duy nhất mà code **mâu thuẫn
trực tiếp** với quyết định đã chốt, không phải chỉ thiếu.

### Rủi ro lớn nhất: đổi shape payload receipt

0010 bỏ `baseline_revision`/`case_revision` để thay bằng `case_hash`. Hiện có
**7 `qa_checkpoint` receipt** trong `examples/todolist` mang shape cũ:

```
rcpt_01M1TQK18NY  ST-001    rcpt_01M1W1J7J79  ST-002
rcpt_01M1W00F13Y  TK-005    rcpt_01M1W1J7S1C  ST-002
rcpt_01M1W1GC1EC  ST-002    rcpt_28WQZR1M10T  TK-001
rcpt_RBQWFS1M10H  ST-001
```

`verify_receipt` → `validate_envelope` (`src/evidence/receipt/envelope.rs:53`)
→ `validate_checkpoint_receipt` chạy trên **mọi** lần load. Repo này đã bị
đúng lớp lỗi này cắn một lần: commit `074fabf` sửa vụ 8 receipt cổ chặn **mọi**
packet build.

**Hướng xử lý đã chốt (Decision 0003):** pre-release có đúng một baseline —
regenerate, không viết migration code, không thêm decoder cho bản cũ. Cụ thể:
đổi shape, chạy lại `pulse run qa` trên `examples/todolist` sinh receipt mới,
xoá 7 cái cũ trong **cùng một commit**. Kèm một test hồi quy chứng minh
receipt shape cũ bị từ chối sạch sẽ chứ không làm hỏng packet build của Ticket
không liên quan — đó mới là cái đã đau lần trước.

### Khối lượng

| Việc | File |
|---|---|
| Parser heading thay fenced JSON (`## Scope`, `## Posture`, `## Risks`, `## Exit criteria`, `### QA-NNN`, các dòng `Key:`) | `src/qa/baseline.rs` (321 dòng, viết lại phần lớn) |
| Mã lỗi `qa_baseline_unknown_field` nêu tên dòng sai | `src/qa/baseline.rs` |
| Block `pulse-check` (YAML tối giản → struct, argv không qua shell) | `src/qa/baseline.rs` mới |
| `case_hash` (sha256 section từ `### QA-` tới heading cùng cấp kế) thay `revision` | `src/qa/receipt.rs` |
| `qa-input.json` theo shape §"Runner input" của ADR | `src/kernel/run.rs` |
| Ready gate resolve `Cases:` của `## QA impact` với `qa.md` của Story owner | `src/graph/read/readiness.rs` |
| Đọc case từ `qa-input.json`, chạy `check`, `inconclusive` khi không có | `examples/todolist/scripts/qa-run.mjs` |
| Chuyển sang heading | `examples/todolist/works/ST-00{1,2}/qa.md` |
| Regenerate 7 receipt | `examples/todolist/.pulse/evidence/` |

Nặng nhất là parser và ready gate; hai cái đó nên đi trước, `pulse-check` sau
cùng vì nó tuỳ chọn.

### Ràng buộc đã học, đừng vi phạm lại

- **Lock:** `verify_receipt` lấy repository write lock (nó load docs
  registry). Không gọi nó khi đang giữ fence — đó là bug `e27b5e4`.
  `load_receipt` chỉ đọc file, an toàn trong fence.
- **Canonical form:** thêm/bỏ trường trong payload receipt phải cân nhắc
  fingerprint. `src/execution.rs:426` có ghi chú: collection rỗng bị bỏ khỏi
  canonical form để receipt cũ vẫn validate. Cùng lớp bẫy.
- **Dirty fence:** đừng sửa file tracked ngoài phạm vi khi có Ticket đang
  chạy — LRN-001, và nó áp cho cả ghế operator.

## Sau 0010

Theo thứ tự ma sát thật, không theo số ADR:

1. **Ba fix nhỏ từ `friction.md`**, mỗi cái có chi phí đã đo: chụp stdout
   tail của reviewer vào run record (mất 2 vòng chẩn đoán); `close_source_stale`
   nêu path vi phạm; `pulse policy normalize` cho `authority.json`.
2. **Decision 0011** — event log `<date>.jsonl` + `events compact`. Accepted
   nhưng chưa động một dòng (`src/event.rs:214` vẫn ghi `<date>/<id>.json`,
   `pulse events` chỉ có `tail`). **Không có mục friction nào** nhắc tới nó,
   nên đây là nợ ADR thuần — làm khi tiện, đừng chen lên trước.
3. **Decision 0009** — bề mặt skill, khối lớn nhất còn lại. Bước đầu tiên là
   gỡ guard `tests/graph/architecture_guards.rs:106` đang cấm chính `skills/`.
   **Nên xem lại phạm vi trước khi làm:** 0009 viết ngày 2026-09-06, trước khi
   có dữ liệu Track B. Toàn bộ 15+ mục `friction.md` là ergonomics của CLI,
   không mục nào là "agent không biết làm gì tiếp" — dữ liệu đang nói bài toán
   nằm ở bề mặt lệnh, không ở thiếu lớp hướng dẫn. Cần human quyết có thu hẹp
   0009 không.

## Nợ tài liệu nhỏ đã phát hiện, chưa sửa

- `PRODUCT.md` §8 lệch code ở dòng 5.3: vẫn liệt kê `--check/--proof`,
  `reviewers_required`, shape finding là "còn lại" trong khi đã xong;
  "Artifact ingest đang làm" trong khi `ARCHITECTURE.md` khai đã chạy thật.
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
