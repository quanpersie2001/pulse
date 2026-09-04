# Pulse — Product Definition

> Trạng thái: chốt ngày 2026-09-05. Đây là nguồn sự thật về sản phẩm và thiết kế
> mục tiêu. Nó thay thế toàn bộ `pulse-reboot/` (đã xoá, còn trong Git history
> trước commit này). Khi README, AGENTS.md hay `proposals/` mâu thuẫn với file
> này, file này thắng cho đến khi có ADR thay thế.

---

## 1. Định nghĩa

Pulse là một **CLI local cho developer dùng coding agent trong một repository**.
Nó giữ sự thật về việc cần làm, cấp đúng context cho agent, gọi agent hay script
qua lệnh cấu hình, chỉ cho đóng việc khi có bằng chứng, và biến mỗi lần fail
thành harness tốt hơn.

Pulse **không chạy agent, không chạy test, không có daemon**. Trạng thái nằm
trong `.pulse/`, `works/` và `docs/` dưới Git.

Luận điểm:

> Agent chọn đúng việc, lấy đúng context, tạo proof đáng tin, và repository dễ
> vận hành hơn sau mỗi lần chạy. Correct completion với ít can thiệp của human
> hơn, trong khi work truth, authority và evidence vẫn local, inspectable và
> recoverable.

Trục thiết kế:

```text
workflow-first  -> capability-first
phase artifacts -> risk-adaptive evidence
agent memory    -> repository-legible context
task list       -> local work graph
success claims  -> executable proof
agent runtime   -> vendor's job, không phải của Pulse
```

## 2. Người dùng và phạm vi

Một developer, một repo tại một thời điểm, một hoặc nhiều agent chạy dưới sự
giám sát của developer. Agent là bất kỳ thứ gì chạy được từ shell: Claude Code,
Codex, Aider, OpenCode, một script.

Không phải: team nhiều người, fleet không giám sát, dịch vụ cloud, Jira, workflow
engine, agent framework, QA platform, orchestration engine.

## 3. Nguyên tắc

1. **Capability before ceremony.** Mỗi bước phải tăng khả năng, độ tin cậy hoặc
   khả năng phục hồi. Bước không làm được điều đó là ceremony và bị bỏ.
2. **Evidence over assertion.** `done` là kết quả của gate đọc receipt, không
   phải câu báo cáo. Process exit, message "xong", session closed không có nghĩa.
3. **Người verify khác người làm.** Cùng một actor không được vừa handoff vừa
   verify, vừa làm vừa đóng.
4. **One writable source per truth.** Docs, work graph, work prose, evidence,
   knowledge, config là sáu plane riêng; không plane nào ghi đè plane khác.
5. **Deterministic mechanism, agent judgment.** Pulse validate schema, hash,
   revision, grant, gate. Pulse không phán đoán ngữ nghĩa.
6. **Progressive disclosure.** Root là map; chi tiết và docs áp dụng chỉ nạp khi
   cần. Không agent nào phải đọc toàn bộ `docs/`.
7. **Risk-adaptive materialization.** Việc nhỏ cần ít artifact; việc rủi ro cần
   nhiều artifact và proof sâu hơn. Không phase cố định.
8. **Resolve critical ambiguity before execution.** Ticket chưa `ready` khi agent
   còn phải đoán objective, acceptance, scope, invariant hoặc một lựa chọn khó
   đảo ngược.
9. **Đổi agent là đổi một dòng config.** Seam với agent là shell command và
   file, không phải protocol riêng.
10. **Làm trực tiếp trong checkout.** Worktree chỉ khi chạy song song.
11. **Human authority explicit.** Automation có quyền theo grant cụ thể, không
    theo vai trò mơ hồ. Không wildcard.
12. **Failure feeds the ratchet.** Mỗi lớp failure phải có đường thành docs,
    check, policy hoặc eval. Learning phải retrievable, không chỉ stored.
13. **Docs hiện trạng và docs tương lai ở hai chỗ.** Không tài liệu nào mô tả
    feature chưa có code như đã có.
14. **Không thêm feature khi golden path chưa chạy thật.**

## 4. Sáu plane dữ liệu và layout

| Plane | Trả lời câu hỏi | Writable truth |
|---|---|---|
| Durable docs | Repo hiện được hiểu như thế nào? | `docs/`, `AGENTS.md`, `PULSE.md` |
| Work prose | Thay đổi nào đang được đề xuất/thực hiện? | `works/<id>/` |
| Work graph | Work item, relation, lifecycle ở trạng thái nào? | `.pulse/workgraph/` |
| Evidence | Điều gì đã được chứng minh trên snapshot nào? | `.pulse/evidence/` |
| Knowledge | Future work nên biết gì khi trigger tương tự xuất hiện? | `.pulse/knowledge/` |
| Config | Agent nào, policy nào, tag nào? | `.pulse/config/`, `.pulse/policy/` |

Event log `.pulse/events/` là audit trail bất biến cho mọi plane. `.pulse/cache/`
luôn gitignored, xoá được, không cần cho correctness.

```text
AGENTS.md                      # repository map, ngắn, route tới owner
PULSE.md                       # intent, risk policy, verification profiles, human gates

docs/
  product/                     # user/system-visible behavior contract
  architecture/                # boundaries, dependency direction, invariants
  domain/                      # glossary, rules, state machines, error taxonomy
  operations/                  # setup, deploy, recovery, runbooks
  reference/                   # authored API/config reference
  generated/                   # projection từ code, không hand-edit
  decisions/                   # ADR projection (optional)
  _index.md                    # generated navigation, không phải truth

works/
  EP-001/  brief.md design.md
  ST-014/  story.md approach.md qa.md
  TK-031/  ticket.md plan.md validation.md
  DEC-006/ decision.md

.pulse/
  workgraph/
    manifest.json
    nodes/   EP-001.json ST-014.json TK-031.json DEC-006.json
    edges/   parent--TK-031--ST-014.json blocked-by--TK-031--TK-029.json
  docs/
    registry.json
    tags.json
  evidence/
    receipts/<ulid>.json
    artifacts/sha256/<hash>
  knowledge/
    entries/LRN-001.json
    relations/
  events/<date>/<ulid>.json
  policy/authority.json
  config/runners.json
  runtime/                     # lock, transaction intent, lease TTL; gitignored
  cache/                       # index, snapshot; gitignored
```

Git ownership: `workgraph`, `docs/registry.json`, `tags.json`, `events`,
`evidence/receipts`, `knowledge`, `policy`, `config` là tracked. `works/` và
`docs/` là tracked. `runtime/`, `cache/` gitignored. Artifact lớn theo retention
policy.

## 5. Tính năng

### 5.1 Work graph: chia việc và giữ việc

#### Node kinds

```text
Epic: outcome lớn và ranh giới đầu tư
  -> Story: lát cắt hành vi có thể chứng minh, owner của QA baseline
       -> Ticket: đơn vị một agent thực thi và verify

Decision: lựa chọn khó đảo ngược, liên kết vào bất kỳ node nào
```

Hierarchy là tuỳ chọn. Ticket độc lập hợp lệ. Dependency, ordering, supersession
là typed edge riêng, không suy từ parent-child.

| Kind | Giữ gì | Prose |
|---|---|---|
| Epic | Outcome, success signals, scope boundary, constraints, risks | `brief.md`, `design.md` |
| Story | User/system outcome, acceptance hành vi, shared approach, QA baseline, close gate | `story.md`, `approach.md`, `qa.md` |
| Ticket | Executable contract, plan, validation | `ticket.md`, `plan.md`, `validation.md` |
| Decision | Context, options, decision, consequences, supersession | `decision.md` |

Ticket có hai role: `implementation` (đổi repo theo contract) và `decision_work`
(trả lời một câu hỏi cụ thể: research, spike, prototype). Decision-work Ticket
không cần anchors/invariants và có thể chạy từ `draft` khi câu hỏi đã precise.

#### Storage

Sharded JSON: một file mỗi node, một file mỗi edge. Không `graph.json` writable,
không SQLite. Lý do: diff nhỏ, merge tốt, hai agent sửa hai Ticket không chạm
cùng file, human inspect bằng Git.

`manifest.json` chỉ giữ contract hiếm đổi (schema version, id pattern, content
root), không giữ counter. Full graph là derived projection có fingerprint từ
sorted content hashes, cache được và rebuild được.

#### Node contract

```json
{
  "schema_version": 1,
  "id": "TK-031",
  "kind": "ticket",
  "role": "implementation",
  "title": "Phân loại lỗi refresh token",
  "status": "ready",
  "revision": 9,
  "contract_revision": 4,
  "priority": "P1",
  "risk": "medium",
  "materialization": "R1",
  "content_dir": "works/TK-031",
  "brief_hash": "sha256:...",
  "qa":   {"posture": "required", "owner": "ST-014", "cases": ["QA-001", "QA-004"]},
  "docs": {"posture": "required", "documents": ["DOC-AUTH-DOMAIN"]},
  "tags": ["security", "api-contract"],
  "created_at": "...", "updated_at": "..."
}
```

- `revision`: CAS cho mọi mutation. Ghi phải đúng revision hiện tại.
- `contract_revision`: chỉ tăng khi semantic input đổi (ticket.md, risk, QA/docs
  posture, required Decision). Status/timestamp chỉ tăng `revision`. Nhờ vậy
  receipt bind `contract_revision` không stale khi status đổi.
- `brief_hash`: hash của `ticket.md`. Ticket.md là nguồn của contract; node
  không chứa prose.
- Node không embed children, inverse relation, readiness boolean. Derive từ
  edge/evidence/policy.
- `risk` và `materialization` có thể `unassessed`. Public Ticket create yêu cầu
  khai `risk`; `materialization` suy từ risk nếu không khai.

#### Edge contract

| Type | from → to | Nghĩa | Rule |
|---|---|---|---|
| `parent` | child → parent | hierarchy | tối đa một live parent, không cycle |
| `blocked_by` | dependent → blocker | hard dependency | không cycle; blocker terminal mới executable |
| `preferred_after` | sau → trước | soft ordering | không chặn; chỉ ảnh hưởng gợi ý thứ tự |
| `superseded_by` | cũ → hấp thụ | outcome bị hấp thụ | không cycle |
| `related` | id nhỏ → id lớn | context | symmetric, canonicalize |

ID edge deterministic từ `(type, from, to)`, nên `edge add` idempotent. Dangling
edge làm `graph validate` fail. Reverse edge chỉ là projection.

#### Lifecycle

```text
draft -> shaped -> ready -> active -> verifying -> done
                              |          |
                              v          v
                            blocked    rework -> active

draft/shaped/ready/blocked -> cancelled
any non-terminal -> superseded
```

- `draft -> shaped -> ready`: qua ready gate (mục 5.1 Ready gate). Không `--force`.
- `blocked -> shaped`, không `blocked -> ready` trực tiếp.
- `ready -> active`: chỉ qua lease (`pulse run` hoặc `work claim`).
- `active -> verifying`: chỉ qua handoff receipt.
- `verifying -> done|rework|blocked`: chỉ qua close gate với verification receipt.
- `done` do gate tính. Worker không tự khai.
- Story không nhận lease; Story `ready -> done` qua Story close gate riêng.
- `superseded` có `superseded_by` edge hoặc Decision giải thích. Khác `done`:
  outcome bị hấp thụ, không hoàn thành độc lập. Giữ lịch sử.

#### Mutation protocol

- Mọi mutation qua CLI. Agent không hand-edit graph file.
- Expected revision bắt buộc cho update. Ghi thành công tăng revision.
- Validate schema, referential integrity, cycle trước atomic rename.
- Một immutable event sau mỗi mutation thành công.
- Repository-scoped lock, temp file, fsync, atomic rename. Transaction intent
  ghi trước, commit sau, recover khi crash: rollback hoặc resume-forward, không
  đoán.
- Worker trong worktree không sửa canonical graph; nó gọi CLI, CLI ghi vào
  repo chính dưới lock.

#### Nội dung Ticket: `works/TK-031/ticket.md`

Đây là contract thật của Ticket. Node chỉ giữ hash của nó.

```markdown
# TK-031 Phân loại lỗi refresh token

## Objective
Phân biệt token hết hạn với token không hợp lệ để client xử lý đúng.

## Current behavior
`RefreshTokenHandler` map cả hai trường hợp thành `InvalidToken`.

## Target behavior
- Token hết hạn thành `TokenExpired`.
- Token giả mạo/revoked vẫn là `InvalidToken`.

## Code anchors
- `src/auth/RefreshTokenHandler.ts`
- `src/auth/errors.ts`
- `tests/auth/refresh-token.test.ts`

## Required changes
- Bổ sung domain error cho token hết hạn.
- Giữ nguyên public response envelope.
- Thêm contract tests cho expired và tampered token.

## Invariants
- Không trả chi tiết xác thực nhạy cảm.
- Không thay đổi refresh-token rotation.

## Implementation freedom
guided: agent chọn internal structure, không đổi public contract.

## Scope
- Domain error, HTTP mapping và contract tests.

## Non-scope
- UI đăng nhập.

## Acceptance
- AC-1: Hai failure modes có mã lỗi ổn định.
- AC-2: Response không leak thông tin nội bộ.

## Verify
- `pnpm test auth`

## Open questions
- (resolved) Có giữ generic `InvalidToken` cho client cũ không? Có, xem DEC-006.
- (delegated) Tên internal error class: worker chọn.

## Documentation impact
- Posture: required
- Documents: DOC-AUTH-DOMAIN
- Required update: ghi failure taxonomy mới.

## QA impact
- Owner: ST-014
- Posture: required
- Cases: QA-001, QA-004
- Reason: đổi public error mapping.

## Expected handoff
- Diff, kết quả `pnpm test auth`, mapping AC → check, docs finding.
```

Pulse parse các heading quy ước để lấy trường máy đọc: Acceptance ID, Code
anchors, Verify commands, Documentation impact, QA impact, Open questions
disposition. Prose còn lại là cho agent và human. Sửa file là tăng
`contract_revision`.

Implementation mode trong `Implementation freedom`:

- `locked`: theo Decision/approach đã khoá, lệch phải xin Decision.
- `guided`: anchors, required changes, invariants rõ; agent chọn chi tiết.
- `open`: agent chọn approach trong boundary; nếu uncertainty có thể đổi
  objective/acceptance/invariant, shape thành decision-work Ticket hoặc Decision
  trước.

`plan.md`: worker viết sau khi đọc checkout, chỉ khi `materialization >= R2`
hoặc worker muốn. `validation.md`: worker viết lúc handoff, ghi đã verify gì,
kết quả, rủi ro còn lại.

#### Progressive materialization

| Mức | Dùng khi | Artifact bắt buộc |
|---|---|---|
| R0 | Việc nhỏ, risk low, direction rõ | Node + ticket.md chỉ cần Objective, Acceptance, Code anchors, Verify |
| R1 | Thay đổi thông thường | ticket.md đầy đủ, validation.md |
| R2 | Cross-module, nhiều session, ambiguity cao | Thêm plan.md, Story approach.md, verify độc lập |
| R3 | Architecture, migration, security, destructive | Thêm Decision, rollback plan, QA sâu, human gate |

Materialization có thể nâng khi agent phát hiện risk. Hạ xuống phải có lý do
ghi lại. Không tạo artifact chỉ để thoả ceremony.

#### Ambiguity gate (shaping)

Trước `ready`, mọi câu hỏi có thể đổi objective, acceptance, invariant, public
contract hoặc hướng khó đảo ngược phải có disposition trong `Open questions`:

| Disposition | Nghĩa | Điều kiện |
|---|---|---|
| `resolved` | đã chọn | có lựa chọn và lý do, hoặc link Decision |
| `rejected` | đã xem xét, loại | lý do đủ để không mở lại |
| `delegated` | worker được chọn | nằm trong implementation freedom |
| `deferred` | chưa cần trong scope này | có owner và trigger hoặc linked Ticket |
| `blocking` | chưa dispatch được | Ticket không `ready` |

Cách shape: đọc repo và docs trước khi hỏi human. Chỉ hỏi human về intent,
preference, authority, trade-off mà evidence không trả lời được. Hỏi từng câu,
kèm recommended answer. Việc R0 chỉ cần self-check ngắn. Persisted shaping map,
decision frontier, fog-of-war là **Later**.

#### Ready gate

Implementation Ticket `ready` khi:

- Objective, target behavior, acceptance không mâu thuẫn; acceptance có ID.
- Code anchors đủ để orient (hoặc là decision-work Ticket).
- Không open question ở `blocking` hoặc chưa disposition.
- Hard blocker đều terminal.
- Required Decision tồn tại và đã accepted.
- QA posture không `unknown`; `required` thì owner Story và case ID resolve
  được; `none` có rationale.
- Docs posture không `unknown`; `required` thì document ID tồn tại; `none` có
  rationale.
- Mọi reference (docs, Decision, Story) tồn tại; graph valid.

Readiness là derived report với fingerprint hẹp trên input liên quan. `ready`
nhưng input đổi là `ready_stale`: bị loại khỏi `work ready` và không tự đổi
status.

#### Query surface

Agent không grep graph file. CLI là query surface:

```text
pulse work list [--status] [--kind] [--role] [--tag] --json
pulse work show <id> --json
pulse work ready --json                 # execution-ready Tickets, không stale
pulse work packet <id> --json
pulse work rollup <story|epic> --json   # status counts, open blockers
pulse graph neighborhood <id> --depth 2
pulse graph affected-by <id>
pulse graph validate
pulse graph export --json               # derived, cache được
```

#### Priority và reconciliation

`priority` P0–P3 là urgency, không phải thứ tự dispatch. Chọn việc tiếp theo
là judgment của developer hoặc conductor agent, dựa trên: hard dependency, soft
ordering, foundation value (việc nhỏ unlock nhiều việc), supersession, cost of
delay, risk. Pulse cung cấp `work ready` và `preferred_after`; không tính hidden
score. Supersession: so acceptance, ghi `superseded_by`, chuyển acceptance chưa
cover sang node mới, giữ lịch sử. Debate hai agent và reconciliation receipt là
**Later**.

### 5.2 Packet: cấp đúng context cho một Ticket

`pulse work packet <id>` trả một JSON bounded, là thứ duy nhất agent cần đọc
trước khi làm:

- Ticket node và `contract_revision`.
- Nguyên văn `ticket.md`, và `plan.md` nếu có.
- Parent Story/Epic summary; Story `approach.md` khi có.
- Applicable Decisions: id, title, decision, consequence.
- Blocker states; related work.
- Docs: `required` (id, path, summary, section refs, content hash, reason),
  `suggested` (top section hits có score và reason), `write_candidates`,
  `excluded` với lý do. Kèm `read_budget` gợi ý, ví dụ 4 sections/240 dòng.
- QA: posture, affected cases nguyên văn từ `qa.md`.
- Knowledge: learning `required` và `recommended` với summary, why_applicable,
  required_checks, detail ref.
- Notes nhắm tới Ticket; rework observation nếu có.
- Source commit, dirty state.
- `tags_vocabulary`.
- Handoff protocol: lệnh cần gọi khi xong.

Packet không inline toàn bộ docs; nó trả refs và snippet, agent `docs get` khi
cần. Packet fence theo source commit và fingerprint của mọi input: code đổi thì
packet cũ hết hiệu lực và `pulse run` từ chối dùng packet stale.

Trước lease là preview; trong `pulse run`, packet được commit cùng lease và
không tự rebuild từ revision mới. Contract đổi giữa chừng tạo finding
`contract_drift`; worker phải acknowledge hoặc handoff rồi dừng.

### 5.3 Runner: gọi agent hay script bất kỳ

#### Config

```json
// .pulse/config/runners.json
{
  "worker":   {"command": "claude -p --output-format json --input-file {input}", "timeout_seconds": 3600},
  "reviewer": {"command": "codex exec --json --input {input}", "timeout_seconds": 1800},
  "qa":       {"command": "node scripts/qa-run.mjs {input}", "timeout_seconds": 900},
  "check":    {"command": "npm run docs:check", "timeout_seconds": 300}
}
```

Role là tên tuỳ ý. Placeholder: `{input}` đường dẫn file input, `{ticket}`,
`{repo}`, `{artifact_dir}`. Không shell interpolation; args parse bằng argv
parser, không qua `sh -c`.

#### Luồng `pulse run <role> --ticket <id>`

1. Kiểm tra Ticket `ready` (worker) hoặc `verifying` (reviewer, qa). Từ chối
   nếu stale.
2. Lấy lease cho actor `runner:<role>`. Một Ticket một lease. Ticket sang
   `active` với worker.
3. Quyết định isolation (xem dưới).
4. Ghi packet hoặc input ra `.pulse/runtime/run/<ticket>/<role>-input.json`.
5. Spawn lệnh với timeout, bounded stdout/stderr, process group để cancel được.
6. Đọc stdout cuối cùng là JSON theo contract của role. Non-zero exit,
   timeout, output malformed thì ghi receipt `inconclusive`, không đoán.
7. Hash artifact khai báo, copy vào `.pulse/evidence/artifacts/sha256/`.
8. Ghi receipt tương ứng role và event. Thả process, giữ hoặc thả lease theo
   role.

Agent trong lúc chạy vẫn gọi CLI trực tiếp: `pulse work handoff`, `pulse note`,
`pulse docs get`, `pulse knowledge get`. Output JSON cuối chỉ là tóm tắt.

Bootstrap prompt cho agent chỉ mô tả workflow và identity: "packet ở file X,
đọc required docs bằng `pulse docs get`, khi xong gọi `pulse work handoff`,
không tự đổi acceptance". Không copy Ticket, docs, QA, knowledge vào prompt.

#### Isolation rule

- Mặc định chạy trực tiếp trong checkout.
- Khi đã có Ticket `active` khác mà gọi `pulse run` cho Ticket mới, Pulse tạo
  worktree cho Ticket mới, hoặc từ chối nếu `auto_isolation: false`.
- `--isolation worktree` để ép. Lease theo Ticket, không theo worktree.
- Worktree do Pulse tạo thì Pulse dọn khi Ticket terminal và không còn
  reference. Không xoá thứ Pulse không tạo.
- Reviewer/QA trên cùng checkout chạy tuần tự sau worker xong.

#### Contract input/output theo role

Worker input = packet. Worker output:

```json
{"status": "handed_off", "handoff_receipt": "01JX…H1", "summary": "…", "blockers": []}
```

hoặc `{"status": "blocked", "reason": "…", "decision_request": "…"}`.

QA input:

```json
{
  "ticket_id": "TK-031", "story_id": "ST-014",
  "source_commit": "d4e5f6", "baseline_hash": "sha256:…",
  "cases": [ {"id": "QA-001", "intent": "…", "steps": [], "expected": "…", "surface": "api"} ],
  "artifact_dir": ".pulse/runtime/run/TK-031/qa-artifacts"
}
```

QA output:

```json
{
  "cases": [ {"id": "QA-001", "status": "passed|failed|inconclusive|not_applicable", "observation": "…"} ],
  "artifacts": [ {"path": "…", "role": "log|screenshot|trace", "case_id": "QA-001"} ],
  "findings": [ {"case_id": "QA-004", "summary": "…", "severity": "high"} ]
}
```

Reviewer output: `{"disposition": "pass|rework", "acceptance": {"AC-1": {"check": "pnpm test"}}, "findings": []}`.

Check output: `{"status": "passed|failed", "findings": []}`.

Pulse không biết bên trong là vitest, Playwright, curl hay một agent.

#### Crash và recovery

Lease có TTL ghi trong `.pulse/runtime/`. Agent chết giữa chừng: lease còn,
`pulse run` lại cùng Ticket resume với cùng packet nếu source chưa đổi. Lease
hết TTL mà không handoff: `pulse work release <id>` đưa Ticket về `ready`, ghi
event. Không blind retry. Không có receipt thì không có gì được coi là xong.

### 5.4 Docs: doc first

#### Taxonomy và owner

| Kind | Path | Mục đích | Không dùng cho |
|---|---|---|---|
| map | `AGENTS.md` | entrypoints, commands, docs map, boundaries | knowledge dump |
| policy | `PULSE.md` | risk policy, protected areas, verification profiles, human gates | product design |
| product | `docs/product/` | user/system-visible behavior, compatibility | plan, changelog |
| architecture | `docs/architecture/` | boundaries, dependency direction, invariants | tutorial |
| domain | `docs/domain/` | glossary, rules, state machines, error taxonomy | implementation detail |
| operations | `docs/operations/` | setup, deploy, recovery, runbook | design rationale |
| reference | `docs/reference/` | authored API/config reference | |
| generated | `docs/generated/` | projection từ code; không hand-edit | semantic rationale |

Decision node là durable reasoning ("vì sao chọn"); docs là current state
("hiện hoạt động thế nào"). Work prose, evidence, runtime không phải docs truth.

Source hierarchy khi mâu thuẫn: accepted Decision và product contract = intent;
code và test = implementation; receipt = observation; docs = explanation. Pulse
không tự chọn bên nào; mâu thuẫn ảnh hưởng acceptance/safety làm gate fail với
finding `docs_conflict`, human quyết định code sai hay docs cũ.

#### Registry

Sidecar `.pulse/docs/registry.json`. Chỉ đăng ký docs có vai trò contract,
ownership, generated freshness hoặc routing; không bắt mọi markdown có metadata.
Content vẫn là file Git thường, không front matter bắt buộc.

| Trường | Ý nghĩa |
|---|---|
| `id` | định danh ổn định, không phụ thuộc path, ví dụ `DOC-AUTH-DOMAIN` |
| `path` | repository-relative, không traversal/symlink escape |
| `summary` | một đến hai câu, dùng cho index, search, packet |
| `owner` | human/team/role |
| `kind` | policy, architecture, domain, product, operations, reference, generated |
| `status` | approved, draft, stale, retired |
| `scope.paths` | glob code path; tín hiệu routing chính |
| `tags` | từ vựng kiểm soát, tín hiệu routing phụ |
| `generated` | `{command, freshness_check}` nếu sinh tự động |
| `superseded_by` | id thay thế |

`pulse docs register --path --summary --scope-paths [--tag]`, còn lại default.
Duplicate id/path làm validate fail. Path rename giữ id. Retired/superseded
không route.

#### Tags

Một trường `tags` duy nhất. Từ vựng khai báo tại `.pulse/docs/tags.json`, khoảng
10 đến 20 tag như `security`, `api-contract`, `migration`, `performance`. Tag
ngoài danh sách bị từ chối; thêm tag là mutation có chủ ý. Ticket dùng cùng từ
vựng. Packet ghi kèm danh sách tag hợp lệ.

#### Applicability

Docs áp dụng cho Ticket khi `code_anchors` giao `scope.paths` **hoặc** tag giao
nhau **hoặc** Ticket reference rõ. Path là chính vì tự đúng khi code đổi; tag
bắt mối quan tâm cắt ngang mà path không thể hiện.

Packet phân loại: `required` (phải đọc trước khi sửa vùng liên quan), `optional`,
`write_candidates` (có thể phải cập nhật), `excluded` (stale, retired, generated
navigation, backup) kèm lý do. Registry thiếu docs áp dụng là `docs_context_gap`
để ratchet.

#### Ticket documentation impact

Mỗi implementation Ticket có posture trong `ticket.md`:

- `required`: docs phải đổi trong Ticket này, kèm document ID.
- `none`: không ảnh hưởng, kèm rationale.
- `deferred`: có linked follow-up Ticket và policy cho phép. Không defer cho
  public API, security, destructive migration, runbook.

Ready gate: posture không `unknown`; public behavior/API đổi mà không có
product docs assessment thì không ready. Close gate: `required` cần receipt
`docs_validation` cho đúng document id trên đúng commit; `none` rationale còn
đúng sau diff.

#### Validation

`pulse docs validate [--record]`: mechanical, read-only trừ khi `--record`:

- registry schema, duplicate id/path, dangling reference;
- broken internal link;
- generated docs stale (chạy `freshness_check` bằng argv, không shell);
- retired docs vẫn được link như current;
- `_index.md` lệch projection;
- `AGENTS.md` route tới path không tồn tại.

`--record` ghi receipt `docs_validation` bind source commit và content hash từng
document. Semantic review (docs có mô tả đúng behavior không) là việc của
reviewer agent hoặc human, ghi vào verification receipt.

Generated contract: `sources`, `command`, `outputs`, `editable: false`,
`freshness_check`. Source và generator là truth, output là projection.

#### Retrieval

Progressive disclosure:

```text
AGENTS.md / docs tree
  -> pulse docs search <query>      ranked section, snippet, không full body
  -> pulse docs get <section-ref>   bounded section, line range, content hash
  -> --full chỉ khi thật sự cần
```

- Retrieval unit là markdown section (ATX heading), có `section_ref =
  DOC-ID#anchor`, line range, content hash. Section quá lớn split ở nested
  heading rồi paragraph.
- Engine: BM25+ pure-Rust (tantivy), offline, không model. Field boost:
  heading > title > summary > tags > path > body. Boost nhẹ khi trùng tag hay
  scope Ticket; không để metadata đè strong lexical match.
- Cache `.pulse/cache/docs-search/` gitignored, key theo content hash, atomic
  replace, rebuild deterministic, incremental khi đổi. Search tự refresh cache
  stale.
- Exclude mặc định: retired, backup, generated navigation. `--include-draft`,
  `--include-stale` rõ ràng.
- `_index.md` generated từ registry summary, có marker, không hand-edit.
- Default: 8 hits, snippet 3–6 dòng, `get` một section. Full document là
  opt-in.
- Content docs là untrusted text: snippet không mang authority ngoài registry.

Eval harness recall@k, bench p95, semantic/hybrid adapter là **Later**.

#### CLI

```text
pulse docs register|edit|retire|supersede
pulse docs list|show|tree
pulse docs applicable --work <id>
pulse docs search <query> [--tag] [--kind] [--work <id>]
pulse docs get <doc-or-section-ref> [--full]
pulse docs index [--check]
pulse docs validate [--record]
pulse docs impact <ticket-id>
```

#### Brownfield

Trước khi restructure docs có sẵn: scan, đề xuất phân loại và owner, human
approve, snapshot vào `.pulse/migrations/docs-backups/<id>/` với manifest, rồi
mới register và cập nhật AGENTS.md. Backup không route, không xoá tự động.
Pulse không tự move/merge docs có semantic ambiguity.

### 5.5 Evidence gate: đóng việc bằng bằng chứng

#### Receipt

Receipt là JSON bất biến trong `.pulse/evidence/receipts/<ulid>.json`, hash
theo canonical JSON, ghi kèm event `evidence.receipt.recorded`. Ghi lại cùng
nội dung là idempotent; khác nội dung cùng id là conflict. Không có chữ ký,
không hash chain: với local single-writer, content hash + event log là đủ.

Envelope: `id`, `kind`, `subject` (work id + contract_revision), `actor`,
`source_commit`, `recorded_at`, `payload`, `artifacts[] {role, sha256}`.

| Kind | Ai ghi | Chứng minh gì |
|---|---|---|
| `handoff` | worker | đã làm gì, changed files, checks đã chạy, acceptance → check mapping, remaining risk, docs finding, learning candidate |
| `verification` | reviewer khác worker | acceptance → check/receipt mapping, disposition pass/rework, findings |
| `qa_checkpoint` | qa runner | case nào pass/fail trên baseline hash nào, artifact |
| `docs_validation` | `docs validate --record` | document id + content hash pass mechanical checks |
| `decision_acceptance` | human có grant | Decision id + content hash được accept |
| `close` | close gate | Ticket/Story đóng với receipt nào, graph fingerprint |

Validity chung: schema đúng, subject tồn tại đúng `contract_revision`, source
commit khớp target hoặc ancestor policy cho phép, artifact hash khớp, actor có
grant, actor độc lập khi gate yêu cầu. Receipt cũ không bị sửa khi hết hiệu lực;
gate chỉ không dùng nó nữa.

#### Verification profiles và review layers

Profile nằm trong `PULSE.md` của repo, không hard-code trong Pulse:

```yaml
profiles:
  docs-only:         {commands: ["npm run lint:docs"], review: light}
  service-change:    {commands: ["npm run lint", "npm test"], review: standard}
  web-behavior:      {commands: ["npm test"], qa: required, review: standard}
  migration:         {commands: ["npm run test:migrations"], review: independent, rollback: required}
```

Layers, chọn theo risk: self-check (worker) → mechanical (lint, test, check
runner) → independent review (reviewer khác worker) → QA checkpoint → human
gate (security, destructive, production). Không bắt mọi Ticket qua mọi layer.

#### QA

QA trả lời: "hành vi Story hứa có còn đúng trên snapshot này, và ai chứng minh?"
Nó khác developer verification (Ticket có thoả contract kỹ thuật không). Cùng
một test có thể phục vụ cả hai; khác nhau ở intent, actor, source binding và
receipt.

**Baseline** `works/<STORY>/qa.md`: một fenced block `pulse-qa` cho máy, prose
ngoài cho người. Hash toàn file bind vào receipt.

```json
{
  "story_id": "ST-014", "revision": 3,
  "scope": "Refresh token failure contract",
  "risks": ["RISK-LEAK"],
  "cases": [
    {"id": "QA-001", "intent": "Token hết hạn trả TokenExpired",
     "steps": ["…"], "expected": "…", "surface": "api",
     "priority": "high", "risk_refs": ["RISK-LEAK"]}
  ],
  "exit_criteria": ["Mọi case high pass trên candidate source"]
}
```

Case nói hành vi, không khoá selector hay implementation detail. Case
`not_applicable` cần lý do.

**Story QA posture**: `automated`, `hybrid`, `manual_structured`, `static_proof`,
`not_applicable` (hiếm, cần lý do). Manual vẫn phải có action log, expected
vs actual, screenshot, actor.

**Ticket QA impact** trong `ticket.md`: `required` (case IDs), `none` (rationale),
`covered_by_story_close` (Story sẽ cover, có rationale). Ready gate chặn
`unknown`. Worker được đề xuất case mới, không tự đổi expected behavior.

**Hai scope trên cùng baseline**:

- Ticket checkpoint: affected cases sau một Ticket. Receipt `qa_scope:
  ticket_checkpoint`.
- Story qualification: toàn bộ required cases trên candidate tích hợp trước khi
  Story đóng. Receipt `qa_scope: story_close`.

**Kết quả** mỗi case: `passed`, `failed` (product), `test_failure`,
`environment_failure`, `inconclusive`, `not_applicable`. Chỉ `failed` tự requeue
worker; loại khác tạo harness Ticket nhưng vẫn chặn close vì chưa có proof.

**Retry**: mọi attempt giữ receipt riêng. Pass sau fail trên cùng source là
`flaky`, chặn close cho đến khi human waive với lý do ghi vào close receipt
hoặc root cause được sửa. Không có waiver grant riêng; developer là authority.

**Executor** là runner role `qa`. Pulse cung cấp input/output contract, timeout,
artifact ingest, receipt. Repo sở hữu script hoặc agent chạy Playwright, HTTP,
CLI, data assertion. Environment start/stop, deployment identity, trace
validation là việc của script.

#### Close gate Ticket

`pulse work close <id>` từ chối với lý do cụ thể nếu thiếu:

1. Ticket `verifying`, revision khớp.
2. Receipt `handoff` trên commit hiện tại.
3. Receipt `verification` pass, actor khác actor handoff.
4. Mọi acceptance ID map tới check pass hoặc receipt.
5. QA `required`: receipt `qa_checkpoint` passed, đúng baseline hash, đủ case,
   đúng commit, actor khác worker. `covered_by_story_close`: Story qualification
   hiện hành. `none`: rationale.
6. Docs `required`: receipt `docs_validation` cho đúng document trên commit
   này. `none`: rationale còn đúng.
7. Không open finding blocking; không `flaky` chưa waive.
8. Docs promotion candidate trong handoff đã `promoted`, `non_durable` hoặc
   `deferred` có link.
9. Actor có grant `work.close`. Risk `high|critical` cần thêm actor human.

Đủ thì Ticket `done`, thả lease, ghi receipt `close` và event. Áp dụng cho mọi
risk; risk cao chỉ thêm human gate, không thiếu đường đóng.

#### Close gate Story

`pulse work close-story <id>`: Story `ready`, mọi child `done|superseded`, ít
nhất một child Ticket terminal, không hard blocker mở, baseline hiện hành không
gap required, receipt `qa_checkpoint` với `story_close` scope passed trên HEAD
hiện tại cover đủ required cases, không flaky/inconclusive chưa waive, docs
required của Story đã có receipt, closing actor khác QA actor. Ghi node,
receipt, event atomic. Epic đóng khi developer đánh giá success signals.

### 5.6 Ratchet: harness tự tốt lên

#### Failure classification

Sau run/review/QA fail, phân loại primary và contributing:

`context_gap`, `tool_gap`, `guardrail_gap`, `verification_gap`,
`task_shape_gap`, `policy_gap`, `environment_gap`, `test_failure`,
`inconclusive_evidence`, `flaky`, `model_execution_error`, `product_defect`,
và docs: `docs_missing`, `docs_stale`, `docs_conflict`, `docs_duplicate_truth`,
`docs_generated_stale`, `docs_work_leak`, `docs_context_gap`.

#### Learning record

`.pulse/knowledge/entries/LRN-001.json`, một learning một file, optional
`knowledge/learnings/LRN-001.md` cho narrative.

```json
{
  "id": "LRN-001", "revision": 2,
  "kind": "failure_pattern",
  "status": "validated",
  "title": "Token rotation cần atomic mutation",
  "summary": "Refresh song song tạo token invalid ngay khi rotation là check-then-act.",
  "guidance": {
    "do": ["Dùng transaction hoặc optimistic conflict."],
    "avoid": ["Tách read và write khi rotate."],
    "required_checks": ["Chạy 10 refresh song song, đúng một cái thành công."]
  },
  "applicability": {
    "paths": ["src/auth/**"], "tags": ["security"],
    "symbols": ["rotateRefreshToken"], "signals": ["token already invalidated"],
    "exclusions": ["stateless access-token verification"]
  },
  "provenance": {"work": ["TK-031"], "receipts": ["01JX…Q1"], "commits": ["d4e5f6"]},
  "confidence": "high",
  "promotion": [{"kind": "document", "id": "DOC-AUTH-DOMAIN", "content_hash": "sha256:…"}],
  "freshness": {"review_after": "2027-03-01", "invalidated_by_paths": ["src/auth/refresh/**"]}
}
```

Kind tối thiểu: `success_pattern`, `failure_pattern`, `correction`, `ratchet`
(must-check đã earned), `decision_heuristic`, `debugging_technique`,
`verification_technique`, `tooling_constraint`, `environment_constraint`,
`integration_constraint`, `context_routing_insight`. Không reusable thì
`non_durable`, không tạo record.

Status: `candidate -> reviewed -> validated -> promoted`, nhánh `non_durable`,
`disputed`, `superseded`, `retired`. `candidate` và `disputed` không tự inject.
Confidence tách khỏi status: `low|medium|high|enforced`.

Relations: `derived_from`, `corroborates`, `contradicts`, `superseded_by`,
`promoted_to`, `implemented_by`, `applied_to`.

#### Vòng lặp

```text
failure / review / QA / recovery có evidence
  -> pulse knowledge capture --from <ticket|receipt>   (candidate)
  -> developer hoặc agent review: reject vague/duplicate, classify, link provenance
  -> pulse knowledge validate <id> --evidence <receipt>  (validated)
  -> pulse knowledge promote <id> --target <doc|decision|check|eval>
  -> pulse knowledge applicable --work <id>  đưa vào packet của việc sau
  -> handoff ghi knowledge_usage: helpful | not_needed | misleading
  -> reinforce / revise / supersede / retire
```

Promotion ladder: observation → candidate → validated → docs/Decision/guidance →
deterministic check trong `runners.json` role `check` → blocking hook. Chỉ
promote lên hook khi signal ổn định và false-positive thấp.

Promotion target theo loại knowledge: user behavior → `docs/product/` +
QA case; boundary/invariant → `docs/architecture/` hoặc Decision; domain rule →
`docs/domain/`; procedure → `docs/operations/`; navigation gap → `AGENTS.md` hay
registry scope; authority rule → `PULSE.md`; mechanical invariant → check;
historical failure → eval fixture; implementation work → Ticket.

`promote --target <doc>` mở doc, developer hoặc agent sửa, Pulse ghi relation
`promoted_to` với content hash mới và đánh dấu learning `promoted`. Handoff có
`documentation_findings` là promotion candidate; close gate đòi disposition.

#### Applicable recall

`pulse knowledge applicable --work <id> [--audience worker|reviewer|qa]`:

1. Lọc eligibility: status cho phép, không exclusion, không contradiction với
   Decision/docs hiện hành.
2. Score: explicit relation > path/symbol > tag/signal > lexical.
3. Bucket: `required` (ratchet enforced hoặc Ticket reference rõ),
   `recommended` (validated, match mạnh), `suggested`, `excluded` kèm lý do.

Packet inject `required` và `recommended` summary trong budget; `suggested`
chỉ ref. Không bao giờ inject toàn bộ corpus. Learning mâu thuẫn Decision hay
docs hiện hành tạo finding, không được override bằng rank.

Freshness: `review_after`, `invalidated_by_paths`, promoted target đổi hash,
usage feedback `misleading` lặp lại. Stale suspicion là finding, không tự xoá.

Compound run post-cycle (`pulse compound <work>`), doctor, retrieval eval là
**Later**. Chấp nhận kết luận `no_reusable_learning`.

### 5.7 Giao tiếp giữa agent: event log, không broker

Kênh là `.pulse/events/` append-only, mọi mutation đã ghi. Mọi process đọc ghi
qua CLI, sống sót khi process chết, audit lại được. Không daemon, socket,
presence, mailbox, delivery guarantee.

```text
pulse events tail --since <cursor> [--follow] [--ticket <id>] --json
pulse note --ticket <id> "<nội dung>" [--from <actor>]
```

`note` là event nhắm Ticket, hiện trong packet và `events tail` của agent giữ
Ticket. Không có semantics delivery.

| Nhu cầu | Cơ chế |
|---|---|
| Biết worker xong chưa | handoff receipt, Ticket `verifying`; `events tail` hoặc `work list --status verifying` |
| Reviewer trả rework | verification `rework` kèm findings; worker thấy trong packet lần chạy lại |
| Worker kẹt, cần hỏi | Ticket `blocked` + note hoặc `decision_request`; human trả bằng Decision hoặc sửa ticket.md |
| Worker A đổi API, B đang dùng | A `note --ticket B` hoặc Decision; B thấy trong packet/tail |
| Conductor biết ai giữ gì | lease, `work list --status active` |
| Contract đổi giữa chừng | event `contract_drift`; worker acknowledge hoặc handoff rồi dừng |

#### Chạy song song

Nhiều Ticket song song chỉ khi: hard dependency đã thoả, lease khác nhau,
Ticket thứ hai trở đi vào worktree, không hard conflict trên write scope (cùng
file), thứ tự merge rõ. `pulse run` cảnh báo overlap path giữa các Ticket
active; overlap là advisory, developer quyết định.

Conductor là developer, hoặc một agent session dùng chính các lệnh Pulse để
fan-out. Pulse không có orchestration loop riêng.

### 5.8 Giao diện và authority

#### CLI

```text
pulse init
pulse work     create|show|list|edit|ready|packet|claim|release|handoff|verify|close|close-story|supersede|rollup|transition
pulse graph    edge add|remove, neighborhood, affected-by, validate, export, recover
pulse docs     register|edit|retire|supersede|list|show|tree|applicable|search|get|index|validate|impact
pulse evidence receipt record|show|list|verify, artifact put|show|verify
pulse qa       baseline <story>, resolve <ticket>
pulse knowledge create|capture|show|list|edit|validate|promote|supersede|retire|applicable|search|get|relation
pulse run      <role> --ticket <id> [--isolation worktree]
pulse events   tail
pulse note
```

Mọi lệnh có `--json` với `schema_version`, stable field, non-zero exit khi
invalid. Human output ngắn. Mutating command nhận `--expected-revision` và
`--actor`.

MCP server mỏng khoảng 10 tool (`next_ready`, `packet`, `claim`, `handoff`,
`verify`, `close`, `docs_search`, `docs_get`, `knowledge_applicable`, `note`),
làm sau khi CLI path chạy thật.

#### Authority

`.pulse/policy/authority.json` default-deny, grant liệt kê rõ, không wildcard.
`pulse init` tạo principal cho developer với đủ mọi grant Core, nên fresh repo
dùng được ngay. Runner actor `runner:<role>` nhận grant theo role: worker có
`work.handoff`, `note`, `knowledge.capture`; reviewer có `work.verify`; qa có
`evidence.record`. Không runner nào có `work.close`, `decision.accept`,
`docs.edit-approved`.

| Hành động | Developer | Worker | Reviewer/QA |
|---|---|---|---|
| Sửa objective/acceptance | có | không, gửi decision_request | không |
| Tạo/link/prioritize work | có | đề xuất qua handoff | không |
| Sửa source trong scope | có | có | read-only |
| Handoff / finding / note | có | có | có |
| Verify | có | không tự verify | có |
| Close Ticket/Story | có | không | không |
| Accept Decision | có | đề xuất | đề xuất |
| Promote learning vào docs | có | đề xuất | đề xuất |

## 6. Không có trong product

- Daemon, session/process/provider management, timeline, transport, WebSocket.
- Broker, mailbox, session registry, presence, request/reply giữa agent.
- Playwright, environment lifecycle, deployment identity, trace validation
  trong Pulse.
- Story qualification matrix per platform, flaky waiver grant riêng, priority
  reconciliation receipt, semantic deliberation.
- Persisted shaping map, decision frontier, fog-of-war.
- Multi-user authorization, dashboard, report, external tracker sync.
- Semantic/hybrid search, embedding, reranker.
- Windows tier-1 cho đến khi có người dùng Windows.
- Worktree mặc định.

## 7. Golden path v0.1

Tiêu chí "xong" của v0.1. Chưa đạt thì không thêm feature.

1. `pulse init` trên một repo thật; developer có đủ grant ngay.
2. Tạo Story với `qa.md` hai case, tạo Ticket R1 với `ticket.md` đầy đủ, risk
   medium, QA required, docs required. Ticket `ready`.
3. `pulse work packet` sinh packet developer đọc thấy đủ và không thừa.
4. `pulse run worker --ticket <id>` chạy một agent thật trong checkout; agent
   `handoff` qua CLI với acceptance mapping.
5. `pulse run qa` bằng script của repo ghi `qa_checkpoint`; `pulse run
   reviewer` bằng agent khác ghi `verification`; `docs validate --record`;
   developer `close`. Ticket risk medium đóng được.
6. Kill agent giữa chừng, `pulse run` lại, lease và packet đúng; sửa source sau
   handoff làm receipt stale và close bị từ chối với lý do.
7. `knowledge capture` một learning từ lần chạy, `promote` vào một doc, Ticket
   tiếp theo chạm cùng path thấy learning trong packet.

Tiêu chí phụ: hai Ticket chạy song song bằng hai `pulse run`, Ticket thứ hai tự
vào worktree, không va nhau; Story đóng bằng `close-story` sau khi hai Ticket
done và qualification pass.

## 8. So với code hiện tại

| Tính năng | Hiện trạng | Việc cần làm |
|---|---|---|
| 5.1 Work graph | Có, tốt | ticket.md thành nguồn contract, bỏ JSON contract 25 trường; bỏ shaping map/receipt bắt buộc; default grants khi init |
| 5.2 Packet | Có, quá dày | Nhúng ticket.md; bỏ section `not_installed`; bỏ DTO mirror |
| 5.3 Runner | Chưa có | Tách contract chạy lệnh từ `qa/executor.rs` thành `runner/`; thay `daemon/` |
| 5.4 Docs | Có | Metadata 17 → 8 trường; `tags` + `tags.json`; bỏ eval, bench, schema JSON không dùng, per-doc retrieval knob |
| 5.5 Evidence/QA | Có | Bỏ Playwright kind, env lifecycle, story matrix, flaky waiver grant; qa.md 16 → 6 trường; mở close cho mọi risk với human gate |
| 5.6 Ratchet | Chỉ có store + validate | `capture`, `validate`, `promote`, `applicable`; packet inject; usage feedback |
| 5.7 Giao tiếp | Event log có | `events tail`, `note`; thay `communication.rs`, `timeline.rs` |
| 5.8 MCP | Stub không bind | Server thật, sau CLI |

## 9. Triage code

| Nhóm | Module |
|---|---|
| Spine | `graph/`, `storage/`, `kernel/{reservation,completion,packet,readiness}`, `evidence/`, `source.rs`, `event.rs`, `policy/`, `cli/{work,graph,evidence,docs}` |
| Supporting | `docs/{registry,applicability,index,search,validate,check}`, `qa/{baseline,receipt}`, phần contract trong `qa/executor.rs` |
| Frozen | `knowledge/` cho đến khi làm 5.6, `docs/{eval,cache}`, `benches/` |
| Remove | `daemon/` (giữ `assignment.rs` trong `design/archive/` làm tham chiếu runner), `cli/daemon.rs`, browser/env/qualification trong `qa/executor.rs` và kernel, 10 JSON schema nhúng, 13 shim re-export trong `graph/`, `windows-sys` |

## 10. Cấu trúc docs mục tiêu

```text
PRODUCT.md            file này
README.md             cái đã có code và test, không hơn
ARCHITECTURE.md       kiến trúc hiện tại; viết sau khi cắt code
ROADMAP.md            Now / Next / Later / Undecided; viết sau khi cắt code
AGENTS.md             quy tắc vận hành, validation command, trỏ sang PRODUCT.md
CONTRIBUTING.md       workflow đóng góp
docs/decisions/       ADR; 0008 ghi quyết định thu hẹp phạm vi này
docs/GLOSSARY.md      thuật ngữ theo PRODUCT.md
design/archive/       proposals đã xong; assignment saga làm tham chiếu runner
examples/todolist/    target repo dogfood
```

## 11. Later

Thứ tự ưu tiên sau golden path: MCP server; `pulse doctor` với finding có
evidence/impact/proposed Ticket; compound run và retrieval eval; persisted
shaping map và decision frontier cho R2/R3; Story qualification matrix; external
tracker adapter; semantic search adapter nếu lexical eval chứng minh recall gap;
conductor agent loop; Windows.

## 12. Dấu hiệu đang đi sai hướng

- Số artifact tăng nhưng correct completion không tăng.
- Agent phải đọc file hướng dẫn khổng lồ trước mọi việc.
- `done` vẫn dựa câu báo cáo thay vì receipt.
- Hai hệ thống cùng sửa status mà không có field ownership.
- Registry bắt mọi markdown mang metadata dù không cần routing.
- `_index.md` hoặc cache trở thành truth thứ hai.
- Search mặc định trả full document.
- Durable knowledge mắc kẹt trong work artifact đã đóng.
- Feature được viết thành docs, schema, version trước khi có người dùng.
- Pulse bắt đầu quản lý process, session, browser hay môi trường.

Gặp một dấu hiệu thì dừng feature liên quan, ghi Decision, sửa harness trước.

## 13. Quyết định

1. **Target repo để dogfood: `examples/todolist/`** trong repo Pulse, cùng Git
   history, không nested `.git`. `.pulse/` và `works/` của nó tracked;
   `runtime/` và `cache/` ignored. Lệnh cấm `--repo-root .` ở gốc Pulse vẫn
   giữ. Việc đầu tiên: `src/source.rs` phải strip `git rev-parse --show-prefix`
   khỏi path Git trả về khi repo-root là thư mục con. Chốt 2026-09-05.

Còn mở, mặc định nếu không có ý kiến khác:

2. **Agent runner đầu tiên.** Mặc định Claude Code headless.
3. **Thời điểm xoá `daemon/`.** Mặc định xoá ngay trong bước cắt code.
4. **Cách parse ticket.md.** Mặc định heading quy ước, không fenced block.

## 14. Nguồn tham khảo đã hấp thụ

OpenAI Harness Engineering (repo là môi trường thực thi, AGENTS.md là map,
guardrail cơ học, failure cải thiện harness); Symphony (tracker → normalized
packet → agent; local graph đóng vai tracker); Maestro (typed card, relation,
sidecar prose); Matt Pocock grilling/wayfinder (one-question-at-a-time,
destination, frontier, fog); QMD và Knowledge Base Builder (search/get tách,
section unit, progressive index); Paseo (đã học rồi quyết định không làm
runtime). Chi tiết thiết kế gốc nằm trong Git history của `pulse-reboot/`
trước commit xoá.
