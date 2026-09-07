# Pulse — Product Definition

> Trạng thái: chốt ngày 2026-09-05, cập nhật 2026-09-06 theo Decision 0009,
> 0010, 0011, 0012, 0013, 0014. Đây là nguồn sự thật về sản phẩm và thiết kế
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
10. **Làm trực tiếp trong checkout.** `pulse run` từ chối khi một Ticket khác
    đang `active`; worktree chỉ được tạo khi gọi rõ `--isolation worktree`.
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

Event log `.pulse/events/` là audit trail bất biến cho mọi plane, JSONL theo
ngày (Decision 0011). `.pulse/cache/` luôn gitignored, xoá được, không cần cho
correctness.

```text
AGENTS.md                      # repository map, ngắn; khối <!-- PULSE:BEGIN/END --> do pulse init ghi
PULSE.md                       # intent, risk policy, verification profiles, human gates

docs/
  product/                     # user/system-visible behavior contract
  architecture/                # boundaries, dependency direction, invariants
  domain/                      # glossary.md (DOC-GLOSSARY, grill ghi vào), rules, state machines
  operations/                  # setup, deploy, recovery, runbooks
  reference/                   # authored API/config reference
  generated/                   # projection từ code, không hand-edit
  decisions/                   # ADR projection (optional)
  _index.md                    # generated navigation, không phải truth

works/
  EP-001/  brief.md design.md              # brief.md là map: Destination, Notes, Decisions so far, Not yet specified, Out of scope
  ST-014/  story.md approach.md qa.md research/<topic>.md
  TK-031/  ticket.md plan.md validation.md research/<topic>.md   # research/ khi decision_work
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
  events/<date>.jsonl          # một event một dòng, append-only
  policy/authority.json
  config/runners.json
  runtime/                     # lock, transaction intent, lease TTL, run/<ticket>/, handoff/<node>.md; gitignored
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
cần. Packet fence theo source commit + dirty hash và fingerprint của mọi input:
worktree được phép bẩn, packet ghi `source.dirty`/`source.dirty_hash`; code hay
worktree đổi thì packet cũ hết hiệu lực và `pulse run` từ chối dùng packet stale.

Trước lease là preview; trong `pulse run`, packet được commit cùng lease và
không tự rebuild từ revision mới. Contract đổi giữa chừng tạo finding
`contract_drift`; worker phải acknowledge hoặc handoff rồi dừng.

### 5.3 Runner: gọi agent hay script bất kỳ

#### Config

```json
// .pulse/config/runners.json
{
  "worker":   {"command": "claude -p --output-format text --dangerously-skip-permissions 'Pulse worker run. First read .pulse/runtime/run/{ticket}/worker-prompt.md in this repository and follow those instructions exactly.'", "timeout_seconds": 3600},
  "reviewer": {"command": "codex exec --sandbox workspace-write '<prompt pointer như worker>'", "timeout_seconds": 1800},
  "qa":       {"command": "node scripts/qa-run.mjs {input}", "timeout_seconds": 900},
  "check":    {"command": "npm run docs:check", "timeout_seconds": 300}
}
```

Role là tên tuỳ ý. Placeholder: `{input}` đường dẫn file input, `{ticket}`,
`{repo}` workspace mà role chạy trong đó (là worktree khi run bị cô lập),
`{state_repo}` repo chính giữ state plane, `{artifact_dir}`. `{input}` và
`{artifact_dir}` luôn trỏ vào workspace mà agent thấy, nên một lệnh viết
đúng chạy y hệt trong checkout và trong worktree (Decision 0015).
Không shell interpolation; args parse bằng argv
parser, không qua `sh -c`. Lệnh agent nhận prompt positional trỏ tới
bootstrap prompt Pulse viết sẵn; `--output-format text` giữ JSON cuối của
agent là dòng stdout cuối (contract output của runner).

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
8. Ghi receipt tương ứng role và event `run.completed`, kèm `session_ref` là
   session id của host agent nếu lấy được. Thả process, giữ hoặc thả lease
   theo role. Với role `qa`, Pulse dựng `qa_checkpoint` từ output và artifact
   đã hash; script không tự ghi receipt (Decision 0014).

Agent trong lúc chạy vẫn gọi CLI trực tiếp: `pulse work handoff`, `pulse note`,
`pulse docs get`, `pulse knowledge get`. Output JSON cuối chỉ là tóm tắt.

Bootstrap prompt cho agent chỉ mô tả workflow và identity: "packet ở file X,
đọc required docs bằng `pulse docs get`, khi xong gọi `pulse work handoff`,
không tự đổi acceptance". Không copy Ticket, docs, QA, knowledge vào prompt.

#### Isolation rule

- Mặc định chạy trực tiếp trong checkout.
- Khi đã có Ticket `active` khác trong cùng repo-root, `pulse run` **từ chối**
  với `run_isolation_required`, in ra Ticket đang giữ lease và gợi ý lệnh với
  `--isolation worktree`. Không auto-worktree.
- `--isolation worktree` ép tạo worktree cho Ticket mới. Lease theo Ticket,
  không theo worktree.
- Worktree do Pulse tạo thì Pulse dọn khi Ticket terminal và không còn
  reference. Không xoá thứ Pulse không tạo.
- **Worktree là workspace của repo chính, không phải repo Pulse thứ hai**
  (Decision 0015). Worktree sở hữu **duy nhất** source plane; mọi mutation
  plane (workgraph, evidence, event, knowledge, policy, config, runtime
  lease) route về repo chính và ghi dưới đúng một lock của nó. Nhận diện
  bằng marker `.pulse-owned` do Pulse ghi khi tạo, đối chứng bằng Git; worktree
  developer tự tạo không có marker nên không bao giờ được map.
- Run workspace được ghi vào **cả hai nơi**: bản record ở runtime repo chính,
  và bản agent dùng tại `<workspace>/.pulse/runtime/run/<ticket>/`. Prompt
  nhúng path tuyệt đối của workspace. Bản trong worktree là copy dùng một
  lần, chết cùng worktree, không phải truth.
- **Reviewer và QA chạy trong cùng workspace với worker.** Ticket có worktree
  sống thì assurance role lấy cwd là worktree đó: review đúng cây đã handoff,
  không phải một cây khác.
- Worktree lệch commit so với repo chính được báo `worktree_graph_stale`
  trong run record. Pulse báo, không tự rebase.

#### Contract input/output theo role

Worker input = packet. Worker ghi handoff qua CLI với claim máy đọc, cùng cú
pháp `--check` và `--proof` như `work verify` (Decision 0012); `summary` một
dòng. Worker output:

```json
{"status": "handed_off", "handoff_receipt": "01JX…H1", "summary": "…", "blockers": []}
```

hoặc `{"status": "blocked", "reason": "…", "decision_request": "…"}`. Worker
hết context giữa chừng làm thủ tục bàn giao phiên (§5.7) rồi trả `blocked`
với `reason: context_exhausted`; lease giữ, `pulse run` lần sau resume
(Decision 0013).

QA input (`qa-input.json`, sinh từ parse `qa.md`, Decision 0010; runner không
đọc `qa.md`):

```json
{
  "schema_version": 1,
  "ticket_id": "TK-031", "story_id": "ST-014", "qa_scope": "ticket_checkpoint",
  "source_commit": "d4e5f6",
  "baseline_path": "works/ST-014/qa.md", "baseline_content_hash": "sha256:…",
  "posture": "automated",
  "variables": {"REPO": "/abs", "ARTIFACT_DIR": ".pulse/runtime/run/TK-031/qa-artifacts",
                "STATE_FILE": ".pulse/runtime/run/TK-031/qa-state.json"},
  "cases": [ {
    "id": "QA-001", "title": "…", "case_hash": "sha256:…",
    "intent": "…", "surface": "api", "priority": "high", "applicability": "required",
    "risk_refs": ["RISK-LEAK"], "preconditions": ["…"], "steps": ["…"], "expected": ["…"],
    "evidence": ["stdout"],
    "check": {"run": ["node", "scripts/x.mjs"], "env": {}, "assert": [{"exit_code": 0}]}
  } ]
}
```

`check` chỉ có khi case mang block `pulse-check`. Case không có `check` mà
runner là script thì trả `inconclusive` với lý do, không đoán.

QA output:

```json
{
  "cases": [ {"id": "QA-001", "status": "passed|failed|inconclusive|not_applicable", "observation": "…"} ],
  "artifacts": [ {"path": "…", "role": "log|screenshot|trace", "case_id": "QA-001"} ],
  "findings": [ {"case_id": "QA-004", "summary": "…", "severity": "high"} ]
}
```

Reviewer input (`reviewer-input.json`, Decision 0012) mang **claim để
verify, không mang lời kể của worker**: không có `summary`, không có checks
worker khai, không có remaining risk. Reviewer lấy contract từ `pulse work
show`, diff từ Git, và tự chạy lệnh verify.

```json
{
  "schema_version": 1,
  "ticket_id": "TK-031", "source_commit": "d4e5f6", "contract_revision": 4,
  "acceptance": [{"id": "AC-1"}, {"id": "AC-2"}],
  "handoffs": [{"handoff_id": "01JX…H1", "changed_paths": ["src/auth/errors.ts"],
                "recorded_by": "agent:runner:worker", "source_commit": "d4e5f6"}],
  "proof_receipts": {"qa_checkpoint": ["01JX…Q1"], "documentation_validation": ["01JX…D1"]},
  "reviewers_required": 1,
  "artifact_dir": "artifacts"
}
```

`proof_receipts` được lọc theo hai cách khác nhau vì hai loại receipt có
subject khác nhau (Decision 0016): `qa_checkpoint` theo subject là Ticket;
`documentation_validation` theo **source commit đang review**, vì nó
subject-bound tới documentation registry của repo chứ không tới một Ticket.
Lọc docs receipt theo ticket id không bao giờ khớp.

Reviewer output:

```json
{
  "disposition": "pass|rework",
  "acceptance": {"AC-1": {"check": "pnpm test auth"}},
  "findings": [{"acceptance_id": "AC-2", "summary": "…", "owner": "src/auth/RefreshTokenHandler.ts",
                "check": "pnpm test auth -- --grep revoked", "severity": "high"}]
}
```

Check output: `{"status": "passed|failed", "findings": []}`.

**Shape finding** chung cho reviewer, QA, check và `doctor` sau này: `summary`,
`owner` (repository-relative path hoặc `DOC-ID#section`), `check` (lệnh argv
đã chạy và thấy fail, hoặc receipt id); reviewer thêm `acceptance_id`, QA thêm
`case_id`. Thiếu `check` thì finding vẫn được ghi với `unverifiable: true` và
không một mình là lý do `rework`; một `rework` mà mọi finding đều
`unverifiable` được ghi `inconclusive`, không phải verdict. Đếm, filename,
tuổi file, severity không phải finding.

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
| `handoff` | worker | changed files, `checks[] {name, command, exit_code}`, `acceptance_proofs[] {acceptance_id, check_names, evidence_receipt_ids}`, summary một dòng, docs finding, learning candidate; binding lease, session, commit, `source_dirty_hash`, revision |
| `verification` | reviewer khác worker | acceptance → check/receipt mapping, disposition pass/rework, findings |
| `qa_checkpoint` | qa runner | case nào pass/fail trên baseline hash nào, artifact |
| `docs_validation` | **worker**, trước handoff (Decision 0016) | document id + content hash pass mechanical checks; subject là documentation registry của repo tại một commit, không phải một Ticket |
| `decision_acceptance` | human có grant | Decision id + content hash được accept |
| `close` | close gate | Ticket/Story đóng với receipt nào, graph fingerprint |

Validity chung: schema đúng, subject tồn tại đúng `contract_revision`, source
commit khớp target hoặc ancestor policy cho phép, artifact hash khớp, actor có
grant, actor độc lập khi gate yêu cầu. Receipt cũ không bị sửa khi hết hiệu lực;
gate chỉ không dùng nó nữa.

**Ranh giới riêng tư cho plane tracked** (Decision 0012): mọi trường text của
receipt payload, note, learning và `findings[].summary` bị từ chối với
`receipt_privacy_violation` khi chứa absolute path ngoài repo root hoặc chuỗi
khớp mẫu secret (danh sách trong `src/evidence/redaction.rs`, mở rộng bằng
PR). Path dưới repo root được rewrite thành repository-relative.
`session_ref` là id mờ, được giữ vì là join key duy nhất với transcript host.
`stderr_tail` chỉ ở run record trong `runtime/`, không vào receipt. Nội dung
artifact không được quét.

#### Verification profiles và review layers

Profile nằm trong `PULSE.md` của repo, không hard-code trong Pulse:

```yaml
profiles:
  docs-only:         {commands: ["npm run lint:docs"], review: light}
  service-change:    {commands: ["npm run lint", "npm test"], review: standard}
  web-behavior:      {commands: ["npm test"], qa: required, review: standard}
  migration:         {commands: ["npm run test:migrations"], review: independent, reviewers: 2, rollback: required}
  security:          {commands: ["npm test", "npm audit"], review: independent, reviewers: 2, human: required}
```

Layers, chọn theo risk: self-check (worker) → mechanical (lint, test, check
runner) → independent review (reviewer khác worker) → QA checkpoint → human
gate (security, destructive, production). Không bắt mọi Ticket qua mọi layer.

**Reviewer là bằng chứng, không phải authority** (Decision 0012). Reviewer
không sửa file; lead không đưa kết luận của mình vào prompt reviewer; không
chấp nhận theo điểm trung bình. `reviewers` (mặc định 1) là số actor khác nhau,
khác worker, mỗi actor một receipt `verification` trên cùng `handoff_id`. Chỉ
profile risk cao khai `reviewers: 2`; không bật tam giác hoá toàn cục. Model
nào làm reviewer thứ hai là việc của `runners.json` (`reviewer`, `reviewer-2`),
không phải của Pulse.

#### QA

QA trả lời: "hành vi Story hứa có còn đúng trên snapshot này, và ai chứng minh?"
Nó khác developer verification (Ticket có thoả contract kỹ thuật không). Cùng
một test có thể phục vụ cả hai; khác nhau ở intent, actor, source binding và
receipt.

**Baseline** `works/<STORY>/qa.md`: markdown heading quy ước như `ticket.md`,
không fenced JSON (Decision 0010). Pulse parse thành `qa-input.json` cho runner.
Hash toàn file và hash section từng case bind vào receipt; không có `Revision:`
viết tay.

```markdown
# ST-014 QA baseline — Refresh token failure contract

## Scope
Client phân biệt được token hết hạn với token không hợp lệ.

## Posture
automated

## Risks
- RISK-LEAK: response lộ chi tiết xác thực nội bộ.

## Exit criteria
- Mọi case required pass trên candidate source.

## Cases

### QA-001 Token hết hạn trả TokenExpired
- Intent: Client gửi refresh token đã hết hạn thì nhận mã TokenExpired, không phải InvalidToken.
- Surface: api
- Priority: high
- Risks: RISK-LEAK
- Preconditions:
  - Một refresh token hợp lệ đã quá hạn 1 giờ.
- Steps:
  1. POST /auth/refresh với token đó.
- Expected:
  - HTTP 401, body.code = TokenExpired.
  - Body không chứa lý do nội bộ.

```pulse-check
run: node scripts/qa/refresh-expired.mjs
assert:
  - exit_code: 0
  - stdout_json_path: {path: "$.code", equals: "TokenExpired"}
```
```

Trường case: `Intent`, `Surface` (`cli|api|ui|job|docs`), `Priority`,
`Applicability` (`required` mặc định, `not_applicable` cần `Reason`), `Risks`,
`Preconditions`, `Steps`, `Expected`, `Evidence`, và block `pulse-check` tuỳ
chọn cho surface `cli|api` (argv không qua shell, tập assertion cố định). Dòng
`Key:` lạ bị từ chối kèm tên dòng. Case nói hành vi, không khoá selector hay
implementation detail. Contract đầy đủ trong Decision 0010.

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
validation là việc của script. Script chỉ in JSON cuối; `pulse run qa` ingest
artifact rồi tự dựng `qa_checkpoint` với `bindings.artifacts` đã hash và
`bindings.content[qa.md]`; drift baseline hay run không sạch thì
`inconclusive`, không có receipt. Actor `runner:qa` không cần grant
`evidence.record` (Decision 0014).

#### Close gate Ticket

`pulse work close <id>` từ chối với lý do cụ thể nếu thiếu:

1. Ticket `verifying`, revision khớp.
2. Receipt `handoff` trên commit hiện tại.
3. Đủ `reviewers` receipt `verification` pass trên handoff hiện hành, mỗi
   receipt một actor khác nhau và khác actor handoff. Một receipt `rework` là
   đủ để Ticket `rework`; packet lần sau liệt kê finding của mọi reviewer kèm
   actor.
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

#### Thang bằng chứng

Cấu hình chỉ chứng minh cơ chế tồn tại; chỉ task thực chứng minh nó được dùng;
chỉ kết quả sau chứng minh nó có ích (Decision 0012). Mỗi cơ chế của harness
(doc, role `check`, QA case, learning, profile) có một bậc, tính deterministic
từ registry, `runners.json`, packet, receipt và relation. Không có điểm số,
không có con số tổng hợp cho cả repo: không gate nào đọc điểm, và Pulse không
đọc transcript nên một con số sẽ mãi phản ánh cấu hình chứ không phải hành vi.

| Bậc | Nghĩa | Ví dụ Pulse tính từ đâu |
|---|---|---|
| `present` | cơ chế tồn tại | doc trong registry; role trong `runners.json`; learning `candidate` |
| `wired` | có đường để một task chạm tới | doc `required` trong ít nhất một packet; role `check` nằm trong một profile; learning `promoted` |
| `exercised` | một task đã dùng và để lại kết quả | `docs_validation` receipt; `check` receipt; handoff ghi `knowledge_usage: helpful` |
| `outcome_supported` | kết quả sau cho thấy nó có ích | learning có hai provenance `corroborates`; check đã chặn một handoff thật |
| `missing` | inspect xác nhận thiếu | docs `required` trỏ id không tồn tại; profile gọi role không có |
| `unobserved` | không có bằng chứng để quyết | chưa Ticket nào chạm path của doc; role chưa từng chạy |
| `not_applicable` | inspect chứng minh không áp dụng | Story posture `not_applicable` có lý do |

Bậc không phải pass/fail: `exercised` có thể để lộ defect, `unobserved` không
phải `missing`. Bậc mô tả cơ chế, không mô tả Ticket. Đây là từ vựng chung
của `pulse-ratchet` và `pulse doctor`.

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
worker/reviewer gặp ma sát -> pulse note --ticket <id> --kind friction  (luôn, không tự sửa harness)
close gate gom note friction -> learning candidate, scope harness
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

Skill `pulse-ratchet` (§5.8) chạy vòng này sau `work close` bằng **ba lane
bằng chứng độc lập và một lead hoà giải** (Decision 0012). Mỗi lane sở hữu
một bậc của thang bằng chứng, nhận đúng input của mình, read-only, không ủy
quyền tiếp, không gán severity, trả tối đa năm candidate theo shape finding:

| Lane | Trả lời | Được đọc | Không được đọc |
|---|---|---|---|
| `execution` | Chuyện gì đã xảy ra khi chạy Ticket này? | receipt, run record, `events tail --ticket`, note friction, `knowledge_usage` | registry, AGENTS/PULSE.md, `runners.json`, learning |
| `harness` | Cơ chế nào tồn tại và có được wire không? | registry, `AGENTS.md`, `PULSE.md`, `runners.json`, `qa.md`, profile, packet đã dùng | receipt, note, learning |
| `knowledge` | Learning hiện có còn đúng, lặp, hay mâu thuẫn không? | `knowledge/`, relation, freshness, Decision accepted, docs approved | receipt, note friction |

Lead là chính session ratchet: giữ mọi candidate; chỉ merge khi cùng target,
hậu quả, owner, đường sửa; một mình gán severity; không rescan sau khi lane
xong; mỗi candidate còn lại thành `knowledge capture` với `provenance` trỏ
lane và receipt. Rồi chọn đúng một intervention theo track: `bootstrap` (chưa
có harness, gợi `pulse-onboard`), `operationalize` (cơ chế `present` chưa
`wired|exercised`, wire nó), `optimize` (learning có hai `corroborates`,
promote lên check hoặc `AGENTS.md`), `undetermined` (lane `unavailable`, ghi
finding `evidence_gap`, không sửa). Track chỉ chọn intervention, không thêm
finding, không đổi severity. Ticket R0 chạy `--lanes execution`. Binary Pulse
không spawn lane; skill dùng fan-out của host.

Ratchet được sửa `AGENTS.md`, `PULSE.md`, role `check` trong `runners.json`
ngay khi có candidate, không hỏi. Learning kind `ratchet` phải có
`expected_signal`: một dòng nói handoff của Ticket rerun phải thấy gì. Learning
chỉ lên `validated` khi handoff của Ticket sau ghi `knowledge_usage: helpful`
**và** `expected_signal` xuất hiện trong receipt của Ticket đó. Không có rerun
thì vẫn `candidate`, không claim cải thiện. `pulse ratchet bundle` đóng băng
input ba lane chỉ được thêm khi dogfood thấy lane rò rỉ sang nhau. Đọc
transcript host theo `session_ref` để tìm friction là Later.

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

Kênh là `.pulse/events/<date>.jsonl` append-only, một event một dòng, mọi
mutation đã ghi (Decision 0011). Ghi dưới repository lock với fsync; dòng cuối
cụt do crash bị reader bỏ qua và writer cắt trước khi append. Cursor là ULID
của event. Mọi process đọc ghi qua CLI, sống sót khi process chết, audit lại
được. Không daemon, socket, presence, mailbox, delivery guarantee.

Pulse không ghi trace của agent (tool call, prompt, token): đó là transcript
của host. Pulse chỉ giữ mối nối: `session_ref` trong handoff receipt và
`run.completed`, và gợi ý trailer `Pulse-Ticket: <id>` khi close.

```text
pulse events tail --since <cursor> [--follow] [--ticket <id>] --json
pulse events compact                       # chuyển <date>/evt_*.json cũ sang <date>.jsonl, một lần
pulse note --work <id> "<nội dung>" [--from <actor>] [--kind friction]   # --ticket là alias
```

`note` là event nhắm một node bất kỳ (Epic, Story, Ticket, Decision), hiện
trong packet của Ticket và `events tail`. Tối đa 2000 ký tự. Không có
semantics delivery.

#### Bàn giao phiên (Decision 0013)

Khác với `work handoff` (receipt của một Ticket), bàn giao phiên là chuyển
một cuộc trò chuyện sang phiên mới khi context sắp đầy. Host đếm và ép ngưỡng
bằng hook; Pulse không đọc transcript, không đếm token, không gắn hook. Skill
`pulse-handoff` (user-invoked) làm ba bước: **flush** mọi thứ durable về
`works/`, `docs/` và graph qua lệnh `pulse`; **doc** chỉ live thread tại
`.pulse/runtime/handoff/<node>.md`, trỏ path không chép, redact; **note** một
dòng con trỏ bằng `pulse note --work <node>`, rồi in lệnh mở phiên mới và
dừng. Nếu live thread không vừa, artifact còn thiếu, quay lại flush. Không
HANDOFF.json, không file bàn giao trong `works/`. Hook mẫu cho Claude Code
nằm trong `docs/operations/` của repo đích. `--kind handoff` và `pulse work
resume` (query thuần, không spawn) là Later.

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
Ticket thứ hai trở đi được gọi với `--isolation worktree` (nếu không, `pulse
run` từ chối), không hard conflict trên write scope (cùng file), thứ tự merge
rõ. `pulse run` cảnh báo overlap path giữa các Ticket active; overlap là
advisory, developer quyết định.

Isolation là thật, không phải quy ước: agent trong worktree tìm thấy run
workspace ngay dưới cwd của nó và CLI route state về repo chính, nên nó không
có lý do gì để đi vào checkout chung (Decision 0015). Mutation của mọi worker
song song vẫn hội tụ về một lock duy nhất của repo chính; worktree chỉ mang
source.

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
pulse events   tail|compact
pulse note     [--kind friction]
```

Mọi lệnh có `--json` với `schema_version`, stable field, non-zero exit khi
invalid. Human output ngắn. Mutating command nhận `--expected-revision` và
`--actor`.

MCP server mỏng khoảng 10 tool (`next_ready`, `packet`, `claim`, `handoff`,
`verify`, `close`, `docs_search`, `docs_get`, `knowledge_applicable`, `note`),
làm sau khi CLI path chạy thật.

#### Bề mặt hướng dẫn (Decision 0009)

Quy trình sống trong repo đích, không trong Pulse: `pulse init` ghi khối
`<!-- PULSE:BEGIN --> … <!-- PULSE:END -->` vào `AGENTS.md` và tạo
`PULSE.md`; `pulse init --refresh` render lại khối theo version CLI, giữ
nguyên ngoài marker, phát hiện sửa tay trong marker thì báo, không ghi đè.
Khối route theo hình dạng yêu cầu, không theo chuỗi bước cố định:

```text
chỉ đọc                 -> work show/packet, docs search/get; không mutation
nhỏ, hướng rõ, R0       -> work create --risk low, ticket.md tối thiểu, work ready, run
public behavior / nhiều Ticket / risk >= medium
                        -> pulse-grill -> pulse-spec -> pulse-tickets -> run
lớn hơn một phiên, đường đi chưa thấy
                        -> pulse-wayfind trước, rồi grill
mơ hồ sản phẩm còn mở   -> dừng trước mutation; Open question (blocking) hoặc Decision;
                           hỏi một câu kèm câu trả lời gợi ý
ma sát với harness      -> note --kind friction; không tự sửa AGENTS/PULSE/runners trong Ticket
sau work close          -> pulse-ratchet
context sắp đầy         -> pulse-handoff: flush, doc runtime, note, in lệnh mở, dừng
phiên mới               -> work list --status active|shaped, events tail, đọc doc handoff
                           (pulse work resume khi có)
```

Skill là hướng dẫn, CLI là authority: mọi mutation trong skill là lệnh `pulse`
nguyên văn, không state riêng, không gate riêng. Mỗi skill kết thúc ở một
artifact và một trạng thái graph:

| Skill | Kết thúc ở | Gate human |
|---|---|---|
| `pulse-wayfind` (user-invoked, on-ramp) | Epic với `brief.md` là map; `decision_work` Ticket là câu hỏi; Decision node là câu trả lời; `blocked_by` là frontier; một ticket một phiên; map xong thì bàn giao, không build | Destination; Decision accepted |
| `pulse-grill` (implicit khi mơ hồ) | Story `shaped`; term chốt ghi ngay `docs/domain/glossary.md`; Decision node chỉ khi khó đảo ngược, khó hiểu nếu thiếu context, có trade-off | xác nhận hiểu chung |
| `pulse-spec` (user-invoked) | `approach.md` (solution, seam, implementation và testing decisions, out of scope), `qa.md`; không phỏng vấn lại; Story R2+ và Decision R3 qua hai đến ba reviewer cùng prompt read-only, mỗi reviewer `note --kind review`, `decision_acceptance` liệt kê note đã đọc | seam, hỏi một lần |
| `pulse-tickets` (user-invoked) | Ticket `ready`, `blocked_by`; tracer bullet, blocker tạo trước | breakdown; ready gate |
| `pulse-research` (model-invoked) | `works/<id>/research/<topic>.md`, subagent nền, nguồn sơ cấp; packet liệt kê dạng ref | không |
| `pulse-ratchet` (explicit) | ba lane `execution`/`harness`/`knowledge` độc lập, lead hoà giải, learning có `expected_signal`, một intervention theo track, chờ rerun | không |
| `pulse-onboard` (explicit) | pass read-only và đề xuất; pass hai `init`, `docs register` | approve trước khi ghi |
| `pulse-handoff` (user-invoked, không model-invoked) | flush về plane qua lệnh `pulse`; doc live thread tại `.pulse/runtime/handoff/<node>.md`; một `note --work` con trỏ; in lệnh mở phiên mới rồi dừng | không |

Executing và reviewing không phải skill: bootstrap prompt của `pulse run` là
contract. Guard test: mọi lệnh `pulse …` trong `skills/**` và template khối
AGENTS phải parse được bằng clap của crate.

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
- Shaping map hay decision frontier lưu ngoài graph. Fog-of-war chỉ là hai mục
  prose `Not yet specified` và `Out of scope` trong `brief.md` của Epic.
- Trace tool call, prompt, token của agent; Inspector; adapter transcript cho
  nhiều host. Pulse chỉ giữ `session_ref`.
- Multi-user authorization, dashboard, report, external tracker sync.
- Điểm số harness (năm chiều, trần điểm, con số tổng hợp cho repo), renderer
  HTML/Canvas, host adapter matrix, Harness as Code, Studio. Thang bằng chứng
  §5.6 là nhãn phân loại tính từ receipt, không phải điểm (Decision 0012).
- Semantic/hybrid search, embedding, reranker.
- Windows tier-1 cho đến khi có người dùng Windows.
- Worktree mặc định.
- Đếm token, phát hiện ngưỡng context, tự spawn phiên mới. Host làm; Pulse
  chỉ cung cấp thủ tục bàn giao phiên và chỗ ghi (Decision 0013).

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

> **Đạt:** mục 1–7 đạt ngày 2026-09-05 trên `examples/todolist/`, HEAD
> `845ff01`, hai Ticket (TK-001, TK-002) đóng bằng receipt với worker và
> reviewer là agent thật; xem `examples/todolist/works/ST-001/`. Tiêu chí phụ
> chưa đạt (còn `close-story` trên baseline thật).

## 8. So với code hiện tại

| Tính năng | Hiện trạng | Việc cần làm |
|---|---|---|
| 5.1 Work graph | Đã có spine | `pulse init` cấp Core grants; Ticket tạo `ticket.md`, `work sync` bind hash/metadata, ambiguity và ready gates hoạt động. Legacy JSON contract API vẫn tồn tại cho callers cũ. |
| 5.2 Packet | Đã rút gọn | Packet có ticket prose, context, docs/QA/source/tags/handoff; không còn dispatch, capability, scope enforcement, assurance hay `not_installed`. |
| 5.3 Runner | Chạy thật trên dogfood | `pulse run worker|reviewer|qa` đã chạy thật trên `examples/todolist/` với lease, resume sau kill, drift acknowledgment, inconclusive classification; isolation chuyển sang từ chối khi Ticket khác `active` (quyết định 13.2). Artifact ingest đang làm. Còn lại theo Decision 0012: `work handoff --check/--proof`, `HandoffReceipt.checks/acceptance_proofs`; `reviewer-input.json` bỏ `summary`, thêm `contract_revision`, `reviewers_required` và claim của handoff; `classify_reviewer` validate shape finding và cờ `unverifiable`. Decision 0014: `pulse run qa` tự dựng `qa_checkpoint` với artifact đã hash. Decision 0010: `qa-input.json` mang `baseline_path`, `posture`, `variables` và case nguyên văn kèm `case_hash`; `qa-run.mjs` chỉ chạy block `pulse-check`, không đọc `qa.md`. |
| 5.4 Docs | Đã rút gọn | Registry tám trường, `tags.json`, `docs tags add/list`, tag filtering và path/tag applicability đã có. |
| 5.5 Evidence/QA | Đã có spine | Close hỗ trợ mọi risk; high/critical yêu cầu actor human. `qa.md` là markdown heading, `qa-input.json` là JSON duy nhất, receipt `qa_checkpoint` bind `baseline_content_hash` và `case_hash` (Decision 0010). Decision 0012: `src/evidence/redaction.rs` cho plane tracked; trường `reviewers` trong profile và close gate đếm receipt theo actor. |
| 5.6 Ratchet | `capture`–`applicable` đã chạy thật | `knowledge capture|validate|promote|applicable` đã chạy thật: LRN-001 được capture, promote và inject vào packet; scope `harness|repository` và promote tự sửa doc là việc còn lại (quyết định 13.3, 13.4). Decision 0012: `expected_signal` bắt buộc cho kind `ratchet`, `knowledge validate` đối chiếu nó; ba lane và luật lead sống trong skill `pulse-ratchet`, chưa có lệnh `ratchet bundle`. |
| 5.7 Giao tiếp | Đã có và chạy thật | Event log append-only; `pulse note` ghi note vào Ticket, `pulse events tail` đọc với `--since`/`--ticket`/`--follow`; note hiện trong packet (giới hạn 8 note mới nhất, mỗi note cắt 500 ký tự). Còn một file một event: chuyển sang `<date>.jsonl` và `events compact` theo Decision 0011; `--kind friction`, `session_ref` chưa có. Decision 0013: đổi cờ `--ticket` thành `--work` (code đã nhận mọi node), skill `pulse-handoff`, hook mẫu; `--kind handoff` và `work resume` là Later. |
| 5.8 Bề mặt hướng dẫn | Chưa có | Khối AGENTS có marker, `DOC-GLOSSARY`, template `brief.md` năm mục, bảy skill của Decision 0009 cộng `pulse-handoff` của 0013, đổi guard test cấm `skills/` thành guard parse lệnh. |
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

Thứ tự ưu tiên sau golden path: MCP server; `pulse doctor` offline,
deterministic, non-blocking, trả bảng bậc bằng chứng theo cơ chế (§5.6) và
envelope finding theo shape §5.3 có owner và proposed Ticket; `pulse commands
--json` với audience `workflow | advanced | maintainer` để khối AGENTS render
từ inventory và guard test kiểm tra hai chiều; `pulse ratchet bundle` chỉ khi
dogfood thấy lane rò rỉ; compound run và retrieval eval; persisted shaping map
và decision frontier cho R2/R3; Story qualification matrix; external tracker
adapter; semantic search adapter nếu lexical eval chứng minh recall gap;
conductor agent loop; Windows. Từ Decision 0012 và 0013, làm khi dogfood
thấy chậm thật: `note --kind handoff`, `pulse work resume`, gộp hai họ
receipt (`evidence/execution/*` vào envelope chung) sau khi `doctor` cần đọc
chúng chung.

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
2. **Isolation: từ chối thay vì auto-worktree.** Khi có Ticket khác đang
   `active` trong cùng repo-root, `pulse run` **từ chối** với
   `run_isolation_required` (kèm Ticket id và gợi ý `--isolation worktree`);
   worktree chỉ được tạo khi cờ này có mặt. Sửa nguyên tắc 10 và §5.3:
   worktree là opt-in theo lệnh, không auto. Chốt 2026-09-06.
3. **Learning có `scope`: `harness` hoặc `repository`.** Harness learning là
   bài học về cách dùng Pulse (ví dụ "đừng sửa file sau handoff"); repository
   learning là bài học về codebase. Harness learning không inject theo path;
   nó vào bootstrap prompt của runner role và có thể promote vào `AGENTS.md`
   của target. Repository learning inject theo path/tag/symbol như thiết kế
   §5.6. Chốt 2026-09-06.
4. **Promote phải đổi nội dung đích.** `knowledge promote --document <id>`
   chỉ ghi relation `promoted_to` khi content hash của doc khác hash lúc bắt
   đầu; không đổi thì lỗi `promotion_target_unchanged`. Mặc định
   `--insert-after "<heading>"` tự chèn đoạn text từ `guidance`/`summary` của
   learning (có `--dry-run`); `--agents-md` là đích cho harness learning.
   Chốt 2026-09-06.
5. **Bề mặt hướng dẫn quay lại, thu hẹp Decision 0007.** Quy trình sống
   trong khối `AGENTS.md` của repo đích do `pulse init` ghi; bảy skill theo
   artifact (`wayfind`, `grill`, `spec`, `tickets`, `research`, `ratchet`,
   `onboard`); không `using`, không router, không state riêng. Ma sát ghi tự
   động qua `note --kind friction`, ratchet sửa guidance không hỏi, chỉ
   validated sau rerun. Pulse không ghi trace agent, chỉ giữ `session_ref` và
   gợi ý trailer `Pulse-Ticket`. Decision 0009. Chốt 2026-09-06.
6. **`qa.md` là markdown heading, JSON chỉ ở `qa-input.json`.** Block
   `pulse-check` tuỳ chọn cho case cli/api. Receipt bind hash file và
   `case_hash`. Decision 0010. Chốt 2026-09-06.
7. **Event log là `events/<date>.jsonl`.** Append với lock và fsync, cursor
   ULID, `events compact` chuyển đổi một lần. Node, edge, receipt, learning
   giữ một file một bản ghi. Decision 0011. Chốt 2026-09-06.
8. **Thang bằng chứng, reviewer là bằng chứng, lane độc lập.** Bậc
   `present|wired|exercised|outcome_supported` tính từ receipt, không điểm.
   Reviewer input không mang lời kể của worker; finding có `owner` và
   `check`; plane tracked từ chối absolute path và secret; profile risk cao
   khai `reviewers: 2`; `pulse-ratchet` chạy ba lane `execution|harness|
   knowledge` với lead hoà giải và learning `ratchet` có `expected_signal`.
   Không lấy điểm năm chiều, renderer, Canvas, adapter matrix. Handoff mang
   claim máy đọc `--check`/`--proof`. Decision 0012. Chốt 2026-09-06.
9. **Bàn giao phiên theo ngưỡng của host.** Host đếm token và ép bằng hook;
   Pulse không đọc transcript. Skill thứ tám `pulse-handoff` user-invoked:
   flush về plane, doc live thread ở `.pulse/runtime/handoff/<node>.md`,
   một `note --work` con trỏ, in lệnh mở, dừng. `--ticket` của `note` đổi
   thành `--work`. Không HANDOFF.json, không `/tmp` cho target repo. `--kind
   handoff`, `work resume` là Later. Decision 0013. Chốt 2026-09-06.
10. **Pulse ghi `qa_checkpoint`, runner chỉ in output.** `pulse run qa`
    ingest artifact rồi dựng receipt với `bindings.artifacts`; script không
    cần grant `evidence.record`; drift baseline hay run không sạch thì
    `inconclusive` không receipt. Decision 0014. Chốt 2026-09-06.
11. **Worktree là workspace của repo chính.** Worktree sở hữu duy nhất
    source plane; mutation route về repo chính dưới một lock, nhận diện bằng
    marker `.pulse-owned` cộng đối chứng Git. Run workspace mirror vào
    worktree; reviewer và qa chạy trong cùng workspace với worker; lệch
    commit báo `worktree_graph_stale`, không tự rebase. Hai họ receipt chưa
    gộp. Decision 0015. Chốt 2026-09-07.
12. **Worker sở hữu docs receipt, gate chặn tại handoff.** Posture
    `required` mà handoff không tham chiếu `documentation_validation` thì
    fail với `handoff_documentation_receipt_missing` — lỗi lộ tại bước của
    người sửa được, không phải sau một vòng review. Reviewer input liệt kê
    docs receipt theo source commit (không theo ticket id, vốn không bao giờ
    khớp) và reviewer dùng lại thay vì tự ghi. Close gate không đổi: vẫn chỉ
    đọc proof của reviewer. Decision 0016. Chốt 2026-09-07.

Còn mở, mặc định nếu không có ý kiến khác:

1. **Agent runner đầu tiên.** Mặc định Claude Code headless.
2. **Thời điểm xoá `daemon/`.** Mặc định xoá ngay trong bước cắt code.
3. **Cách parse ticket.md.** Mặc định heading quy ước, không fenced block.
4. **Gộp `evidence/execution/*` vào `evidence/receipts/`.** Mặc định chưa gộp.

## 14. Nguồn tham khảo đã hấp thụ

OpenAI Harness Engineering (repo là môi trường thực thi, AGENTS.md là map,
guardrail cơ học, failure cải thiện harness); Symphony (tracker → normalized
packet → agent; local graph đóng vai tracker); Maestro (typed card, relation,
sidecar prose); Matt Pocock grilling/wayfinder (one-question-at-a-time,
destination, frontier, fog); QMD và Knowledge Base Builder (search/get tách,
section unit, progressive index); Paseo (đã học rồi quyết định không làm
runtime). Chi tiết thiết kế gốc nằm trong Git history của `pulse-reboot/`
trước commit xoá.

Đợt 2026-09-06 (`references/`): repository-harness (quy trình trong khối
`AGENTS.md` có marker, route theo hình dạng yêu cầu, phân loại authority
Authoritative/Observed/Derived/Decision required/Unknown, improve-harness với
baseline → earliest gap → một intervention → fresh rerun, encode-invariant,
onboarding read-only trước); Khuym (hỏi một câu một lần với recommended
answer, validating là gate cứng, gate gắn vào artifact; không lấy state.json,
HANDOFF.json, go mode); Matt Pocock skills (chuỗi grill-with-docs → to-spec →
to-tickets → implement, wayfinder với destination, map là index, decision
ticket, fog of war, research subagent; glossary ghi ngay, ADR sparingly);
Better Harness (đọc transcript host chứ không tự ghi trace, Task Episode,
ToolCallTrace không args, commit ↔ session theo trailer và trùng file, mọi claim
gắn nhãn declared/observed/candidate/unmapped).

Đợt hai Better Harness, Decision 0012 (`references/better-harness`, commit
`7cd26e9`): thang bằng chứng `Present → Wired → Exercised → Outcome-supported`
và luật "cấu hình không phải sử dụng"; finding có gap, impact, owner, check;
reviewer là bằng chứng không phải authority, không chấp nhận theo điểm trung
bình; pattern A ba lane bằng chứng độc lập với lead hoà giải chỉ merge khi
cùng target/hậu quả/owner/đường sửa; pattern B tam giác hoá nhiều reviewer
cùng prompt chỉ cho spec và Decision risk cao; repair verified tách khỏi
effectiveness; ranh giới riêng tư cho output tracked. Không lấy: điểm năm
chiều, support track như tầng báo cáo, renderer HTML/Canvas, host adapter
matrix, Harness as Code, Studio, Inspector, khối lượng prose của SKILL.md.

Matt Pocock `handoff`, Decision 0013 (`references/mattpocock/skills`,
`skills/productivity/handoff`): nén không chép, chỉ live thread, mục
suggested skills, redact, user-invoked với `disable-model-invocation`. Không
lấy: ghi vào `/tmp` cho target repo (đi ngược repository-legible context),
biến thể `claude-handoff` tự spawn phiên nền (Pulse không spawn ngoài `pulse
run`).
