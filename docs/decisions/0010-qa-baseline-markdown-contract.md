# Decision 0010: QA baseline là markdown heading, JSON chỉ ở biên runner

## Status

Accepted, 2026-09-06. Thay thế đoạn "Baseline" trong PRODUCT.md §5.5 và QA
input trong §5.3. Implement 2026-09-07.

Bốn điều chỉnh khi implement so với bản Accepted:

1. `baseline_content_hash` là hash **byte nguyên văn** của `qa.md`, không phải
   một dạng "canonical". Receipt `qa_checkpoint` content-bind đúng path đó và
   `content_source_binding_codes` băm lại file trên đĩa để xác định
   currentness; một hash chuẩn hoá sẽ không bao giờ khớp. Chỉ `case_hash` mới
   chuẩn hoá (xuống dòng, khoảng trắng cuối dòng) như §Revision và hash mô tả.
2. Giữ gate `qa_coverage_incomplete` (mọi risk khai báo phải được ít nhất một
   case tham chiếu). Bảng §Trường tài liệu không nhắc lại nó, nhưng quyết định
   này không lật nó và bỏ một gate im lặng thì tệ hơn giữ.
3. `qa-input.json` mang **hai** trường posture: `posture` là posture của
   baseline theo quyết định này (`automated`…`not_applicable`), `qa_posture`
   là QA impact posture của Ticket (`required`, `none`,
   `covered_by_story_close`, `unknown`) mà runner vẫn cần để báo
   `not_applicable`. Hai khái niệm khác nhau, không gộp.
4. Trong dogfood target, case của `examples/todolist` là surface `api` thuần
   nên `pulse-check` gọi `node scripts/qa-case.mjs <CASE-ID>`: assertion miền
   nằm trong script của repo, còn `qa-run.mjs` trở thành executor block
   `pulse-check` hoàn toàn không biết case nào. Đó là cách đọc đúng của §"Case
   không có `check` … trả `inconclusive`": runner script không đoán, và cũng
   không cần bỏ những assertion in-process vốn tốt hơn argv cho một module
   thuần.

## Context

PRODUCT.md §5.5 hiện quy định `works/<STORY>/qa.md` chứa một fenced block
` ```pulse-qa ` JSON cho máy, prose ngoài cho người. `src/qa/baseline.rs` parse
đúng block đó với `deny_unknown_fields`. Trong khi đó quyết định 13.4 chốt
`ticket.md` dùng heading quy ước, không fenced block.

Hệ quả quan sát được trong dogfood: cùng một plane work prose có hai cách mã
hoá; agent viết JSON hay thiếu trường (`revision` từng case) và bị từ chối;
người đọc `qa.md` phải đọc JSON; và `steps`/`expected` chỉ là chuỗi tự do nên
JSON không mua thêm được gì cho máy.

## Decision

1. `qa.md` là markdown với heading quy ước. Không fenced JSON.
2. Pulse parse `qa.md` thành `QaBaseline` và ghi JSON ra
   `.pulse/runtime/run/<ticket>/qa-input.json` cho runner. Runner không đọc
   `qa.md`.
3. Case tự động có thể mang thêm một block `pulse-check` (argv + assertion)
   để script runner chạy không cần diễn giải. Block này tuỳ chọn, chỉ có nghĩa
   khi `Surface` là `cli` hoặc `api`. Case không có block thì runner là agent
   hoặc người diễn giải `Steps`/`Expected`.
4. Receipt `qa_checkpoint` bind hash toàn file `qa.md` và hash section của
   từng case. Không có `revision` viết tay.

## Contract `works/<STORY>/qa.md`

### Cấu trúc

```markdown
# <STORY-ID> QA baseline — <tên ngắn>

<prose tuỳ ý cho người: vì sao các case này định nghĩa "đúng">

## Scope
<một câu: hành vi Story hứa mà baseline này bảo vệ>

## Posture
automated | hybrid | manual_structured | static_proof | not_applicable
<nếu not_applicable: một dòng lý do>

## Risks
- RISK-<NAME>: <mô tả một dòng>

## Exit criteria
- <điều kiện để Story đóng, ví dụ "Mọi case required pass trên candidate">

## Cases

### QA-001 <tiêu đề ngắn>
- Intent: <hành vi người dùng thấy, một câu, không nói implementation>
- Surface: cli | api | ui | job | docs
- Priority: critical | high | normal | low
- Applicability: required | not_applicable
- Reason: <bắt buộc khi not_applicable>
- Risks: RISK-A, RISK-B
- Preconditions:
  - <trạng thái ban đầu, fixture, dữ liệu>
- Steps:
  1. <hành động quan sát được>
  2. <hành động>
- Expected:
  - <kết quả quan sát được, một dòng một assertion>
- Evidence:
  - <artifact runner phải nộp: stdout, screenshot, file sau khi chạy>

```pulse-check
run: node src/cli.mjs add t1 Buy --due 2026-02-30
assert:
  - exit_code: 2
  - stdout_line: InvalidDate
  - file_unchanged: $STATE_FILE
```
```

### Trường tài liệu

| Heading | Bắt buộc | Máy đọc thành | Quy tắc |
|---|---|---|---|
| `# <STORY-ID> …` | có | `story_id` | phải trùng Story sở hữu file |
| `## Scope` | có | `scope` | một đoạn, không rỗng |
| `## Posture` | có | `posture` | một trong năm giá trị; `not_applicable` cần lý do |
| `## Risks` | không | `risks[] {id, summary}` | id `^RISK-[A-Z0-9-]+$`, duy nhất |
| `## Exit criteria` | có | `exit_criteria[]` | ít nhất một bullet |
| `## Cases` | có | `cases[]` | ít nhất một `### QA-` trừ khi posture `not_applicable` |

### Trường case

| Dòng | Bắt buộc | Máy đọc thành | Quy tắc |
|---|---|---|---|
| `### QA-NNN <tiêu đề>` | có | `id`, `title` | id `^QA-[0-9]{3,}$`, duy nhất trong Story |
| `Intent:` | có | `intent` | nói hành vi, không nói selector, hàm, file |
| `Surface:` | có | `surface` | `cli`, `api`, `ui`, `job`, `docs` |
| `Priority:` | có | `priority` | `critical`, `high`, `normal`, `low` |
| `Applicability:` | không, mặc định `required` | `applicability` | `required` hoặc `not_applicable` |
| `Reason:` | khi `not_applicable` | `non_applicable_reason` | |
| `Risks:` | không | `risk_refs[]` | mỗi id phải có trong `## Risks` |
| `Preconditions:` | không | `preconditions[]` | bullet |
| `Steps:` | có | `steps[]` | danh sách đánh số, ít nhất một |
| `Expected:` | có | `expected[]` | bullet, mỗi dòng một điều kiểm tra được |
| `Evidence:` | không | `evidence[]` | bullet; runner phải nộp artifact tương ứng |
| block `pulse-check` | không | `check` | chỉ khi Surface `cli` hoặc `api`; xem dưới |

Mọi dòng `Key:` khác bị từ chối với `qa_baseline_unknown_field` kèm tên dòng,
để agent sửa được ngay.

### Block `pulse-check`

YAML tối giản, Pulse parse thành struct, không shell interpolation:

```yaml
run: <lệnh, tách argv bằng parser argv, không qua sh -c>
cwd: <tuỳ chọn, tương đối repo root>
env:                      # tuỳ chọn
  TODOLIST_STATE: $STATE_FILE
stdin: <tuỳ chọn, chuỗi>
timeout_seconds: 60       # tuỳ chọn, mặc định từ runners.json role qa
assert:
  - exit_code: <int>
  - stdout_line: <chuỗi, khớp một dòng nguyên văn>
  - stdout_contains: <chuỗi>
  - stderr_contains: <chuỗi>
  - stdout_json_path: {path: "$.outcome", equals: "Completed"}
  - file_unchanged: <đường dẫn hoặc $VAR>
  - file_contains: {path: <đường dẫn>, text: <chuỗi>}
```

Biến `$STATE_FILE`, `$ARTIFACT_DIR`, `$REPO` do runner input cung cấp trong
`variables`. Case có block này thì runner script chạy block; case không có
block thì runner agent hoặc người làm theo `Steps`.

### Revision và hash

- `baseline_content_hash`: sha256 canonical của toàn file `qa.md`.
- `case_hash`: sha256 của section case (từ `### QA-` đến trước heading cùng
  cấp kế tiếp), sau khi chuẩn hoá xuống dòng và bỏ khoảng trắng cuối dòng.
- Receipt `qa_checkpoint` ghi cả hai. Close gate so `case_hash` với hiện tại;
  sửa một case chỉ làm stale receipt của case đó.
- Không có `Revision:` viết tay. Đổi nội dung là đổi hash.

### Ràng buộc với Ticket

`## QA impact` trong `ticket.md` ghi `Cases: QA-003, QA-004`. Ready gate
resolve mỗi id trong `qa.md` của Story owner; id không tồn tại hoặc case
`not_applicable` mà Ticket khai `required` thì Ticket không `ready`.

### Runner input

`.pulse/runtime/run/<ticket>/qa-input.json`, sinh từ parse, là JSON duy nhất
trong vòng đời QA:

```json
{
  "schema_version": 1,
  "ticket_id": "TK-003",
  "story_id": "ST-002",
  "qa_scope": "ticket_checkpoint",
  "source_commit": "9f3c1e2…",
  "baseline_path": "works/ST-002/qa.md",
  "baseline_content_hash": "sha256:…",
  "posture": "automated",
  "variables": {
    "REPO": "/abs/path",
    "ARTIFACT_DIR": ".pulse/runtime/run/TK-003/qa-artifacts",
    "STATE_FILE": ".pulse/runtime/run/TK-003/qa-state.json"
  },
  "cases": [
    {
      "id": "QA-004",
      "title": "Ngày sai bị từ chối",
      "case_hash": "sha256:…",
      "intent": "add với ngày không hợp lệ in InvalidDate và không ghi state file",
      "surface": "cli",
      "priority": "high",
      "applicability": "required",
      "risk_refs": ["RISK-BAD-DATE"],
      "preconditions": ["State file rỗng"],
      "steps": ["Chạy add t1 Buy --due 2026-02-30"],
      "expected": ["stdout có dòng InvalidDate", "exit code 2", "state file không đổi"],
      "evidence": ["stdout", "state_file_after"],
      "check": {
        "run": ["node", "src/cli.mjs", "add", "t1", "Buy", "--due", "2026-02-30"],
        "env": {"TODOLIST_STATE": "$STATE_FILE"},
        "assert": [
          {"exit_code": 2},
          {"stdout_line": "InvalidDate"},
          {"file_unchanged": "$STATE_FILE"}
        ]
      }
    }
  ]
}
```

Output của runner giữ nguyên contract §5.3:

```json
{"cases": [{"id": "QA-004", "status": "passed", "observation": "…"}],
 "artifacts": [{"path": "…", "role": "log", "case_id": "QA-004"}],
 "findings": []}
```

Runner script cho case có `check`: chạy `run`, đánh giá `assert` theo thứ tự,
`observation` là assertion đầu tiên fail hoặc "all assertions passed". Case
không có `check` mà runner là script: trả `inconclusive` với lý do "no
executable check", không đoán.

### Việc của người và của agent

| | Người | Agent (spec) | Runner |
|---|---|---|---|
| Scope, Posture, Risks, Exit criteria | duyệt | viết nháp từ story.md | đọc |
| Intent, Steps, Expected | duyệt, sửa ngôn ngữ | viết từ user story | đọc và thực hiện |
| `pulse-check` | không cần đọc | viết khi surface cli/api | chạy |
| Applicability, Reason | quyết | đề xuất | đọc |

## Ví dụ đầy đủ: `works/ST-002/qa.md`

```markdown
# ST-002 QA baseline — Due dates and overdue view

Baseline này định nghĩa "đúng" cho mọi Ticket chạm due date: ngày được lưu và
hiện đúng, ngày sai không làm hỏng state, state cũ vẫn đọc được, và overdue chỉ
tính todo pending.

## Scope
Due date được lưu, hiện trong list, và là đầu vào duy nhất của view overdue.

## Posture
automated

## Risks
- RISK-BAD-DATE: ngày không hợp lệ ghi vào state file làm hỏng dữ liệu.
- RISK-OLD-STATE: state file tạo trước tính năng này không đọc được nữa.

## Exit criteria
- Mọi case required pass trên candidate source.
- Không case nào inconclusive chưa được giải thích.

## Cases

### QA-003 Thêm todo có due rồi list hiện đúng cột
- Intent: Người dùng thêm todo với ngày ISO và thấy ngày đó ở cột thứ ba của list.
- Surface: cli
- Priority: high
- Preconditions:
  - State file rỗng.
- Steps:
  1. Chạy `add t1 Buy --due 2026-09-10`.
  2. Chạy `list`.
- Expected:
  - list in đúng một dòng `t1<TAB>Buy<TAB>2026-09-10`.
  - exit code 0 ở cả hai lệnh.
- Evidence:
  - stdout của list.

```pulse-check
run: node src/cli.mjs add t1 Buy --due 2026-09-10 && node src/cli.mjs list
env:
  TODOLIST_STATE: $STATE_FILE
assert:
  - exit_code: 0
  - stdout_line: "t1\tBuy\t2026-09-10"
```

### QA-004 Ngày sai bị từ chối và không ghi state
- Intent: Người dùng nhập ngày không tồn tại thì thấy InvalidDate và dữ liệu không đổi.
- Surface: cli
- Priority: high
- Risks: RISK-BAD-DATE
- Preconditions:
  - State file rỗng.
- Steps:
  1. Chạy `add t1 Buy --due 2026-02-30`.
- Expected:
  - stdout có dòng `InvalidDate`.
  - exit code 2.
  - State file không đổi.
- Evidence:
  - stdout.
  - State file sau khi chạy.

```pulse-check
run: node src/cli.mjs add t1 Buy --due 2026-02-30
env:
  TODOLIST_STATE: $STATE_FILE
assert:
  - exit_code: 2
  - stdout_line: InvalidDate
  - file_unchanged: $STATE_FILE
```

### QA-005 State file cũ không có due vẫn list được
- Intent: Người dùng nâng cấp vẫn thấy todo cũ, cột due hiện `-`.
- Surface: cli
- Priority: high
- Risks: RISK-OLD-STATE
- Preconditions:
  - State file chứa `[{"id":"old","title":"Legacy","done":false}]`.
- Steps:
  1. Chạy `list`.
- Expected:
  - list in `old<TAB>Legacy<TAB>-`.
- Evidence:
  - stdout.

```pulse-check
run: node src/cli.mjs list
env:
  TODOLIST_STATE: fixtures/qa/legacy-state.json
assert:
  - exit_code: 0
  - stdout_line: "old\tLegacy\t-"
```

### QA-006 Overdue chỉ tính pending có due trước hôm nay
- Intent: Người dùng thấy đúng những todo chưa xong đã quá hạn, không thấy todo đã xong.
- Surface: api
- Priority: high
- Preconditions:
  - Ba todo: pending due 2026-09-01, done due 2026-09-01, pending due 2026-12-01.
  - today = 2026-09-06.
- Steps:
  1. Gọi `overdueTodos(todos, today)`.
- Expected:
  - Kết quả có đúng một todo, là pending due 2026-09-01.
  - Danh sách đầu vào không bị thay đổi.
- Evidence:
  - Kết quả gọi hàm và input trước/sau.
```

`pulse-check` của QA-003 dùng `&&`: ký tự này bị từ chối vì không có shell.
Viết thành hai case, hoặc một `run` duy nhất và assert trên `list` bằng
precondition. Ví dụ giữ nguyên để test parser báo lỗi
`qa_check_shell_operator` rõ ràng.

## Thay đổi

- `src/qa/baseline.rs`: parser heading thay cho fenced JSON; `QaCase` thêm
  `title`, `preconditions`, `evidence`, `check: Option<QaCheck>`, `case_hash`;
  bỏ `revision`. `QaBaseline` thêm `posture`, `risks[] {id, summary}`.
- `src/qa/receipt.rs`: payload `cases[] {case_id, case_hash, outcome}` thay
  `case_revision`.
- `src/kernel/run.rs`: sinh `qa-input.json` theo schema trên, kèm `variables`.
- `examples/todolist/works/ST-001/qa.md` và `scripts/qa-run.mjs`: chuyển
  sang heading và chạy `check`.
- PRODUCT.md §5.5 mục Baseline và §5.3 QA input.
- Test: parse hợp lệ, thiếu trường, unknown field, id trùng, risk ref lạ,
  `check` có toán tử shell, hash section ổn định khi đổi khoảng trắng cuối dòng.
