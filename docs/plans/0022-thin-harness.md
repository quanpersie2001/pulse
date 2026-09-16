# Plan 0022 — Pulse v3: thin harness

> Trạng thái: **draft để implement**, 2026-09-16. File này vừa là bản nháp cho
> Decision 0022 (mục 1–2) vừa là kế hoạch thực hiện (mục 3–14). Khi 0022 được
> viết chính thức, mục 1–2 chuyển sang `docs/decisions/0022-*.md` và file này
> chỉ còn kế hoạch. Nguồn phân tích: `pulse-thin-harness-audit.html` (audit
> 2026-09-16), `docs/dogfood-friction-track-b.md`, `PRODUCT.md` §12.
>
> Quy ước đọc: **MUST** = gate/test phải ép; **SHOULD** = mặc định, lệch phải
> ghi lý do trong commit; **LATER** = không làm trong v3.0.

---

## 1. Quyết định

Pulse v3 **giữ hợp đồng, bỏ máy móc**:

- Giữ: Ticket là đơn vị một agent một session; `done` chỉ do gate đọc evidence;
  verifier khác worker; reviewer nhận claim chứ không nhận lời kể; finding có
  `owner` + `check`; friction → learning → check; ADR.
- Bỏ: authority policy, docs registry/index/search/cache/applicability/receipt,
  knowledge relation graph, materialization R0–R3, priority, hai họ receipt,
  mini-DSL trên flag, `works/` cho work item, `work sync`/`brief_hash`, parser
  heading, worktree mirroring/state routing, 8 skill.
- Thêm đúng ba thứ: `pulse checkpoint` + vòng `continue` trong runner,
  `pulse board`, store `issues.jsonl` có schema.

Mọi cơ chế bị bỏ chỉ được thêm lại khi **một dogfood đo được** cho thấy ≥ 2
friction cùng loại mà cơ chế đó giải quyết.

## 2. Số đo bắt buộc

Ghi vào `docs/plans/0022-metrics.md` ở đầu Phase 0 và cuối mỗi phase.

| Số đo | Baseline 2026-09-16 | Đích v3.0 |
|---|---|---|
| Dòng Rust `src/` | 43.106 | **< 10.000** |
| Số lệnh CLI (leaf) | ~60 | **≤ 22** |
| Mã lỗi riêng biệt | 271 | **< 40** |
| Lệnh gõ tay để đóng một Ticket | ~11 | **≤ 6** |
| Flag bắt buộc trên đường đó | ~25 | **≤ 4** |
| Friction/Ticket là lỗi Pulse | (Track B: đa số) | **< 1** |
| Repo chạy Pulse thật | 0 | **1** (UI + API) |

---

## 3. Layout repo đích

```text
AGENTS.md                  # khối PULSE ≤ 80 dòng (mục 12.1)
PULSE.md                   # profiles + lanes + human gate (mục 8)
docs/
  README.md                # bản đồ docs, tay viết, ≤ 60 dòng
  product/ architecture/ domain/ operations/ decisions/
                           # frontmatter tuỳ chọn: applies_to, tags, generated_by
.pulse/
  issues.jsonl             # store duy nhất cho epic/story/ticket/decision
  receipts/<ulid>.json     # một họ receipt
  evidence/<issue-id>/     # handoff.json, checkpoint-*.json, <lane>.json, shots/, logs/
  events/<YYYY-MM-DD>.jsonl
  learnings/LRN-<hash>.md
  runners.json
  runtime/                 # lock, lease, run/<id>/ ; gitignored
  cache/                   # board.html ; gitignored
```

Tracked: `issues.jsonl`, `receipts/`, `evidence/` (trừ file > 5 MB, xem
10.5), `events/`, `learnings/`, `runners.json`. `.gitignore` do `pulse init`
ghi phải dùng pattern không neo gốc: `**/.pulse/runtime/`, `**/.pulse/cache/`
(lỗi đã ghi ở ROADMAP).

Không còn `works/`, `.pulse/workgraph/`, `.pulse/docs/`, `.pulse/knowledge/`,
`.pulse/policy/`, `.pulse/evidence/execution/`, `.pulse/config/`.

---

## 4. Data model — `issues.jsonl`

### 4.1 Quy tắc file

- Một dòng = một record JSON canonical (key sorted, không pretty). Dòng trống
  và dòng bắt đầu `#` bị bỏ qua khi đọc (không bao giờ được ghi ra).
- Sort theo `id` khi ghi. Ghi = đọc toàn bộ → mutate trong bộ nhớ → validate →
  ghi temp + fsync + rename dưới `storage::lock`. Một mutation = một event.
- `revision` là CAS **nội bộ**: CLI đọc revision hiện tại và ghi; chỉ lệnh
  `update --if-revision N` mới lộ nó ra (dùng cho agent song song, LATER).
- Không có `manifest.json`; schema version nằm trong từng record
  (`"schema": 3`).
- Đọc file hỏng: dòng không parse được → lệnh **fail** với
  `issues_line_invalid` kèm số dòng. Không bỏ qua im lặng (bài học 0017).

### 4.2 ID

`<PREFIX>-<4 hex>`; prefix `EP|ST|TK|DEC`; 4 hex = 16 bit đầu của
`sha256(kind + title + created_at + 8 byte random)`. Va chạm trong cùng file →
sinh lại. Không counter, không display id riêng. `LRN-<4 hex>` cùng cách.

### 4.3 Trường chung (mọi kind)

```json
{"schema":3,"id":"TK-a3f9","kind":"ticket","title":"…","status":"ready",
 "revision":4,"created_at":"…","updated_at":"…","tags":["security"],
 "deps":[{"type":"blocked_by","id":"TK-9b02"}],
 "notes":[{"at":"…","from":"human:quan","kind":"note|friction","text":"…"}]}
```

`deps[].type ∈ {blocked_by, supersedes}`. `parent` không phải dep: Ticket có
`story`, Story có `epic` (trường đơn, tối đa một cha). `related` → prose.
`notes` là projection của event `note.recorded` để board và packet đọc nhanh;
nguồn sự thật vẫn là event log (tối đa 50 note giữ trên record, cũ hơn cắt).

### 4.4 Ticket

```json
{"kind":"ticket","story":"ST-7c21",
 "role":"implementation|decision_work",
 "risk":"low|medium|high","surface":"cli|api|ui|lib|docs",
 "freedom":"locked|guided|open",
 "objective":"markdown",
 "context":{"anchors":["path[:symbol]"],"docs":["docs/…"],"decisions":["DEC-…"]},
 "change":{"required":["…"],"invariants":["…"],"docs_to_update":["docs/…"]},
 "non_scope":["…"],
 "acceptance":[{"id":"AC-1","given":"…","when":"…","then":"…"}],
 "verify":[{"name":"auth","argv":["pnpm","test","auth"],"cwd":"."}],
 "qa_cases":["QA-001"],
 "open_questions":[{"q":"…","disposition":"resolved|rejected|delegated|deferred|blocking","answer":"…","ref":"DEC-…"}],
 "lease":{"role":"worker","actor":"agent:worker","run_id":"…","expires_at":"…"} ,
 "checkpoints":[{"at":"…","run_id":"…","done_ac":["AC-1"],"in_progress":"…","next":["…"],
                 "files":["…"],"decisions":["…"],"gotchas":["…"],
                 "commands_run":[{"argv":["…"],"exit":0}]}],
 "verdicts":{"review-correctness":{"receipt":"01J…","verdict":"pass","commit":"d4e5f6"}}}
```

- `given` tuỳ chọn; `when`/`then` bắt buộc, không rỗng. Đây là EARS tối
  thiểu; reviewer và QA map 1:1.
- `verify[].argv` là argv, không shell. `cwd` mặc định repo root.
- `lease` và `verdicts` do runner/gate ghi, `update` từ chối sửa
  (`field_owned_by_runtime`).
- `role: decision_work` bỏ qua ready-gate về anchors và acceptance; thay bằng
  `question` (chuỗi) và `deliverable` (đường dẫn dưới `.pulse/evidence/<id>/`).

### 4.5 Story

```json
{"kind":"story","epic":"EP-…","risk":"…","surface":"…",
 "outcome":"markdown",
 "rules":[{"id":"BR-1","text":"…"}],
 "exceptions":[{"id":"E-1","text":"…"}],
 "approach":"markdown | null",
 "qa_cases":[{"id":"QA-001","intent":"…","surface":"api","priority":"high",
              "preconditions":["…"],"steps":["…"],"expected":["…"],
              "check":{"argv":["node","scripts/qa/x.mjs"],"assert":[{"exit_code":0}]}}],
 "open_questions":[…]}
```

`qa_cases[].check` tuỳ chọn; có thì lane `qa-*` script chạy được không cần
agent. `approach` SHOULD có khi `risk ≥ medium`.

### 4.6 Epic và Decision

```json
{"kind":"epic","outcome":"…","success_signals":["…"],"out_of_scope":["…"],"not_yet_specified":["…"]}
{"kind":"decision","status":"proposed|accepted|superseded","context":"…",
 "options":[{"name":"…","consequences":"…"}],"decision":"…","consequences":"…",
 "accepted_by":"human:…","accepted_at":"…"}
```

### 4.7 Lifecycle

```text
ticket:   draft -> ready -> active -> verifying -> done
                     |         |          |
                     v         v          v
                  blocked   blocked    active (rework: verdict fail đính kèm)
          draft|ready|blocked -> cancelled
story:    draft -> ready -> done | cancelled
epic:     draft -> active -> done | cancelled
decision: proposed -> accepted | superseded
```

- `draft -> ready`: `pulse ready <id>` chạy ready gate (mục 7.1) rồi chuyển.
- `ready -> active`: chỉ qua `pulse run worker`.
- `active -> verifying`: chỉ qua `pulse handoff`.
- `verifying -> done`: chỉ qua `pulse close`.
- `verifying -> active`: khi một lane trả `fail`; `verdicts` giữ lại finding.
- Supersede = `deps[{type:supersedes}]` trên record mới + `cancelled` với
  `reason: superseded_by:<id>` trên record cũ.

### 4.8 JSON Schema

`src/schema/issue.schema.json` (một file, `oneOf` theo `kind`), nhúng bằng
`include_str!`, validate bằng crate `jsonschema` tại mọi ghi và tại
`pulse doctor`. Đây là **schema duy nhất** được nhúng; không schema cho
receipt/event (serde struct là đủ vì Pulse là writer duy nhất của chúng).

---

## 5. Receipt, evidence, event

### 5.1 Receipt (một họ)

```json
{"id":"01J…","kind":"handoff|lane|checkpoint|close|close_story|decision_accept",
 "subject":{"id":"TK-a3f9","revision":4},
 "actor":"agent:worker|agent:review-correctness|human:quan",
 "source":{"commit":"d4e5f6","dirty_hash":"sha256:…"},
 "recorded_at":"…","run_id":"…",
 "payload":{…},
 "artifacts":[{"path":".pulse/evidence/TK-a3f9/shots/QA-001.png","sha256":"…"}]}
```

- Bất biến; id ULID; content hash tính từ canonical JSON; ghi lại cùng nội
  dung idempotent, khác nội dung cùng id là `receipt_conflict`.
- `payload` của `lane` = nội dung `<lane>.json` (mục 8.4) đã validate.
- `artifacts[]` = mọi file dưới `.pulse/evidence/<id>/` mà lane khai; Pulse hash
  lúc seal. File khai mà không tồn tại → `artifact_missing`, receipt không ghi.
- Redaction (giữ từ `src/evidence/redaction.rs`, thu về ~80 dòng): từ chối
  chuỗi khớp mẫu secret; absolute path dưới repo root rewrite thành relative;
  ngoài repo root → `receipt_privacy_violation`.

### 5.2 Source fence

`src/source.rs` viết lại ~150 dòng:

```rust
pub struct Source { commit: String, dirty_hash: String, dirty_paths: Vec<String> }
pub fn snapshot(repo: &Path) -> Result<Source>;   // HEAD + sha256(status -z + diff --binary) sau khi lọc
pub fn same(a: &Source, b: &Source) -> bool;
```

Lọc **ra khỏi** fence: `.pulse/**`, và mọi path khớp `fence_ignore` trong
`PULSE.md` (mặc định rỗng). Bài học Track B: sửa file ghi chú không được làm
stale một close. Gate so `same(handoff.source, now)`; khác → lỗi nêu rõ
`dirty_paths` khác nhau.

### 5.3 Event log

Giữ `src/event.rs` + `storage::append_line_fsync`. Event type gọn:
`issue.created|updated|transitioned`, `note.recorded`, `run.started|completed`,
`receipt.recorded`, `learning.added|retired`. Torn tail giữ như 0011.

---

## 6. CLI

Mọi lệnh: `--repo-root`, `--json`; exit 0/1/2 (2 = lỗi dùng sai). JSON output
luôn có `"ok": bool` và khi lỗi `{"ok":false,"error":{"code":"…","message":"…","hint":"…"}}`.
`hint` là bắt buộc cho mọi mã lỗi (friction: lỗi không gợi ý cách sửa).

| Lệnh | Làm gì | Mutation |
|---|---|---|
| `pulse init [--refresh]` | tạo `.pulse/`, `PULSE.md`, khối AGENTS, `docs/README.md` nếu thiếu, `.gitignore` | ✓ |
| `pulse new <kind> <title> [--story ID] [--epic ID] [--risk] [--surface] [--from file.json]` | tạo record `draft`; `--from` nạp trường chi tiết | ✓ |
| `pulse show <id>` | record + verdict + checkpoint mới nhất + deps resolved | |
| `pulse list [--kind] [--status] [--story] [--tag] [--ready]` | bảng ngắn | |
| `pulse tree [<epic|story>]` | epic → story → ticket với status | |
| `pulse ready <id>` | chạy ready gate; pass thì `draft -> ready`; fail in lý do | ✓ |
| `pulse update <id> --set k=v … \| --from file.json \| --stdin` | merge trường (không phải trường runtime), validate, revision++ | ✓ |
| `pulse dep add <id> blocked_by\|supersedes <other>` / `dep rm` | sửa `deps`, kiểm cycle | ✓ |
| `pulse transition <id> --to blocked\|cancelled --reason "…"` | các chuyển thủ công còn lại | ✓ |
| `pulse packet <id>` | JSON packet (mục 9) | |
| `pulse run <role> <id> [--ttl 3600] [--continue-limit 5]` | runner (mục 10) | ✓ |
| `pulse checkpoint <id> --from cp.json` | thêm checkpoint, giữ lease | ✓ |
| `pulse handoff <id> --from handoff.json` | seal receipt handoff, `active -> verifying` | ✓ |
| `pulse release <id>` | gỡ lease hết hạn/kẹt, `active -> ready` | ✓ |
| `pulse close <id>` | close gate (7.3) | ✓ |
| `pulse close-story <id>` | story gate (7.4) | ✓ |
| `pulse note <id> <text> [--friction] [--from actor]` | event + notes[] | ✓ |
| `pulse events tail [--since] [--issue] [--follow]` | | |
| `pulse learn add\|list\|show\|applicable <id>\|retire` | mục 11 | ✓/ |
| `pulse docs applicable <id>` / `pulse docs check` | mục 12.2 | |
| `pulse board [--watch] [--open]` | render `cache/board.html` | |
| `pulse doctor` | present/wired/exercised + lỗi store | |

22 lệnh leaf. Không có: `graph *`, `evidence *` (receipt chỉ đọc qua `show`
và `board`; `pulse show --receipts` in đủ), `qa *`, `knowledge *`, `docs
register|index|search|get`.

### 6.1 Actor

Chuỗi `kind:id`, kind ∈ `human|agent`. `--from`/`--actor` không bắt buộc:
mặc định từ `PULSE_ACTOR` env, rồi `git config user.name` → `human:<name>`.
Runner đặt `PULSE_ACTOR=agent:<role>` cho process con. Bare id bị từ chối với
hint nêu đúng dạng.

### 6.2 Ma trận vai trò (thay authority policy)

| Hành động | human | agent:worker* | agent:review-*/qa-* |
|---|---|---|---|
| new/update/ready/dep/transition | ✓ | ✗ (`role_forbidden`) | ✗ |
| checkpoint/handoff | ✓ | ✓ | ✗ |
| lane receipt (qua `run`) | — | ✗ | ✓ |
| close/close-story | ✓ | ✗ | ✗ |
| note/learn add | ✓ | ✓ | ✓ |
| accept decision | ✓ | ✗ | ✗ |

Cố định trong `src/kernel/roles.rs` (~40 dòng). Không file policy.

---

## 7. Gate

### 7.1 Ready gate (`pulse ready`)

Ticket `implementation` MUST:

1. `acceptance` ≥ 1, mỗi phần tử có `id` duy nhất, `when`, `then` không rỗng.
2. Mọi `context.anchors` tồn tại trên đĩa (phần trước `:`).
3. Không `open_questions[]` với `disposition: blocking` hoặc thiếu disposition.
4. Mọi `deps[blocked_by]` có status `done|cancelled`.
5. `story` (nếu có) tồn tại; `qa_cases[]` resolve được trong Story.
6. `risk` và `surface` đã khai (không `null`).

`decision_work`: chỉ 3, 4 và `question` không rỗng. Story `ready`: `outcome`
không rỗng, ≥ 1 rule hoặc qa_case, không open question blocking.

Mỗi điều kiện một mã lỗi: `ready_acceptance_missing`, `ready_anchor_missing`,
`ready_question_blocking`, `ready_blocked_by_open`, `ready_qa_case_unresolved`,
`ready_classification_missing`. Output liệt kê **tất cả** lỗi, không dừng ở
lỗi đầu.

### 7.2 Handoff gate (`pulse handoff --from handoff.json`)

```json
{"summary":"một dòng","changed_files":["…"],
 "acceptance":[{"id":"AC-1","status":"done|partial|not_done","how":"…"}],
 "verify_results":[{"name":"auth","exit":0}],
 "docs_updated":["docs/…"],
 "learnings_used":[{"id":"LRN-…","usage":"helpful|not_needed|misleading"}],
 "friction":["…"],"open_risks":["…"]}
```

MUST: Ticket `active` với lease của actor gọi; mọi `acceptance[].id` của
Ticket xuất hiện; `verify_results` cover mọi `verify[].name`; nếu
`change.docs_to_update` không rỗng thì `docs_updated` ⊇ nó **và** các file đó
nằm trong `git diff --name-only` (bài học 0016, kiểm ở bước người sửa được).
Pass → receipt `handoff`, source snapshot, `active -> verifying`, thả lease.
`friction[]` → event `note.recorded kind=friction` từng dòng.

Handoff **không** mang `checks` tự khai để reviewer tin; reviewer chạy lại.

### 7.3 Close gate (`pulse close`)

MUST, liệt kê toàn bộ vi phạm:

1. Ticket `verifying`.
2. Receipt `handoff` mới nhất có `source.same(now)` — khác thì
   `close_source_stale` kèm `dirty_paths` lệch.
3. Với mọi lane trong profile (mục 8.2) của `risk × surface`: có receipt `lane`
   với `verdict: pass`, cùng `source.commit` với handoff, `actor` ≠ actor
   handoff, không finding `severity: high` chưa `resolved`.
4. Profile `human: required` → actor close là `human:*`.
5. Mọi `open_questions` không `blocking`.

Pass → receipt `close`, `verifying -> done`, event. Không có "flaky", không
waiver riêng: lane fail thì rerun lane; muốn bỏ qua thì human sửa profile
Ticket bằng `update --set risk=low` có lý do trong event.

### 7.4 Close-story gate

Mọi Ticket con `done|cancelled`, ≥ 1 `done`; nếu Story có `qa_cases` với
`priority: high` thì có receipt lane `qa-*` scope `story` pass trên HEAD hiện
tại cover đủ case đó; actor ≠ actor lane. Ghi receipt `close_story`.

---

## 8. Lane và profile

### 8.1 `PULSE.md`

```yaml
# PULSE.md — do pulse init seed, human sửa
fence_ignore: []
profiles:                       # key = <surface>-<risk>
  cli-low:   {lanes: [review-correctness]}
  lib-low:   {lanes: [review-correctness]}
  api-low:   {lanes: [review-correctness]}
  ui-low:    {lanes: [review-correctness, qa-ui]}
  api-medium:{lanes: [review-correctness, qa-api]}
  ui-medium: {lanes: [review-correctness, qa-ui]}
  api-high:  {lanes: [review-correctness, review-adversarial, qa-api], human: required}
  ui-high:   {lanes: [review-correctness, review-adversarial, qa-ui, qa-api], human: required}
  docs-low:  {lanes: [check-docs]}
  decision_work: {lanes: []}
```

Thiếu key → `profile_missing` với hint. Pulse không có profile mặc định
hard-code ngoài file `init` seed.

### 8.2 `runners.json`

```json
{"worker":            {"command": "claude -p --output-format text --dangerously-skip-permissions \"Pulse worker. Read {input} and follow it exactly.\"", "timeout_seconds": 3600},
 "worker-continue":   {"command": "codex exec --sandbox workspace-write \"Pulse worker continue. Read {input}.\"", "timeout_seconds": 3600},
 "review-correctness":{"command": "codex exec --sandbox read-only \"Pulse reviewer. Read {input}.\"", "timeout_seconds": 1800},
 "review-adversarial":{"command": "claude -p --output-format text \"Pulse adversarial reviewer. Read {input}.\"", "timeout_seconds": 1800},
 "qa-ui":             {"command": "node scripts/qa/ui.mjs {input}", "timeout_seconds": 900},
 "qa-api":            {"command": "node scripts/qa/api.mjs {input}", "timeout_seconds": 900},
 "check-docs":        {"command": "pulse docs check --json", "timeout_seconds": 120}}
```

Mỗi role là một `"command"` (chuỗi, không phải `"argv"` mảng), tách thành argv
bằng `runner::split_argv` — hỗ trợ quote đơn/kép, không bao giờ qua shell —
cộng `"timeout_seconds"`; `"max_output_bytes"` tuỳ chọn (mặc định 8 MiB).
Placeholder trong `command`: `{input}`, `{ticket}`, `{repo}`, `{artifact_dir}`
— khớp `runner::PLACEHOLDERS`; một placeholder lạ (`{issue}`, `{evidence_dir}`,
…) là lỗi `runner_placeholder_unknown` khi spawn, không im lặng bỏ qua.
`worker-continue` tuỳ chọn; thiếu thì `continue` dùng lại `worker`.

### 8.3 Input của từng lane

Pulse ghi `.pulse/runtime/run/<id>/<role>-input.json`:

| Role | Nhận | KHÔNG nhận |
|---|---|---|
| `worker` | packet (mục 9), checkpoint mới nhất, đường dẫn viết `handoff.json`/`cp.json`, protocol | — |
| `review-correctness` | ticket record (objective, change, acceptance, verify, docs_to_update), `git diff <handoff.commit_base>..HEAD --stat` + lệnh để tự lấy diff, danh sách file đổi, đường dẫn evidence dir | `handoff.summary`, `verify_results` của worker, checkpoint |
| `review-adversarial` | như trên + `invariants`, `non_scope`, `rules`/`exceptions` của Story | như trên |
| `qa-ui` / `qa-api` | Story `qa_cases` được Ticket trỏ (scope `ticket`) hoặc toàn bộ (scope `story`), `verify` env từ `docs/operations/run.md` nếu khai, evidence dir, viewport list | mọi thứ của worker |
| `check-*` | issue id, evidence dir | — |

Nguyên tắc: lane nhận **claim để kiểm**, không nhận lời kể. Lane chạy trong
session mới; runner đặt `PULSE_ACTOR=agent:<role>` và, cho lane, `cwd` giống
worker.

### 8.4 Output của lane — một shape

Lane ghi `.pulse/evidence/<id>/<role>.json` rồi in dòng cuối stdout
`{"status":"done"}`. Pulse đọc file, validate, seal receipt.

```json
{"verdict":"pass|fail|inconclusive",
 "acceptance":[{"id":"AC-1","status":"pass|fail|not_checked","how":"chạy pnpm test auth, 12 pass"}],
 "cases":[{"id":"QA-001","status":"pass|fail|inconclusive","observation":"…",
           "artifacts":["shots/QA-001-desktop.png","shots/QA-001-mobile.png","logs/QA-001.console.txt"]}],
 "findings":[{"id":"F-1","ref":"AC-2|QA-004|-","summary":"…","owner":"src/auth/errors.ts",
              "check":{"argv":["pnpm","test","auth","--","-t","revoked"],"exit":1},
              "severity":"high|medium|low","status":"open"}],
 "commands_run":[{"argv":["…"],"exit":0}],
 "environment":{"commit":"d4e5f6","server":"http://127.0.0.1:3000","tool":"playwright 1.5x"}}
```

Quy tắc MUST khi seal:

- `verdict: pass` mà có `acceptance[].status: fail` hoặc finding `high` `open`
  → seal thành `fail` và ghi `lane_verdict_corrected`.
- `verdict: fail` mà không finding nào có `check` → seal thành `inconclusive`
  (finding không kiểm được không một mình tạo rework — giữ từ 0012).
- Lane `qa-ui`: mỗi case `pass` MUST có ≥ 1 artifact ảnh tồn tại; thiếu → case
  `inconclusive`, verdict theo đó.
- Lane `qa-api`: mỗi case `pass` MUST có ≥ 1 artifact log/response.
- `environment.commit` phải bằng HEAD lúc chạy, khác → `lane_commit_mismatch`,
  không receipt.
- Lane không được sửa source: runner so `dirty_hash` trước/sau; đổi →
  `lane_mutated_workspace`, không receipt (lane chỉ được ghi dưới
  `.pulse/evidence/<id>/`).

### 8.5 Prompt mẫu cho lane agent (tài liệu, không code)

`assets/prompts/review-correctness.md`, `review-adversarial.md`, `worker.md`.
Mỗi prompt ≤ 60 dòng: identity, input ở đâu, được/không được làm gì, output
ghi đâu, câu cuối in gì. Không copy nội dung Ticket vào prompt.

### 8.6 Template script QA (cho repo đích)

`assets/qa/ui.mjs` (Playwright): đọc input → với mỗi case: navigate theo
`steps`, chụp 1280×800 và 375×812, ghi console vào `logs/`, lấy a11y
snapshot, so `expected` bằng agent-free assert khi case có `check`, nếu không
đánh `inconclusive` kèm ảnh để lane agent/human kết luận. `assets/qa/api.mjs`:
start server theo `docs/operations/run.md` (`run.start`, `run.ready_url`),
gọi theo `steps`, lưu response + tail log server. `pulse init --with-qa-templates`
copy vào `scripts/qa/`.

---

## 9. Packet

`pulse packet <id>` — JSON bounded, thứ duy nhất worker đọc trước khi làm:

```json
{"issue":{…record đầy đủ trừ lease/verdicts…},
 "story":{"id":"…","outcome":"…","rules":[…],"exceptions":[…],"approach":"…"},
 "epic":{"id":"…","outcome":"…","out_of_scope":[…]},
 "decisions":[{"id":"…","title":"…","decision":"…","consequences":"…"}],
 "blockers":[{"id":"…","status":"…"}],
 "docs":{"applicable":[{"path":"…","why":"anchor src/auth/** ∩ applies_to","lines":120}],
         "map":"docs/README.md"},
 "learnings":[{"id":"LRN-…","summary":"…","do":[…],"avoid":[…],"check":"…"}],
 "checkpoint":{…mới nhất hoặc null…},
 "last_verdicts":[{"lane":"…","verdict":"fail","findings":[…]}],
 "notes":[…8 note mới nhất…],
 "source":{"commit":"…","dirty":false},
 "protocol":{"checkpoint":"pulse checkpoint TK-x --from <path>",
             "handoff":"pulse handoff TK-x --from <path>",
             "continue_exit":"{\"status\":\"continue\"}"}}
```

Không inline nội dung docs; `docs.applicable[].lines` để agent quyết đọc.
Không fingerprint từng input; fence duy nhất là `source`. Packet stale =
`source` đổi; runner chỉ dùng packet nó vừa tạo.

---

## 10. Runner và vòng continue

### 10.1 `pulse run worker <id>`

```text
1. Ticket ready|active. active mà lease của actor khác còn hạn → run_lease_held.
   Một Ticket active khác trong repo → run_another_active (hint: đóng/release nó).
2. Lấy/gia hạn lease; ready -> active.
3. Ghi packet + input; evidence dir tạo nếu chưa.
4. attempt = 0
   loop:
     spawn role (attempt == 0 ? worker : worker-continue||worker), timeout, process group,
       stdout/stderr bounded 1 MB, PULSE_ACTOR, {input} = worker-input.json (có checkpoint mới nhất)
     đọc dòng JSON cuối:
       handed_off  → xác nhận receipt handoff tồn tại; kết thúc ok
       blocked     → note blocked + reason; active -> blocked; kết thúc
       continue    → MUST có checkpoint mới hơn lúc spawn (không → coi như crash);
                     attempt++; attempt > continue_limit → blocked reason=needs_split; kết thúc
                     else loop
       khác/timeout/exit≠0 → run record inconclusive; lease giữ; kết thúc lỗi run_inconclusive
5. Event run.completed với attempt, duration, outcome.
```

Crash giữa chừng: chạy lại `pulse run worker` → resume từ checkpoint mới nhất
(bước 4 với attempt tiếp). Lease hết TTL không handoff → `pulse release`.

### 10.2 `pulse run <lane> <id>`

Ticket `verifying`; profile phải chứa lane (`lane_not_in_profile`, nhưng
`--force` cho chạy thêm lane). Spawn, đợi `{"status":"done"}`, đọc
`<role>.json`, validate 8.4, so dirty_hash, seal receipt, cập nhật
`verdicts[role]`. `fail` → `verifying -> active` với event `rework`.
`pulse run review <id>` (role ảo) = chạy tuần tự mọi lane của profile chưa
có verdict pass trên commit hiện tại.

### 10.3 Checkpoint

`pulse checkpoint <id> --from cp.json`: actor phải là chủ lease; validate
schema (4.4); append `checkpoints[]` (giữ tối đa 10, cũ hơn chuyển sang
`.pulse/evidence/<id>/checkpoint-<n>.json`); receipt `checkpoint` (payload =
checkpoint); không đổi status. Worker prompt MUST nói: checkpoint sau mỗi AC
xong và trước khi thoát `continue`.

### 10.4 Detector cho host (không phải code Pulse)

`assets/hosts/claude-code/statusline.sh`: đọc
`.context_window.used_percentage`; ≥ 70 → `touch
.pulse/runtime/context-threshold` (nếu `.pulse/runtime/run/current` tồn tại).
`assets/hosts/claude-code/post-tool-use.sh`: nếu marker tồn tại → in
`{"decision":"continue","reason":"Context ≥70%: pulse checkpoint rồi thoát {\"status\":\"continue\"}"}`
một lần rồi xoá marker. `pulse init --host claude-code` copy hai file và in
đoạn JSON cần dán vào `settings.json`. Host khác: chỉ quy tắc trong worker
prompt + timeout.

### 10.5 Artifact

Runner hash mọi file dưới `.pulse/evidence/<id>/` mà lane khai. File > 5 MB →
lưu hash, không track git (`.pulse/evidence/**/*.{webm,mp4,zip}` vào
`.gitignore` do init ghi), receipt vẫn giữ sha256 + size.

### 10.6 Không có trong runner v3

Worktree tạo/mirror/route, `worktree_graph_stale`, `--isolation`, session_ref,
harness-learning section trong prompt (learnings đi qua packet), QA scope
`story_close` là `pulse run qa-<x> <story-id>` với record kind story.

---

## 11. Learnings

### 11.1 File

`.pulse/learnings/LRN-<hash>.md`:

```markdown
---
id: LRN-3f2a
status: candidate | active | retired
kind: failure | constraint | technique | routing
applies_to: ["src/auth/**"]
tags: [security]
from: [TK-a3f9, "01J…"]
expected_signal: "handoff của Ticket chạm src/auth/** ghi rõ rotation là atomic"
usage: {helpful: 0, not_needed: 0, misleading: 0}
---
## Summary
Refresh song song tạo token invalid khi rotation là check-then-act.
## Do
- Transaction hoặc optimistic conflict.
## Avoid
- Tách read và write khi rotate.
## Check
- Chạy 10 refresh song song, đúng một cái thành công.
```

### 11.2 Lệnh

- `pulse learn add --from lrn.md|--title … --applies-to … --kind …` → `candidate`.
- `pulse learn applicable <id>`: `applies_to` glob ∩ `context.anchors` **hoặc**
  `tags` ∩ `tags`; chỉ `status: active` vào packet (tối đa 5, sort theo
  `usage.helpful` giảm dần); `candidate` chỉ hiện với `--all`.
- Handoff `learnings_used[]` cập nhật `usage`; `candidate` → `active` tự động
  khi `helpful ≥ 1` **và** human chạy `pulse learn activate <id>` (hai nửa, giữ
  từ 0012 nhưng không so prose).
- `pulse learn retire <id> --reason` → `status: retired` (file giữ).
- `pulse doctor` cảnh báo: `candidate` > 30 ngày, `active` chưa xuất hiện
  trong packet nào (`present` chưa `wired`), `misleading ≥ 2`.

### 11.3 Thang bằng chứng (từ file, không Rust riêng)

`pulse doctor` tính cho mỗi cơ chế: doc trong `docs/README.md` có
`applies_to` (`present`) → đã vào packet nào (`wired`, đếm từ event
`run.started` payload) → Ticket handoff ghi `docs_updated` hoặc lane cite
(`exercised`). Tương tự cho lane role và learning. In bảng, không điểm.

---

## 12. Guidance surface

### 12.1 Khối AGENTS (≤ 80 dòng, `assets/agents-block.md` viết lại)

Nội dung bắt buộc, theo thứ tự: (1) Pulse là gì trong repo này, một đoạn;
(2) bốn câu hỏi trước mutation; (3) route theo hình dạng: read-only / R0
(`pulse new ticket` + `ready` + `run`) / lớn hơn (skill `pulse-shape` →
`pulse-plan`); (4) completion standard = receipt; (5) friction → `pulse note
--friction`; (6) context đầy → checkpoint + continue; (7) bảng lệnh 12 dòng.
Guard test: mọi `pulse …` trong block và `skills/**` parse được bằng clap.

### 12.2 Docs

- `docs/README.md`: bản đồ tay viết; `pulse init` seed 8 dòng.
- Frontmatter tuỳ chọn trên doc: `applies_to: [glob]`, `tags: []`,
  `generated_by: {argv, check_argv}`.
- `pulse docs applicable <id>` = glob/tag match như learnings; trả path +
  `why` + số dòng. Không index, không search.
- `pulse docs check` = link nội bộ vỡ, `generated_by.check_argv` exit ≠ 0,
  path trong `docs/README.md` không tồn tại. Là lane `check-docs`.

### 12.3 Skill (4)

| Skill | Vào | Ra | Không |
|---|---|---|---|
| `pulse-shape` | yêu cầu mơ hồ | Epic/Story record `draft` với outcome, rules, exceptions, qa_cases, open_questions; glossary | tạo Ticket, code |
| `pulse-plan` | Story `draft`/`ready` | Ticket record chi tiết (4.4) + deps; `pulse ready` từng cái | chạy worker |
| `pulse-review` | Ticket `verifying` (khi không dùng `pulse run`) | `<lane>.json` đúng 8.4 | sửa source |
| `pulse-learn` | Ticket vừa `done` | 0–1 learning `candidate`, 0–1 intervention (check > template > doc > AGENTS) | sửa AGENTS trong Ticket |

Wayfind/grill/spec gộp vào `pulse-shape` (một cuộc phỏng vấn, một câu một
lượt, recommended answer, D-id cho quyết định); research/onboard/handoff là
đoạn trong khối AGENTS. Skill hiện có dùng làm nguyên liệu, không giữ nguyên.

---

## 13. Board

`pulse board`: đọc `issues.jsonl`, `receipts/`, `learnings/`; render một file
HTML tự chứa (CSS/JS inline, không CDN) `.pulse/cache/board.html`:

- Cột theo status Ticket; nhóm theo Story; Epic là filter.
- Drawer Ticket: objective, AC với verdict lane, findings mở, checkpoint mới
  nhất, deps, notes, receipt list, ảnh dưới `evidence/<id>/shots/` (đường dẫn
  file://).
- Tab Learnings và Doctor.
- `--watch`: poll mtime 1s, render lại; trang có `<meta http-equiv=refresh
  content=3>` khi được render với `--watch`.
- `--open`: `open`/`xdg-open`.

Template ở `assets/board/board.html` với placeholder `/*DATA*/`. Không server
trong v3.0. `pulse serve` là LATER với điều kiện: người dùng mở board > 20
lần/ngày và refresh 3s gây phiền.

---

## 14. Kế hoạch thực hiện

Branch `v3/thin-harness` từ `features/harness-experimental`. Mỗi bước dưới
đây là **một commit** với dòng cuối message: `lines: <src trước> -> <src sau>`.
Chạy `cargo fmt --check && cargo clippy --all-targets --quiet -- -D warnings
&& cargo test --all-targets` trước mỗi commit; test của cơ chế bị xoá thì xoá
cùng commit, không `#[ignore]`.

### Phase 0 — chốt (1–2 ngày)

- [ ] P0.1 Ghi baseline vào `docs/plans/0022-metrics.md` (mục 2).
- [x] P0.2 Dogfood target — **chốt 2026-09-16**: repo riêng
      `~/Workspace/Personal/todolist` (chưa tồn tại; tạo ở đầu Phase 2), một
      todolist web: Next.js + TypeScript, Tailwind + shadcn/ui, Zustand +
      TanStack Query; FastAPI + Pydantic, SQLAlchemy + Alembic + PostgreSQL;
      Google OAuth/OIDC; Redis + ARQ; REST + OpenAPI; Docker. Mobile-first,
      minimal productivity. Không dùng `examples/` trong repo Pulse (stack kéo
      theo node_modules/.venv/Docker volume và lỗi `.gitignore` neo gốc, git
      prefix đã gặp ở Track B). `examples/todolist` cũ chỉ còn ở tag
      `dogfood/track-b-final`, không khôi phục.
      Thứ tự Story (tracer bullet, không phải danh sách tính năng):
      ST-1 Task CRUD + Inbox không auth (TK api + TK ui) → ST-2 Today/Upcoming/
      Overdue + due/priority → ST-3 Google OAuth + per-user (risk high) → ST-4
      Subtask/Project/Tag → ST-5 Reminder + recurring (Redis/ARQ) → ST-6
      Search/filter/drag-drop → ST-7 PWA offline. V2 (Calendar sync, Focus
      timer, AI planning) là Epic riêng, chưa shape.
- [ ] P0.3 Viết `docs/decisions/0022-thin-harness.md` từ mục 1–2; status
      accepted; cập nhật `docs/decisions/README.md`; đánh dấu 0009, 0010,
      0012–0021 "superseded in scope by 0022, lessons retained".
- [ ] P0.4 `PRODUCT.md` → thêm banner đầu file: "v3 theo plan 0022; nội dung
      dưới là v2, chỉ còn giá trị lịch sử cho tới khi SPEC.md thay thế".

### Phase 1 — xoá và gộp (≈ 1 tuần)

Số phận từng file hiện tại:

| Fate | File |
|---|---|
| **giữ nguyên/sửa nhỏ** | `storage/{atomic,lock,append,paths,mod}.rs`, `event.rs`, `canonical_json.rs`, `id.rs` (đổi sang hash id), `error.rs`, `identity/*`, `cli/{mod,output,args,events,init}.rs`, `bin/pulse.rs`, `evidence/artifact.rs`, `evidence/redaction.rs` (thu ~80 dòng) |
| **viết lại nhỏ hơn** | `source.rs` (→ ~150), `kernel/init.rs` (→ ~200: `.pulse/`, PULSE.md, AGENTS block, docs/README, .gitignore, hosts), `kernel/packet.rs` (→ ~300), `kernel/completion.rs` (→ ~400: handoff/close/close-story gates), `kernel/run.rs` (→ ~600: worker loop + lane + seal), `kernel/reservation.rs` (→ ~150: lease), `runner/mod.rs` (giữ spawn/timeout/bounded), `cli/work.rs` (→ ~400 với lệnh mới), `cli/run.rs`, `evidence/receipt/{envelope,store}.rs` (→ một `evidence/receipt.rs` ~250) |
| **mới** | `store/issues.rs` (đọc/ghi/validate JSONL, ~300), `schema/issue.schema.json`, `kernel/roles.rs`, `kernel/ready.rs` (~150), `kernel/checkpoint.rs`, `kernel/lane.rs` (validate 8.4 + seal), `kernel/profile.rs` (PULSE.md yaml → cần thêm dep `serde_yaml`), `learn/{mod,store,recall}.rs` (~300), `docs/{applicable,check}.rs` (~250), `board/mod.rs` (~200 + template), `doctor.rs` (~200), `cli/{learn,docs,board,doctor}.rs` |
| **xoá** | `policy/*`, `docs/{applicability,cache,index,lexical,search,section,get,tree,markdown,model,registry,manifest,tags,policy,projection,receipt_validation,validate,check/*}.rs`, `knowledge/*`, `graph/*` toàn bộ (model/brief, validation, read, store — thay bằng `store/issues.rs` + `kernel/ready.rs`), `qa/*` (baseline → `qa_cases` trong schema; receipt → `kernel/lane.rs`; executor → `runner`), `execution.rs`, `reservation.rs` (root), `work_packet.rs`, `kernel/{documentation*,guidance,frontier,readiness,lifecycle,story_completion,communication}.rs` (phần còn dùng gộp vào file mới), `evidence/{model,manifest,receipt/{bindings,decision,documentation,supersession,helpers}}.rs`, `cli/{docs,graph,evidence,knowledge,qa}.rs` |

Thứ tự commit (mỗi dòng một commit, test xanh sau mỗi commit):

- [ ] P1.1 Thêm `store/issues.rs` + schema + `id.rs` hash; test đọc/ghi/validate/lock/atomic; chưa nối CLI.
- [ ] P1.2 Thêm `kernel/roles.rs`, `kernel/ready.rs`; test 6 điều kiện ready.
- [ ] P1.3 `cli/work.rs` mới: `new|show|list|tree|ready|update|dep|transition|note` trên `issues.jsonl`. Xoá `graph/*`, `cli/graph.rs`, tests `graph/*` (giữ `architecture_guards.rs`, `public_api_paths.rs` sửa theo layer mới).
- [ ] P1.4 Xoá `policy/*`, `docs/*` (trừ file sẽ viết mới), `knowledge/*`, `qa/*`, `cli/{docs,knowledge,qa,evidence}.rs`, tests tương ứng. Build phải xanh — kernel tạm thời stub các import (mục tiêu commit này là cây nhỏ, không phải chức năng).
- [ ] P1.5 Viết lại `source.rs`; test: fence bỏ qua `.pulse/**`, `same()` đúng khi chỉ file ignore đổi.
- [ ] P1.6 `evidence/receipt.rs` một họ + redaction thu gọn; migrate không cần (không có repo đích v2 sống); xoá `evidence/execution` code.
- [ ] P1.7 `kernel/completion.rs` mới: handoff gate 7.2, close gate 7.3, close-story 7.4; `pulse handoff|close|close-story`. Test từng mã lỗi.
- [ ] P1.8 `kernel/packet.rs` mới; `kernel/checkpoint.rs`; `pulse packet|checkpoint`.
- [ ] P1.9 `kernel/profile.rs` (PULSE.md), `kernel/lane.rs` (validate + seal 8.4), `kernel/run.rs` mới với vòng continue 10.1 và lane 10.2; xoá worktree code; tests `runner/*` viết lại với fake agent script (giữ pattern `worker_finds_its_run_workspace_under_its_own_cwd` bỏ, thêm `continue_spawns_fresh_process_with_latest_checkpoint`, `continue_without_new_checkpoint_is_a_crash`, `continue_limit_blocks_with_needs_split`, `lane_that_mutates_workspace_gets_no_receipt`, `qa_ui_pass_without_screenshot_is_inconclusive`).
- [ ] P1.10 `kernel/init.rs` mới + assets (`agents-block.md`, `PULSE.md` seed, `docs/README.md` seed, hosts, prompts). Guard test parse lệnh trong block.
- [ ] P1.11 Đo và ghi metrics. Đích sau Phase 1: src < 12k, lệnh ≤ 22, mã lỗi < 60.

### Phase 2 — chạy thật (≈ 1 tuần)

- [ ] P2.1 `learn/*` + `pulse learn *`; `docs/{applicable,check}.rs`; nối vào packet.
- [ ] P2.2 Tạo `~/Workspace/Personal/todolist`: scaffold tối thiểu chạy được
      (`docker compose` với postgres + redis, FastAPI hello, Next.js hello,
      `pnpm`/`uv`), commit đầu. `pulse init --host claude-code --with-qa-templates`;
      viết `docs/README.md`, `PULSE.md` (profile `api-*`, `ui-*`),
      `runners.json`, và **`docs/operations/run.md`** (lệnh start db/redis/api/web,
      ready URL, cách đọc log) — lane `qa-api` đọc file này, thiếu thì
      `inconclusive`. Scaffold là việc của human/agent ngoài Pulse; từ ST-1 trở
      đi mọi thay đổi đi qua Ticket.
- [ ] P2.3 Bằng agent tương tác (không gõ tay): `pulse-shape` → Epic todolist +
      ST-1 "Task CRUD + Inbox, không auth, một user cố định" với qa_cases:
      QA-001 (api: POST rồi GET trả đúng task), QA-002 (ui: tạo task trên Inbox
      thấy xuất hiện, hoàn thành thì gạch) → `pulse-plan` → TK api (FastAPI
      `/tasks` CRUD + Alembic migration đầu) và TK ui (Inbox list + create +
      complete qua TanStack Query), `blocked_by` ui→api, cả hai `ready`.
- [ ] P2.4 Golden path v3 (mục 15) cho TK api rồi TK ui. Ghi mọi friction bằng
      `pulse note --friction`; phân loại `pulse-bug | target-harness | agent`.
      ST-2 trở đi chỉ khi golden path đã xanh; ST-3 (OAuth) là Story đầu tiên
      chạy profile `*-high` với `review-adversarial` và human gate.
- [ ] P2.5 Kill worker giữa chừng → `pulse run worker` resume từ checkpoint; ép `continue` bằng cách đặt `--continue-limit 1` và prompt test.
- [ ] P2.6 Sửa friction loại `pulse-bug`; **không** thêm cơ chế mới. Ghi metrics.

### Phase 3 — vòng học và board (≈ 1 tuần)

- [ ] P3.1 `pulse board`; dùng nó thay `show` trong một ngày; ghi friction.
- [ ] P3.2 `pulse doctor` tối thiểu (11.3 + lỗi store).
- [ ] P3.3 `pulse-learn` chạy trên friction Phase 2; ít nhất một intervention là **check** (role `check-*` trong runners.json của target).
- [ ] P3.4 Thay `PRODUCT.md` bằng `SPEC.md` ≤ 300 dòng mô tả đúng v3 đang chạy; `ARCHITECTURE.md`, `ROADMAP.md`, `AGENTS.md` (repo Pulse) viết lại theo cây mới; `GLOSSARY.md` cắt theo.
- [ ] P3.5 Xoá `skills/pulse-{wayfind,grill,spec,planning}` sau khi `pulse-shape`/`pulse-plan` chạy thật trên P2.3; giữ eval fixture có giá trị.
- [ ] P3.6 Ghi metrics cuối; tag `v3.0.0`.

### Điều kiện dừng

Sau v3.0, **không** bắt đầu mục nào trong danh sách sau nếu không có ≥ 2
friction cùng loại ghi trong `.pulse/events` của dogfood: docs search, worktree
song song, knowledge relation, authority grant, receipt signature, MCP server,
`pulse serve`, reviewer ≥ 2 mặc định, materialization.

---

## 15. Golden path v3 (tiêu chí xong)

Trên dogfood target, bằng agent thật:

1. `pulse init --host claude-code --with-qa-templates`; `pulse doctor` sạch.
2. `pulse-shape` tạo Story `ready` với `rules`, 2 `qa_cases` (ui, api).
3. `pulse-plan` tạo 2 Ticket `ready` (surface api risk medium; surface ui risk medium), có `blocked_by` giữa chúng.
4. `pulse run worker <api>` → worker checkpoint ≥ 1 lần → handoff. `pulse run review <api>` chạy `review-correctness` + `qa-api`; qa-api để lại response + log; `pulse close` thành công; hoặc lane fail → rework → pass.
5. `pulse run worker <ui>` → kill giữa chừng → chạy lại resume từ checkpoint → handoff. Lane `qa-ui` để lại 2 ảnh/case và console log. Close.
6. Sửa một file source sau handoff → `pulse close` từ chối `close_source_stale` nêu đúng path.
7. `pulse close-story` pass. `pulse board` hiện hai Ticket done với ảnh.
8. `pulse-learn` sinh một learning; Ticket thứ ba chạm cùng anchor thấy nó trong packet; handoff ghi `helpful`; `pulse learn activate`.
9. Số đo mục 2 đạt.

---

## 16. Quyết định còn mở (mặc định nếu không ai phản đối)

1. **YAML cho PULSE.md** — mặc định có (thêm `serde_yaml`); phương án B là
   JSON `PULSE.json` để không thêm dep. Chọn YAML vì human sửa tay.
2. **Một `issues.jsonl` hay tách theo kind** — mặc định một file. Tách khi
   file > 2 MB hoặc merge conflict > 1 lần/tuần.
3. **Ảnh evidence track git hay không** — mặc định track (PNG nhỏ), video
   không. Xem lại khi repo > 200 MB.
4. **`worker-continue` khác vendor mặc định?** — mặc định cùng `worker`;
   khác vendor là quyết định của repo đích trong `runners.json`.
5. **Lane agent viết `<lane>.json` hay in JSON ra stdout** — mặc định file
   (bền hơn khi stdout bị agent làm bẩn); stdout chỉ `{"status":"done"}`.

---

## 17. Nguồn

Audit 2026-09-16 (`pulse-thin-harness-audit.html`); `docs/dogfood-friction-track-b.md`;
OpenAI *Harness engineering* (02/2026); Anthropic *Effective harnesses for
long-running agents* (11/2025) và *Managed Agents*; GitHub spec-kit / Kiro
(EARS, requirements → design → tasks); Every *Compound Engineering* và phản
biện *Make It a Check*; Playwright *Coding agents*; AgentLens (arXiv
2607.06624); repository-harness `improve-harness`; beads_rust (JSONL, hash id,
`ready`); Claude Code statusline docs và issues #27969/#34340/#43431.
