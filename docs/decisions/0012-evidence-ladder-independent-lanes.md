# Decision 0012: Thang bằng chứng, reviewer là bằng chứng, lane độc lập và lead hoà giải

## Status

Accepted, 2026-09-06. Bổ sung PRODUCT.md §5.3 (contract reviewer), §5.5
(validity receipt, verification profile, close gate), §5.6 (thang bằng chứng,
vòng ratchet), §5.8 (skill `pulse-spec`, `pulse-ratchet`), §11 (Later). Thu
hẹp mục `pulse-ratchet` của Decision 0009: bước 1–4 ở đó được thay bằng ba
lane bằng chứng và luật lead dưới đây; bước 5 (fresh rerun) giữ nguyên.

## Context

`references/better-harness` (QoderAI, commit `7cd26e9`, 2026-09-04) là một
plugin đánh giá vòng làm việc xung quanh coding agent. Nó không chạy agent,
không chạy test; nó thu bằng chứng từ project và session, chấm năm chiều, sinh
findings có prompt sửa. Phần lớn bộ máy của nó (điểm số, renderer HTML/Canvas,
adapter cho mười host, DSL Harness as Code, Studio) nằm ngoài PRODUCT.md §6.
Nhưng ba luật vận hành của nó chạm đúng chỗ Pulse còn yếu:

1. **Cấu hình chỉ chứng minh cơ chế tồn tại; chỉ task thực chứng minh nó được
   dùng; chỉ kết quả sau chứng minh nó có ích.** Better Harness gọi tên thang
   này (`Present → Wired → Exercised → Outcome-supported`) và phải suy ra từ
   transcript. Pulse sở hữu receipt nên tính được thang này deterministic,
   nhưng chưa có từ vựng cho nó. Hệ quả hiện tại: một doc registered, một
   role `check` trong `runners.json`, một learning `promoted` đều được nói
   như đã "có", dù chưa Ticket nào chạm tới.
2. **Lane bằng chứng độc lập, lead hoà giải.** Ba agent nhận ba envelope khác
   nhau, không agent nào thấy kết luận của agent khác, lead một mình gán
   severity và chỉ merge khi cùng target, hậu quả, owner, đường sửa. Pulse có
   cấu trúc này cho vòng Ticket (Decision 0006: worker, reviewer, QA là ba
   actor, close gate là lead), nhưng độc lập chỉ bằng lời dặn:
   `reviewer-input.json` mang `handoff.summary` và bootstrap prompt phải nói
   "Do not trust the worker summary". Ở vòng harness, `pulse-ratchet` của
   Decision 0009 là một agent đọc mọi thứ, nên nó có thể thấy một cơ chế được
   cấu hình rồi kết luận cơ chế đó đã được dùng.
3. **Reviewer là bằng chứng, không phải authority.** Skill
   `triangulate-spec-review` của cùng repo chạy hai đến ba reviewer với cùng
   một prompt read-only trên cùng một spec, không chấp nhận theo điểm trung
   bình, chỉ chấp nhận khi không còn P1/P2. Pulse có `review: independent`
   trong profile nhưng nó chỉ nghĩa là "actor khác worker".

Ba lỗ hổng quan sát được trong code hiện tại:

- `findings[]` của reviewer và QA không có shape. Khi reviewer trả `rework`,
  worker lần sau đọc finding trong packet và phải đoán sửa ở đâu, chứng minh
  bằng gì. Điều này vi phạm nguyên tắc 2 ở phía reviewer.
- Receipt, event, knowledge là plane tracked dưới Git và có thể push. Không có
  luật nào chặn absolute path, chuỗi giống secret hay stderr lọt vào đó.
- `verification` receipt chỉ đếm một. Ticket R3 (migration, security,
  destructive) không có cách yêu cầu hai góc nhìn độc lập mà không thêm human
  vào mọi bước.

Hai pattern trong Better Harness cần được phân biệt vì chúng trả lời hai câu
hỏi khác nhau:

| | Pattern A: lane theo miền bằng chứng | Pattern B: tam giác hoá |
|---|---|---|
| Đầu vào | khác nhau mỗi agent | giống hệt mỗi agent |
| Mục đích | chống nhiễm chéo giữa nguồn bằng chứng | giảm blind spot của một model |
| Lead làm gì | merge candidate theo owner, gán severity | tìm hội tụ, thách thức mâu thuẫn |
| Chi phí | cố định theo số lane, chạy một lần mỗi vòng ratchet | nhân theo số reviewer, mỗi lần review |
| Áp vào Pulse | `pulse-ratchet`, sau này `doctor` | chỉ Ticket và Decision risk cao, theo profile |

## Decision

1. **Thang bằng chứng là từ vựng chung, tính từ receipt, không phải điểm.**
   Mỗi cơ chế của harness (doc, role `check`, QA case, learning, profile) có
   một bậc: `present`, `wired`, `exercised`, `outcome_supported`, hoặc
   `missing`, `unobserved`, `not_applicable`. Pulse tính bậc deterministic
   từ registry, `runners.json`, packet, receipt và relation. Không có số
   điểm; không có tổng hợp thành một con số cho cả repo.
2. **Reviewer là bằng chứng, không phải authority.** `reviewer-input.json`
   không mang `handoff.summary` hay bất kỳ lời kể nào của worker; nó mang
   claim để verify: acceptance id, `changed_paths`, proof receipt id, source
   commit. Reviewer không sửa file. Lead (close gate hoặc developer) không
   đưa kết luận của mình vào prompt reviewer.
3. **Finding có shape bắt buộc.** Mọi `findings[]` trong `verification`,
   `qa_checkpoint`, `check` output và `doctor` sau này có `summary`, `owner`
   và `check`; `verification` thêm `acceptance_id`, `qa_checkpoint` thêm
   `case_id`. Finding thiếu `check` vẫn được ghi, mang `unverifiable: true`,
   và không được coi là lý do `rework` một mình. Đếm, filename, tuổi file,
   severity không phải finding.
4. **Plane tracked có ranh giới riêng tư cơ học.** `evidence receipt record`,
   `work handoff`, `work verify`, `note`, `knowledge create|capture` từ chối
   trường text chứa absolute path ngoài repo root hoặc chuỗi khớp mẫu secret.
   `session_ref` là id mờ, được giữ vì là join key duy nhất với transcript
   host. `stderr_tail` chỉ ở run record trong `runtime/`, không vào receipt.
   Nội dung artifact không được quét; developer chịu trách nhiệm với thứ
   mình khai `artifacts[]`.
5. **Tam giác hoá chỉ theo profile risk cao.** Verification profile nhận
   `reviewers: N` (mặc định 1). Close gate đòi N receipt `verification` pass
   trên cùng handoff, từ N actor khác nhau và khác worker. Gate chỉ đếm, không
   merge finding. Có một reviewer `rework` thì Ticket `rework`, worker thấy cả
   hai bộ finding trong packet. `pulse-spec` chạy hai đến ba reviewer cùng
   prompt read-only trên `approach.md` của Story R2 trở lên và trên
   `decision.md` của Decision R3 trước khi human accept; mỗi reviewer ghi
   `note --kind review`, receipt `decision_acceptance` liệt kê note id đã
   đọc. Không bật tam giác hoá toàn cục.
6. **`pulse-ratchet` chạy ba lane độc lập, một lead hoà giải.** Lane
   `execution`, `harness`, `knowledge`, mỗi lane sở hữu một bậc của thang
   bằng chứng, nhận đúng input của mình, read-only, không ủy quyền tiếp,
   không gán severity, trả tối đa năm candidate theo shape finding. Lead là
   chính session ratchet, áp luật hoà giải cố định, chọn một intervention theo
   track, rồi chờ rerun. Binary Pulse không spawn lane; skill dùng công cụ
   fan-out của host.
7. **Learning kind `ratchet` mang `expected_signal`.** Một dòng nói handoff
   của Ticket rerun phải thấy gì để `knowledge_usage: helpful` có thứ đối
   chiếu. Không có `expected_signal` thì learning không được lên `validated`
   chỉ bằng câu tự khai của worker.
8. **Later, đúng thứ tự §11.** `pulse doctor` trả bảng bậc bằng chứng theo cơ
   chế và envelope finding theo shape ở mục 3, offline, deterministic,
   non-blocking. `pulse commands --json` với audience
   `workflow | advanced | maintainer` để khối AGENTS render từ inventory và
   guard test kiểm tra hai chiều. `pulse ratchet bundle` chỉ thêm khi dogfood
   thấy lane rò rỉ sang nhau.
9. **Không lấy.** Điểm năm chiều và trần điểm, support track như một tầng
   báo cáo, renderer HTML, Canvas, host adapter matrix, Harness as Code,
   Studio, Inspector, và khối lượng prose của SKILL.md (Better Harness route
   qua mười hai reference; §12 PRODUCT.md coi đó là dấu hiệu đi sai).

## Thang bằng chứng

| Bậc | Nghĩa | Pulse tính từ đâu |
|---|---|---|
| `present` | cơ chế tồn tại | doc trong registry; role trong `runners.json`; case trong `qa.md`; learning `candidate` hoặc `reviewed` |
| `wired` | có đường để một task chạm tới | doc `required` hoặc `suggested` trong ít nhất một packet; role `check` nằm trong một profile; case được một Ticket khai `QA impact`; learning `promoted` hoặc `applicable` trả về cho một Ticket |
| `exercised` | một task đã dùng và để lại kết quả | `docs_validation` receipt cho doc; `check` receipt cho role; `qa_checkpoint` cho case; handoff ghi `knowledge_usage: helpful` cho learning |
| `outcome_supported` | kết quả sau cho thấy nó có ích | learning có hai provenance `corroborates` từ hai Ticket; check đã chặn một handoff thật (receipt `failed` rồi `passed` trên cùng Ticket) |
| `missing` | inspect xác nhận thiếu | Ticket khai docs `required` nhưng doc id không tồn tại; profile gọi role không có trong `runners.json` |
| `unobserved` | không có bằng chứng để quyết | chưa Ticket nào chạm path của doc; role chưa từng chạy |
| `not_applicable` | inspect chứng minh không áp dụng | Story posture `not_applicable` có lý do; Ticket docs `none` có rationale |

Bậc không phải pass/fail: `exercised` có thể để lộ defect, `unobserved` không
phải `missing`. Bậc mô tả cơ chế, không mô tả Ticket. Bậc của một cơ chế chỉ
tăng khi có receipt mới; không giảm khi receipt cũ hết hiệu lực, nhưng
`doctor` báo receipt gần nhất cách bao nhiêu commit.

Không có điểm số. Lý do: điểm chỉ có ích khi có consumer, và không gate nào
trong Pulse đọc điểm; Better Harness cũng tự nói "điểm không tạo và không chặn
finding". Thêm nữa, Pulse không đọc transcript nên bậc cao nhất từ cấu hình
là `wired`; một con số tổng hợp sẽ mãi phản ánh cấu hình chứ không phản ánh
hành vi.

## Contract reviewer

`reviewer-input.json`:

```json
{
  "schema_version": 1,
  "ticket_id": "TK-031",
  "source_commit": "d4e5f6",
  "contract_revision": 4,
  "acceptance": [{"id": "AC-1"}, {"id": "AC-2"}],
  "handoffs": [{
    "handoff_id": "01JX…H1",
    "changed_paths": ["src/auth/errors.ts", "tests/auth/refresh-token.test.ts"],
    "recorded_by": "agent:runner:worker",
    "source_commit": "d4e5f6"
  }],
  "proof_receipts": {"qa_checkpoint": ["01JX…Q1"], "documentation_validation": []},
  "reviewers_required": 1,
  "artifact_dir": "artifacts"
}
```

Không có `summary`, không có `checks` worker khai, không có `remaining_risk`.
Reviewer lấy contract từ `pulse work show`, diff từ Git, và tự chạy lệnh
verify. Handoff receipt đầy đủ vẫn tra được bằng `pulse evidence receipt
show`, nhưng reviewer phải chủ động mở, và bootstrap prompt không bảo mở.

Reviewer output:

```json
{
  "disposition": "pass|rework",
  "acceptance": {"AC-1": {"check": "pnpm test auth"}, "AC-2": {"check": "…"}},
  "findings": [{
    "acceptance_id": "AC-2",
    "summary": "Response body vẫn chứa stack trace khi token bị revoke.",
    "owner": "src/auth/RefreshTokenHandler.ts",
    "check": "pnpm test auth -- --grep revoked",
    "severity": "high"
  }]
}
```

`owner` là repository-relative path hoặc `DOC-ID#section`. `check` là lệnh
argv reviewer đã chạy và thấy fail, hoặc receipt id. Thiếu `check` thì
`classify_reviewer` giữ finding, gắn `unverifiable: true`; một disposition
`rework` mà mọi finding đều `unverifiable` được ghi `inconclusive`, không
phải verdict. Cùng shape cho QA (`case_id` thay `acceptance_id`) và role
`check`.

## Ranh giới riêng tư

Áp cho mọi trường text của receipt payload, note body, learning
`summary|guidance|title`, và `findings[].summary`:

- Từ chối chuỗi bắt đầu bằng `/` hoặc `[A-Z]:\` mà không nằm dưới repo root
  sau canonicalize. Path dưới repo root được rewrite thành repository-relative.
- Từ chối chuỗi khớp mẫu secret: `AKIA[0-9A-Z]{16}`, `ghp_[A-Za-z0-9]{36}`,
  `sk-[A-Za-z0-9]{20,}`, `-----BEGIN [A-Z ]*PRIVATE KEY-----`, `Bearer
  [A-Za-z0-9._-]{20,}`. Danh sách nằm trong `src/evidence/redaction.rs`, có
  test, mở rộng bằng PR chứ không bằng config.
- Lỗi là `receipt_privacy_violation` kèm tên trường và mẫu khớp, không kèm giá
  trị.

Không quét: `artifacts/sha256/*`, `runtime/`, `cache/`. Không rewrite nội
dung đã ghi.

## Verification profile

```yaml
profiles:
  docs-only:      {commands: ["npm run lint:docs"], review: light}
  service-change: {commands: ["npm run lint", "npm test"], review: standard}
  web-behavior:   {commands: ["npm test"], qa: required, review: standard}
  migration:      {commands: ["npm run test:migrations"], review: independent, reviewers: 2, rollback: required}
  security:       {commands: ["npm test", "npm audit"], review: independent, reviewers: 2, human: required}
```

`reviewers` mặc định 1. `reviewers: 2` không có nghĩa hai model khác nhau; nó
nghĩa hai actor khác nhau, khác worker, mỗi actor một receipt trên cùng
`handoff_id`. Developer chọn model nào qua `runners.json` (`reviewer`,
`reviewer-2`), không phải Pulse.

Close gate mục 3 đổi thành: đủ `reviewers` receipt `verification` pass trên
handoff hiện hành, mỗi receipt một actor khác nhau và khác actor handoff. Một
receipt `rework` là đủ để Ticket `rework`; packet lần sau liệt kê finding của
mọi reviewer, gắn actor.

## Ba lane của `pulse-ratchet`

| Lane | Trả lời | Được đọc | Không được đọc | Bậc sở hữu |
|---|---|---|---|---|
| `execution` | Chuyện gì đã xảy ra khi chạy Ticket này? | receipt của Ticket, run record, `events tail --ticket`, note friction, `knowledge_usage` trong handoff | registry, AGENTS/PULSE.md, `runners.json`, learning | `exercised` |
| `harness` | Cơ chế nào tồn tại và có được wire không? | registry, `AGENTS.md`, `PULSE.md`, `runners.json`, `qa.md`, verification profile, packet đã dùng | receipt, note, learning | `present`, `wired` |
| `knowledge` | Learning hiện có còn đúng, lặp, hay mâu thuẫn không? | `knowledge/`, relation, `freshness`, Decision accepted, docs approved | receipt, note friction | `outcome_supported` (qua `corroborates`) |

Ràng buộc mỗi lane:

- Read-only. Không ủy quyền tiếp. Không chạy lệnh mutating.
- Chỉ nhận input của lane mình và một file reference của lane mình trong
  `skills/pulse-ratchet/references/`.
- Trả tối đa năm candidate, mỗi candidate theo shape finding (mục 3) cộng
  `lane` và `evidence_refs[]` (receipt id, event id, path). Không gán severity.
- Lane thiếu dữ liệu trả `unavailable` kèm lý do, không tự đọc thêm để bù.

Chạy lane bằng lệnh có sẵn trước: `execution` dùng `evidence receipt list
--work`, `events tail --ticket`; `harness` dùng `docs list`, `docs validate`,
đọc file; `knowledge` dùng `knowledge list|search|applicable`. `pulse ratchet
bundle` đóng băng commit, tập Ticket và fingerprint ba lane vào
`.pulse/runtime/ratchet/<id>/{execution,harness,knowledge,bundle}.json` chỉ
được thêm khi dogfood thấy lane rò rỉ sang nhau.

`--lanes execution` cho Ticket R0: hai lane còn lại gần như chắc chắn trống,
không đáng ba agent.

## Luật của lead

Lead là session ratchet. Sau khi ba lane trả kết quả:

1. Giữ mọi candidate. Không bỏ để rút gọn, không gộp theo chủ đề.
2. Merge chỉ khi cùng target, cùng hậu quả quan sát được, cùng owner, cùng
   đường sửa. Hai candidate cùng file nhưng khác hậu quả là hai finding.
3. Lead một mình gán severity và owner cuối cùng. Đếm, filename, tuổi file,
   số note không tạo candidate mới.
4. Mỗi candidate còn lại thành `knowledge capture --from <ticket>` với
   `provenance` trỏ lane, receipt và event. Candidate `unverifiable` vẫn ghi,
   giữ cờ.
5. Không rescan repo sau khi lane xong. Mọi thứ lead biết nằm trong output ba
   lane.
6. Chọn đúng **một** intervention theo track:

   | Track | Bằng chứng | Intervention |
   |---|---|---|
   | bootstrap | không có `AGENTS.md` block, không profile, không registry | dừng, gợi `pulse-onboard` |
   | operationalize | cơ chế `present` nhưng chưa `wired` hoặc `exercised` | wire nó: docs `required` cho path, role `check` vào profile, case vào `QA impact` |
   | optimize | learning có hai provenance `corroborates` | `knowledge promote --target check` hoặc `--agents-md` |
   | undetermined | lane `unavailable` chặn quyết định | ghi finding `evidence_gap`, không sửa gì |

   Track chỉ chọn intervention. Không thêm finding, không đổi severity.
7. Ghi `expected_signal` vào learning kind `ratchet` trước khi sửa, rồi chờ
   rerun theo Decision 0009 bước 5.

## `expected_signal`

```json
{
  "id": "LRN-007", "kind": "ratchet", "scope": "harness", "status": "candidate",
  "title": "Reviewer bỏ sót docs_validation khi Ticket có docs required",
  "expected_signal": "Handoff của Ticket kế tiếp có docs required liệt kê receipt docs_validation trước khi gọi verify.",
  "provenance": {"work": ["TK-002"], "receipts": ["01JX…V2"], "lane": "execution"}
}
```

`knowledge validate` cho learning kind `ratchet` đòi handoff ghi
`knowledge_usage: helpful` **và** developer hoặc ratchet lần sau xác nhận
`expected_signal` xuất hiện trong receipt của Ticket rerun. Không xuất hiện
thì `not_needed` hoặc `misleading`, không `helpful`.

## Thay đổi

Code, cùng lượt với `run.rs` đang dở:

- `src/kernel/run.rs`: `reviewer-input.json` bỏ `summary`, thêm
  `contract_revision`, `reviewers_required`; `classify_reviewer` validate
  shape finding, gắn `unverifiable`, coi `rework` toàn `unverifiable` là
  `inconclusive`; bootstrap prompt reviewer bỏ dòng "Do not trust the worker
  summary" vì không còn gì để không tin.
- `src/evidence/redaction.rs` (mới): mẫu secret, canonicalize path, hàm
  `check_text_fields`. Gọi từ `evidence receipt record`, `work handoff`,
  `work verify`, `note`, `knowledge create|capture`.
- `src/evidence/receipt/verification.rs`, `src/qa/receipt.rs`: `findings[]`
  theo shape mới, `deny_unknown_fields`.
- `src/kernel/completion.rs`: đếm `reviewers` theo profile; actor phân biệt.
- `src/policy/` hoặc parser `PULSE.md`: trường `reviewers` trong profile.
- `src/knowledge/model.rs`: `expected_signal: Option<String>`, bắt buộc khi
  `kind: ratchet`; `knowledge validate` kiểm tra như trên.
- Tests: `tests/runner/reviewer.rs` (input không có summary, finding thiếu
  `check`, hai reviewer), `tests/evidence` (redaction từng mẫu, path dưới và
  ngoài repo root), `tests/knowledge` (`expected_signal` bắt buộc cho
  `ratchet`).

Skill:

- `skills/pulse-ratchet/SKILL.md` ngắn: ba lane, luật lead, track, rerun.
  `references/lane-execution.md`, `lane-harness.md`, `lane-knowledge.md`,
  `lead-reconciliation.md`. Mỗi lane reference liệt kê đúng lệnh được chạy.
- `skills/pulse-spec/SKILL.md`: bước tam giác hoá cho `approach.md` R2+ và
  `decision.md` R3, ghi `note --kind review`.
- `assets/agents-block.md`: dòng "sau work close → pulse-ratchet" không đổi.

Docs:

- PRODUCT.md: header; §5.3 contract reviewer; §5.5 validity, profile, close
  gate; §5.6 thang bằng chứng và vòng ratchet; §5.8 bảng skill; §8; §11; §13
  mục 8; §14.
- `docs/GLOSSARY.md`: thang bằng chứng, lane, lead, tam giác hoá.
- Decision 0009: không sửa; 0012 thu hẹp mục `pulse-ratchet` của nó.

Thứ tự: reviewer input và shape finding trước (đang dở); redaction; profile
`reviewers`; `expected_signal`; skill `pulse-ratchet` chạy trên TK kế tiếp của
todolist với lane bằng lệnh có sẵn; `pulse-spec` tam giác hoá khi có Story R2
thật. `bundle`, `doctor`, `commands --json` sau.

## Consequences

- Reviewer không còn đường tắt là đọc lời worker; mọi verdict phải tự chạy
  check. Chi phí review tăng nhẹ ở Ticket nhỏ, nhưng verdict là bằng chứng
  thật.
- Finding có owner và check nên vòng `rework` không còn đoán; worker rerun
  đúng lệnh reviewer đã chạy.
- Plane tracked không lọt path máy developer hay secret; push repo an toàn
  hơn. Mẫu secret có thể false positive; lỗi kèm tên mẫu để developer sửa
  text, không có cờ bỏ qua.
- Ticket R3 có hai góc nhìn độc lập mà không thêm human vào mọi bước; chi phí
  chỉ tăng ở profile khai `reviewers: 2`.
- `pulse-ratchet` không thể nhầm cấu hình với sử dụng, vì lane biết cấu hình
  không thấy receipt và ngược lại. Giá là ba agent mỗi lần ratchet; R0 chạy
  một lane.
- Thang bằng chứng cho `doctor` một output không cần model và không cần
  transcript, mạnh hơn cách Better Harness phải suy từ session.
- Một field mới (`expected_signal`), một module mới (`redaction.rs`), một
  trường profile mới (`reviewers`). Không schema report, không renderer,
  không điểm.
