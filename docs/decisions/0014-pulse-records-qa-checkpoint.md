# Decision 0014: Pulse ghi `qa_checkpoint`, runner chỉ in output

## Status

Accepted, 2026-09-06. Sửa PRODUCT.md §5.3 (contract role `qa`) và §5.5
(Executor). Không đổi contract output của runner đã ghi ở §5.3.

## Context

PRODUCT.md §5.3 nói `pulse run` "hash artifact khai báo, copy vào
`.pulse/evidence/artifacts/sha256/`" rồi "ghi receipt tương ứng role". §5.5
nói executor QA là runner role `qa` và "Pulse cung cấp input/output contract,
timeout, artifact ingest, receipt".

Code hiện tại làm ngược cho QA. `examples/todolist/scripts/qa-run.mjs` tự tạo
envelope, tự sinh ULID, tự đọc `evidence/manifest.json`, rồi gọi `pulse
evidence receipt record` bằng grant `evidence.record` của actor `runner:qa`,
**trước khi** in dòng JSON cuối. `pulse run qa` chỉ đọc dòng cuối và, với
phần artifact ingest đang làm trong `src/kernel/run.rs`, mới copy artifact
vào store.

Hệ quả nhìn thấy trong receipt thật `rcpt_28WQ…` của TK-001:

```text
 receipt.bindings.artifacts = []          script ghi receipt khi chưa có sha256 artifact
 evidence/artifacts/                       rỗng
 runtime/run/TK-001/artifacts/qa-observations.json   log QA chỉ ở đây, gitignored, sẽ bị dọn
```

Bằng chứng durable của QA vì thế chỉ là `observations[]` một dòng mỗi case.
Log chi tiết không được receipt trỏ tới. Ngoài ra script phải biết envelope,
ULID, manifest và grant, tức là mọi runner QA của mọi repo đích phải chép lại
cùng một đoạn boilerplate, và redaction của Decision 0012 cùng shape finding
phải áp ở hai chỗ.

Script cũng còn parse block ` ```pulse-qa ` JSON trong `qa.md`. Đó là nợ của
Decision 0010 (runner đọc `qa-input.json`, không đọc `qa.md`), nhắc ở đây vì
cùng file.

## Decision

1. **Runner role `qa` chỉ in JSON cuối.** Contract output giữ nguyên §5.3:
   `cases[] {id, status, observation}`, `artifacts[] {path, role, case_id}`,
   `findings[]`. Script không gọi `pulse evidence receipt record`, không cần
   grant `evidence.record`, không đọc `manifest.json`.
2. **`pulse run qa` ghi `qa_checkpoint`.** Sau khi đọc output và ingest
   artifact, Pulse tự dựng envelope: `actor` là `agent:runner:qa`; `subject`
   là Ticket với `ticket_checkpoint` hoặc Story với `story_close`;
   `bindings.source` là commit của run; `bindings.content` là `qa.md` với
   hash từ `qa-input.json`; `bindings.artifacts` là sha256 và role của mọi
   artifact vừa ingest; `payload.cases` map từ `status` sang outcome,
   `case_hash` lấy từ input; `payload.observations` từ `observation`;
   `result` là `passed` khi mọi case required `passed`, `failed` khi có
   `failed`, còn lại `inconclusive`.
3. **Baseline drift do Pulse quyết.** Trước khi ghi receipt Pulse so hash
   `qa.md` hiện tại với `baseline_content_hash` trong input; lệch thì receipt
   `inconclusive` với observation `qa_baseline_drift`, không tin script.
4. **Run không sạch thì không có receipt.** Exit khác 0, timeout, JSON hỏng,
   artifact không resolve: run record `inconclusive`, không ghi
   `qa_checkpoint`. Không có receipt thì không có gì được coi là đã QA.
5. **`findings[]` theo shape Decision 0012:** `case_id`, `summary`, `owner`,
   `check`; thiếu `check` thì `unverifiable`. Redaction áp một chỗ, trong
   Pulse.
6. **Grant `evidence.record` bỏ khỏi actor `runner:qa`** trong policy mà
   `pulse init` sinh. Script vẫn có thể `pulse evidence artifact put` nếu
   muốn tự hash trước, nhưng không bắt buộc.

## Luồng sau quyết định

```text
 pulse run qa --ticket TK-031
   ├─ (1) parse qa.md → qa-input.json (0010)
   ├─ (2) spawn script {input}
   │        script chạy case, ghi log vào artifact_dir, in JSON cuối
   ├─ (3) đọc JSON cuối; lỗi → inconclusive, dừng
   ├─ (4) ingest artifacts[] → evidence/artifacts/sha256/<h>
   ├─ (5) so hash qa.md với input → drift thì inconclusive
   ├─ (6) dựng rcpt_Q1 với bindings.artifacts đã hash → record_receipt_envelope
   └─ (7) run record + event run.completed trỏ rcpt_Q1
```

## Thay đổi

- `src/kernel/run.rs`: `classify_qa` dựng `QaCheckpointPayload` và envelope,
  gọi `record_receipt_envelope`; ghi `receipt_id` vào run record và event.
- `src/qa/receipt.rs`: hàm `build_checkpoint_envelope(input, output,
  artifacts, source)`; giữ `validate_checkpoint_receipt`.
- `src/policy/`: actor `runner:qa` mất `evidence.record` trong policy mặc
  định của `pulse init`; policy có sẵn trong `examples/todolist/` sửa tay.
- `examples/todolist/scripts/qa-run.mjs`: bỏ toàn bộ phần envelope, ULID,
  manifest, `receipt record`; đọc case từ `qa-input.json` thay vì parse
  `pulse-qa` (nợ 0010); chạy `check` khi case có, `inconclusive` khi không.
- Tests: `tests/runner` QA passed, failed, drift, artifact không resolve,
  output không có `artifacts`; receipt có `bindings.artifacts` khớp store.
- PRODUCT.md §5.3 và §5.5.

## Consequences

- Log QA vào evidence store và được receipt trỏ tới; bằng chứng QA không còn
  chỉ là một dòng observation.
- Runner QA của repo đích mỏng đi: chỉ chạy case và in JSON. Đổi runner (script
  sang agent, Playwright sang curl) không đụng receipt.
- Pulse có thêm một chỗ dựng envelope. Đây là đúng chỗ theo §5.3, và là chỗ
  duy nhất redaction và shape finding cần áp cho QA.
- Receipt cũ `rcpt_28WQ…` giữ nguyên, vẫn hợp lệ với `artifacts: []`.
