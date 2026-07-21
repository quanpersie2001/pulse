# Runtime Và Repository Harness

[Trang vào](../PULSE_REBOOT.md) | [Work graph](02-work-graph.md) | [Verification ratchet](07-verification-ratchet.md) | [Documentation system](10-documentation-system.md)

**Đọc khi:** cần implement kernel, CLI, skills, scripts, hooks, events hoặc target repository layout.
**Sở hữu:** runtime layers, capability contracts, run lifecycle, policy và event model.

## Bốn tầng kiến trúc

### 1. Pulse kernel

Kernel chỉ làm deterministic mechanism:

- Đọc/ghi sharded JSON node/edge store theo schema và revision.
- Dựng inverse indexes, readiness và bounded execution packet từ graph.
- Resolve applicable durable docs từ registry, scope và Ticket impact contract.
- Validate transition, lease, evidence và documentation receipts.
- Resolve config/policy có thứ tự rõ.
- Chạy process, timeout, cancellation và log capture.
- Ghi immutable semantic event files.
- Tính gate từ rules và receipts.
- Cung cấp stable CLI/API cho agent adapters.

Kernel không shape Ticket, không chọn kiến trúc và không tự đánh giá semantic priority.

### 2. Repository harness

Mỗi target repository chứa capability cụ thể:

- `AGENTS.md` và docs map.
- Scripts build/test/lint/dev/seed/reset.
- Skills cho planning, debugging, QA và review.
- Hooks enforce invariant rẻ và deterministic.
- Verification profiles.
- Evals và regression fixtures.
- `PULSE.md` mô tả policy/judgment boundaries.

### 3. Agent adapters

Adapter biến runtime cụ thể thành một capability contract:

- Start/resume một run.
- Gửi prompt/context.
- Stream events và tool calls.
- Cancel/interrupt.
- Thu result/handoff.
- Map native thread/session ID sang Pulse identity.

Codex là first-class adapter đầu tiên. Chỉ trừu tượng hóa phần đã chứng minh chung qua usage thật.

### 4. Optional orchestration

Orchestration dùng kernel primitives để điều phối independent Agents. Nó là semantic control loop riêng, không nằm trong process runner. Xem [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md).

## Capability taxonomy

### Skills

Skill đóng gói judgment workflow và progressive context:

- `pulse-orient`: đọc map, policy và relevant code.
- `pulse-shape`: pressure-test outcome/contract còn mơ hồ, resolve critical decision branches và materialize Story/Ticket/Decision tối thiểu theo risk.
- `pulse-plan`: tạo plan risk-adaptive.
- `pulse-implement`: thực thi Ticket trong scope.
- `pulse-debug`: reproduce, isolate, fix, prove.
- `pulse-qa`: ground Story contract/risk, duy trì coverage, chọn affected/full cases, resolve environment/executor, chạy và tạo receipt/finding.
- `pulse-review`: kiểm tra correctness, regression, security, missing tests.
- `pulse-reconcile`: phân tích priority/dependency/supersession.
- `pulse-improve-harness`: biến failure class thành guardrail/eval.
- `pulse-compound`: gather completed-work evidence, synthesize/deduplicate learnings, classify applicability/confidence, route promotion và publish compound summary.
- Documentation capability references: orient, impact, update, review và promote theo [`10-documentation-system.md`](10-documentation-system.md).
- Knowledge capability references: capture, review, validate, search, applicable recall, promote và retire theo [`12-knowledge-compounding.md`](12-knowledge-compounding.md).

Skill không nên chứa một bản sao dài của repository docs. Nó chỉ ra cách khám phá và output contract.

### Shaping và decision-tree grilling primitive

`pulse-shape` sở hữu public shaping capability. Bên trong capability này, Pulse dùng một reusable **decision-tree grilling primitive** để tìm và giải quyết các soft spot trước khi work trở thành executable. Đây là judgment process của Agent, không phải kernel state machine và không tạo thêm một public phase bắt buộc.

Primitive phải tuân các nguyên tắc:

1. **Ground before asking.** Đọc owning work context, applicable Decisions, durable docs, code anchors và recent evidence trước; không hỏi human điều repository đã trả lời được.
2. **Walk dependencies, not a questionnaire.** Xác định decision tree và giải quyết parent decision trước các lựa chọn phụ thuộc. Mỗi lượt chỉ hỏi một câu để câu trả lời có thể thay đổi nhánh tiếp theo.
3. **Ask only across the authority boundary.** Human trả lời intent, preference, risk appetite, irreversible trade-off và approval. Agent tự resolve fact có evidence và reversible choice đã nằm trong implementation freedom.
4. **Recommend, do not present a blank page.** Khi có strong default, câu hỏi phải kèm recommended answer và rationale ngắn để human confirm hoặc override.
5. **Track branch disposition.** Mỗi nhánh planning-critical kết thúc ở một trong năm trạng thái semantic: `resolved`, `rejected`, `delegated`, `deferred` hoặc `blocking`.
6. **Stop at the right boundary.** Shaping làm rõ outcome, acceptance, scope, constraints, shared direction và authority; không giả làm exact execution plan hoặc khóa reversible implementation detail.
7. **Materialize by risk.** Conversation không tự trở thành source of truth. Kết quả được ghi vào đúng owner: Epic/Story outcome, Story `approach.md`, Ticket contract, Decision, linked Discovery/Spike Ticket hoặc durable docs promotion candidate.

`delegated` chỉ hợp lệ khi work contract ghi rõ implementation freedom và lựa chọn không thể làm đổi objective, acceptance, invariant hoặc public contract. `deferred` phải có owner/target, reason và trigger hoặc linked work; nếu deferral có thể làm implementation hiện tại sai, nhánh đó phải là `blocking`.

#### Khi nào kích hoạt

- **Skip/full-pass không phải binary phase:** R0 work rõ và low-risk có thể chỉ cần một short self-check không hỏi human.
- Dùng focused grilling khi có nhiều direction hợp lệ, domain language chưa ổn định, acceptance/scope còn implicit, hoặc Agent sắp tối ưu solution trước khi outcome rõ.
- Dùng persisted shaping map khi effort lớn hơn một reasoning session, nhiều decisions phụ thuộc nhau, resolution dự kiến làm lộ câu hỏi mới, hoặc cần nhiều human/Agent cùng resume/audit.
- Dùng sâu hơn cho R2/R3, architecture, migration, security, destructive change hoặc lựa chọn khó đảo ngược.
- Trong planning/reconcile/triage, capability khác có thể gọi cùng primitive để pressure-test quyết định của mình thay vì reimplement interview logic.
- Trong execution, Worker không tự mở rộng shaping. Nếu phát hiện critical ambiguity ngoài implementation freedom, Worker dừng và gửi `decision_request` hoặc đề xuất re-shape/requeue.

#### Destination và shaping map

Một persisted shaping effort phải khóa **destination** trước khi fan out decision work. Destination mô tả trạng thái cuối cần đạt, boundary của effort và exit condition để biết khi nào đường đi đã đủ rõ. Nó không phải implementation plan và không được dùng như khẩu hiệu mơ hồ.

Shaping map là một **index**, không phải store thứ hai. Nó có thể materialize trong owning Epic/Story `approach.md` hoặc một referenced shaping artifact theo policy, nhưng chỉ giữ low-resolution view và pointers tới canonical Decision/Ticket/resolution:

```markdown
## Destination
<target state, scope boundary, shaping exit condition>

## Decisions so far
- [canonical Decision/work link] — one-line gist

## Decision frontier
- [open, unblocked, unclaimed decision work links]

## Not yet specified
- <bounded in-scope fog chưa thể phát biểu thành actionable question>

## Out of scope
- <consciously excluded relative to destination, with rationale/link when needed>
```

Quy tắc ownership:

- canonical answer sống ở đúng Decision, research/prototype resolution hoặc owning work contract; map chỉ gist + link;
- một câu hỏi đủ sắc nét thì trở thành typed decision work, kể cả khi đang blocked;
- một vùng uncertainty chưa thể viết thành precise question ở `not_yet_specified`, không pre-slice thành speculative Tickets;
- out-of-scope không phải fog và không tự graduate trừ khi destination được human-authorized redraw;
- map revision phải gắn source work revisions để stale shaping result không mở readiness sai.

#### Decision frontier và routing

Decision frontier là tập decision work `open + unblocked + unclaimed` có thể xử lý ngay để làm rõ đường tới destination. Nó khác execution frontier: decision frontier làm rõ contract; execution frontier chứa implementation Tickets đã qua ready gate.

Mỗi uncertainty sắc nét được classify trước khi route:

| Gap | Default route | Authority/output |
|---|---|---|
| `fact_gap` | Research/Discovery Ticket | Agent thu evidence; không hỏi human fact có thể tìm |
| `intent_gap` | Grilling question/session | Human xác nhận outcome/preference |
| `tradeoff_gap` | Decision proposal | Required authority approve hard-to-reverse choice |
| `fidelity_gap` | Prototype/Spike Ticket | Tạo artifact rẻ để human phản hồi |
| `prerequisite_gap` | Enabling Ticket | Hoàn thành setup/access/data cần cho decision |

Các loại này là decision-work roles, không bắt buộc thêm top-level node kinds mới trong Core. Pulse có thể dùng Ticket subtype/labels + relations tới owning Story/Decision question; accepted hard-to-reverse resolution vẫn materialize thành Decision.

#### Fog-of-war và progressive discovery

`not_yet_specified` chỉ chứa uncertainty thỏa cả ba điều kiện: in scope theo destination, có khả năng ảnh hưởng đường đi, nhưng chưa thể phát biểu thành precise actionable question. Nó không được dùng để giấu blocker đã biết, backlog ý tưởng chung hoặc work cố tình defer.

Phân biệt:

- `blocking`: precise question đã biết và phải resolve trước execution;
- `deferred`: precise question đã biết nhưng current slice vẫn đúng khi chưa resolve, có owner/trigger;
- `not_yet_specified`: chưa đủ evidence để biết câu hỏi chính xác là gì;
- `out_of_scope`: đã chủ động loại khỏi destination hiện tại.

Pulse không yêu cầu complete map upfront. Chỉ chart visible frontier và bounded fog. Sau mỗi resolution, `pulse-shape` phải chạy shaping reconciliation:

```text
persist resolution/evidence
  -> update Decisions so far pointer
  -> reconcile affected branches/dependencies
  -> graduate newly precise fog into decision work
  -> reject/cancel/supersede invalidated branches
  -> recompute decision frontier
  -> recompute implementation readiness
```

Mỗi mutation phải qua normal graph CAS/audit. Kernel tính graph projection; Agent skill quyết định semantic graduation, invalidation và rationale. Một resolution không hoàn tất nếu chỉ viết comment nhưng không reconcile downstream map/readiness mà nó ảnh hưởng.

#### Output contract

Một shaping result phải cho biết:

- work boundary, destination và shaping exit condition đã xác nhận khi persisted map được yêu cầu;
- acceptance/scope/invariants ở độ chi tiết phù hợp;
- các direction đã cân nhắc, lựa chọn và trade-off quan trọng;
- branch disposition cho mọi critical ambiguity còn liên quan;
- decision frontier, blocked decision work và bounded `not_yet_specified` khi effort cần map;
- authority/approval đã dùng;
- artifact hoặc linked work được tạo/cập nhật;
- reconciliation effects: newly surfaced, rejected, cancelled hoặc superseded branches;
- mức materialization/risk đề xuất và lý do;
- remaining non-blocking uncertainty cùng trigger quay lại shaping.

Output không bắt buộc là một `work-brief.md` riêng. R0 có thể cập nhật trực tiếp concise Ticket contract; Story-level direction có thể thuộc Story/`approach.md`; hard-to-reverse choice thuộc Decision. Điều bắt buộc là canonical work graph và content owner phản ánh shared understanding, không để kết luận chỉ nằm trong chat.

### Scripts

Script thuộc repository hoặc Pulse package khi cần kết quả deterministic:

- Bootstrap/config discovery.
- Schema validation.
- Build/lint/test/check.
- Start/stop local environment.
- Fixture/seed/reset.
- Evidence hashing và receipt validation.
- Worktree create/cleanup.
- Graph query và CAS mutation.

Script phải có exit code, timeout behavior và machine-readable output ổn định.

### Tools/adapters

Tool nối capability ngoài process thông thường:

- Git/worktree.
- Browser/Playwright/Chrome DevTools.
- Structured HTTP/API contract runner.
- Shell/PTY CLI runner.
- Platform/mobile/desktop automation.
- Data query/reconciliation/rollback runner.
- GitHub/Jira/Linear sync.
- Secret provider.
- Agent thread transport.
- Artifact storage.

Tool phải khai báo capability, supported surfaces, permission, side effects, environment requirements, artifact types, timeout/cancellation, redaction và failure taxonomy.

QA executor không được chọn chỉ bằng tên tool. Resolver dùng `surface + required capabilities + environment applicability + evidence requirements`; deterministic executor được ưu tiên cho critical assertions, semantic/manual fallback chỉ dùng khi policy cho phép. Contract chi tiết thuộc [`03-story-qa.md`](03-story-qa.md).

### QA environments, fixtures và executors

Target repository sở hữu QA capability config:

- Environment profiles: start, healthcheck, seed/reset, stop/cleanup, platform/config identity.
- Surface profiles: web, API, CLI/TUI, SDK, desktop/mobile, data/migration.
- Executor manifests: Playwright/browser-agent/Chrome DevTools, HTTP contract, shell/PTY, consumer fixture, platform automation, query/reconciliation hoặc structured manual.
- Policy: independence, critical evidence, TTL, retry/flaky, waiver và required matrix.

Environment adapter phải fail rõ ở start/healthcheck/reset thay vì để case bị hiểu nhầm thành product failure. Fixture identity và reset result là một phần receipt validity. Story close trên preview/deployed environment phải bind source snapshot với immutable build/deployment artifact ID.

### Hooks

Hook chỉ dành cho guardrail rẻ, nhanh, ít false-positive:

- Chặn forbidden import/path.
- Validate generated files/schema.
- Kiểm tra ticket/commit binding nếu policy yêu cầu.
- Redact secret trước khi lưu evidence.
- Nhắc verification bắt buộc trước transition.

Không dùng hook để chạy full suite hoặc bắt agent tuân một style reasoning cụ thể.

### Evals

Eval đo harness, không chỉ model:

- Agent có tìm đúng docs/entrypoint không?
- Có chọn đúng verification profile không?
- Có phát hiện dependency/supersession không?
- Có tạo receipt valid và source-bound không?
- Recovery có tiếp tục đúng sau process/agent crash không?

## Public CLI

CLI là đường đọc/ghi work graph bắt buộc cho Agent và Orchestrator. Raw JSON/Markdown tồn tại để Git và human inspect; Agent không tự `find`, `grep` hoặc reconstruct graph semantics từ filesystem.

```text
pulse init
pulse doctor [--json]

pulse work list|show|create|edit|ready|close
pulse work packet <ticket-id> [--json]
pulse work children <id> [--json]
pulse work frontier --kind decision|execution [--for <epic-or-story-id>] [--json]
pulse work claim|release

pulse graph edge add|remove
pulse graph neighborhood <id> [--depth N] [--json]
pulse graph affected-by <id> [--json]
pulse graph validate [--json]
pulse graph export [--json]

pulse docs list|show
pulse docs index|status
pulse docs search <query> [--json]
pulse docs get <document-or-section-ref> [--json]
pulse docs tree [path] [--json]
pulse docs applicable --work <id> [--json]
pulse docs impact <ticket-id> [--json]
pulse docs validate [--changed] [--json]

pulse compound <work-id> [--include-children]
pulse compound --run <run-id>
pulse compound status <work-id> [--json]
pulse compound review --candidates [--json]

pulse knowledge create|edit|list|show
pulse knowledge capture --from-work <id>|--from-run <run-id>
pulse knowledge review|validate|promote|supersede|retire <learning-id>
pulse knowledge index|status
pulse knowledge search <query> [--json]
pulse knowledge get <learning-id> [--summary] [--json]
pulse knowledge applicable --work <id> [--audience <role>] [--moment <moment>] [--json]

pulse run <ticket-id> [--agent codex]
pulse run resume <run-id>
pulse run cancel <run-id>

pulse verify [profile] [--source <snapshot>]
pulse qa plan <story-id> [--ticket <ticket-id>] [--json]
pulse qa cases <story-id> [--applicable] [--json]
pulse qa impacted --ticket <ticket-id> [--json]
pulse qa run <story-id> --scope ticket-checkpoint --ticket <ticket-id> [--case <id>] [--executor <name>]
pulse qa run <story-id> --scope story-close [--case <id>] [--executor <name>]
pulse qa receipt verify <receipt-id> [--json]
pulse review <ticket-id|source>

pulse evidence show|verify <receipt-id>
pulse events tail [--json]
pulse eval [suite]
```

Orchestration commands chỉ thêm khi transport/lease contracts ổn định:

```text
pulse agent list|show|send|wait|interrupt
pulse orchestrate start|resume|status
```

## Single-agent run lifecycle

```text
request Ticket execution packet from CLI
  -> validate ready/dependencies/policy
  -> acquire assignment lease
  -> create isolated workspace when required
  -> build bounded execution packet
  -> start/resume Agent
  -> stream events and checkpoints
  -> collect handoff
  -> run developer verification + review
  -> run targeted Ticket QA checkpoint khi impact/policy yêu cầu
  -> close/rework/blocked/requeue Ticket
  -> khi Story có integrated candidate: run full Story qualification rồi close/rework Story
  -> continuously capture learning candidates from handoff/review/QA/failure
  -> khi policy/cycle yêu cầu: compound, promote và refresh applicable knowledge index
  -> classify failures and propose harness improvements
```

### Bounded execution packet

`pulse work packet` resolve graph và content references thành một versioned packet. Packet chỉ chứa context cần cho Ticket:

- Work item IDs và revisions.
- Objective, current/target behavior, implementation contract, scope và acceptance.
- Parent Story/Epic summaries, applicable Decisions và shared approach.
- Forward/reverse edges, blocker states và supersession.
- Code anchors, required changes và invariants.
- Shaping result/receipt, destination, critical branch dispositions, decision-frontier summary, bounded fog refs, authority/approval và remaining non-blocking uncertainty.
- Required/optional/write-candidate docs, authority, owner và content hashes.
- Ranked section refs, summaries/snippets và recommended initial read budget từ docs retrieval.
- Applicable knowledge buckets, typed match reasons, required ratchet checks, promotion targets, knowledge fingerprint và role-specific prompt budget.
- Allowed source/docs writable scope.
- Documentation impact posture và promotion/defer policy.
- Learning candidate capture policy, required compound posture và escalation khi knowledge contradicts current docs/Decision.
- Implementation mode, plan policy, verification profile, QA impact/affected cases/checkpoint policy và docs profile.
- Capability permissions.
- Source/workspace identity.
- Hard stops và escalation conditions.

Prompt builder nhận packet trực tiếp. Agent chỉ search source repository cho implementation discovery; nó không search `.pulse/workgraph` để tìm Ticket hoặc tự tính dependency.

### Risk-adaptive run

- Risk thấp: in-place workspace có guardrail, focused checks.
- Risk vừa: isolated worktree, plan ngắn, focused + regression checks.
- Risk cao: Decision/design, dedicated worktree, review độc lập, rollout/rollback proof.

Risk policy có thể nâng mức trong run; mọi downgrade phải có audit.

## State model

Pulse tách ba lớp:

| Lớp | Ví dụ | Đặc tính |
|---|---|---|
| Durable documentation | `docs/`, `AGENTS.md`, `PULSE.md` + docs registry | Git-diffable, owned, authority/freshness-aware |
| Canonical work | sharded JSON nodes/edges + top-level `works/` contracts | Durable, Git-diffable, CAS graph mutation |
| Runtime coordination | agent presence, lease, heartbeat, wait cursor | Shared local, gitignored, rebuildable, TTL |
| Evidence | receipts, logs, traces, screenshots, diffs | Immutable, content-addressed |

Một message “done” không đổi canonical work. Một Agent biến mất không xóa Ticket. Một receipt không được sửa để khớp source mới.

## Event envelope

```json
{
  "event_version": 1,
  "event_id": "evt_01J...",
  "type": "ticket.handoff_submitted",
  "occurred_at": "2026-07-18T02:00:00Z",
  "actor": {"kind": "agent", "id": "agent_codex_17"},
  "subject": {"kind": "ticket", "id": "TK-031", "revision": 7},
  "correlation": {"run_id": "run_01J...", "lease_id": "lease_01J..."},
  "payload": {"receipt_id": "handoff_01J..."}
}
```

Mỗi semantic event là một immutable file `.pulse/events/<date>/<event-id>.json`. Không dùng một tracked monthly JSONL vì concurrent append giữa worktrees tạo merge hotspot. Raw high-volume runtime logs có thể gitignored; secrets và raw prompts nhạy cảm phải được redact hoặc lưu qua protected artifact reference.

## Repository policy contract

Target repo có thể khai báo `PULSE.md`:

```markdown
# Repository intent
- Ưu tiên backward compatibility cho public API.
- Không chạy migration destructive khi thiếu rollback proof.

# Human judgment boundaries
- Human phải approve thay đổi auth model, billing và production deploy.
- Agent được tự sửa focused test và docs trong Ticket scope.

# Verification profiles
- docs-only
- service-change
- web-behavior
- migration-high-risk
```

Policy mô tả intent/quyền. Command cụ thể nằm trong config/scripts có schema.

## Target repository layout

```text
AGENTS.md
PULSE.md
.pulse/
  config.yaml
  workgraph/
    manifest.json
    schemas/
    nodes/
    edges/
  docs/
    registry.json
    schemas/
  knowledge/
    manifest.json
    schemas/
    entries/
    relations/
  events/
  evidence/
  cache/                 # workgraph/docs/knowledge indexes; gitignored, disposable
  skills/
    pulse-orient/SKILL.md
    pulse-qa/SKILL.md
  scripts/
    verify.mjs
    dev-start.mjs
  hooks/
  evals/
docs/
  product/
  architecture/
  domain/
  operations/
works/
  EP-001/
  ST-014/
  TK-031/
```

Documentation taxonomy, registry, authority và validation contract thuộc [`10-documentation-system.md`](10-documentation-system.md). Generated navigation, section-level search/get, lexical cache và optional semantic adapter thuộc [`11-documentation-retrieval.md`](11-documentation-retrieval.md).

Shared live coordination state nằm trong một repository-scoped local control directory, không nằm trong tracked work graph và không dùng SQLite ở v1. `AGENTS.md` chỉ là map. `PULSE.md` là policy. `docs/` là durable current knowledge. `works/` là active work prose. `.pulse/knowledge/` giữ reusable learning metadata/provenance, không phải current product truth. `.pulse/config.yaml` là machine-readable config. Không file nào cố sở hữu nhiều vai trò.
