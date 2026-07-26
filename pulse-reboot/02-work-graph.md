# Local Work Graph

[Trang vào](../PULSE_REBOOT.md) | [Bản đồ tài liệu](README.md) | [Priority reconciliation](06-priority-reconciliation.md) | [Documentation system](10-documentation-system.md)

**Đọc khi:** cần biết Epic, Story, Ticket, Decision và typed relations được lưu, đọc, sửa và materialize như thế nào.
**Sở hữu:** canonical work model, Git-native storage, CLI query contract, artifacts, lifecycle và mutation rules.

## Tại sao cần work graph

Ticket là đơn vị executable, nhưng Ticket không đủ để giữ ý định dài hạn. Một thay đổi lớn còn cần outcome, design, approach, behavioral baseline, quyết định và nhiều nhánh implementation.

```text
Epic: outcome lớn và ranh giới đầu tư
  -> Story: lát cắt hành vi có thể chứng minh
       -> Ticket: đơn vị một Agent có thể thực thi và verify

Decision: quyết định kiến trúc hoặc nghiệp vụ, liên kết vào bất kỳ node nào
```

Hierarchy là tùy chọn. Một Ticket độc lập vẫn hợp lệ. Dependency, scheduling preference, relation và supersession là typed edges riêng, không suy ra từ parent-child.

## Quyết định storage

Pulse dùng một **sharded JSON graph store**, không dùng một `graph.json` duy nhất và không dùng SQLite làm canonical store.

Lý do:

- JSON có parser/schema rõ, không có ambiguity của YAML.
- Một file cho mỗi node/edge tạo diff nhỏ và merge tốt hơn.
- Hai Agent sửa hai Tickets khác nhau không cùng chạm một file tổng.
- Edge tách khỏi node để Orchestrator thêm dependency mà không conflict với Worker update Ticket metadata.
- Human vẫn inspect, diff, blame và recover bằng Git.
- CLI có thể build full graph deterministic mà không cần database.

Một full `graph.json` vẫn tồn tại dưới dạng output/cache có thể rebuild, không phải writable truth.

## Canonical layout

```text
.pulse/
  workgraph/
    manifest.json
    schemas/
      node.schema.json
      edge.schema.json
    nodes/
      EP-001.json
      ST-014.json
      TK-031.json
      DEC-006.json
    edges/
      parent--ST-014--EP-001.json
      parent--TK-031--ST-014.json
      blocked-by--TK-031--TK-029.json
      preferred-after--TK-031--TK-030.json
  docs/
    registry.json
    schemas/
      document.schema.json
  events/
    2026-07-18/
      evt-01J....json
  evidence/
    receipts/<receipt-id>.json
    artifacts/<content-hash>/...
  knowledge/
    manifest.json
    schemas/
    entries/LRN-001.json
    relations/
  cache/
    workgraph.snapshot.json
    knowledge-search/

works/
  EP-001/
    brief.md
    design.md
  ST-014/
    story.md
    approach.md
    qa.md
  TK-031/
    ticket.md
    plan.md
    validation.md
```

Top-level `works/` là human-facing work content. `.pulse/workgraph/` chỉ giữ machine graph metadata; durable repository knowledge nằm trong `docs/`, `AGENTS.md` và `PULSE.md` theo [`10-documentation-system.md`](10-documentation-system.md).

Git ownership:

- `workgraph/manifest.json`, `schemas`, `nodes`, `edges` là tracked canonical graph truth.
- Top-level `works/` là tracked human-facing work content được node reference qua `content_dir`.
- Semantic transition/audit events là immutable JSON files; không append mọi Agent vào cùng một JSONL.
- Receipt metadata có thể track; artifact lớn tuân retention/storage policy.
- `cache/` luôn gitignored và xóa được.
- Agent presence, leases, mailbox cursor và process state không nằm trong tracked graph; xem [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md).

## Manifest

`manifest.json` chỉ chứa contract hiếm thay đổi, không chứa danh sách mọi node:

```json
{
  "schema_version": 1,
  "graph_id": "pulse-main",
  "node_schema": "schemas/node.schema.json",
  "edge_schema": "schemas/edge.schema.json",
  "content_root": "../../works",
  "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
}
```

Không lưu global counter/revision thường xuyên trong manifest vì nó sẽ biến thành merge hotspot. Full graph revision là fingerprint được derive từ sorted node/edge content hashes.

## Node contract

Một implementation Ticket node theo node schema baseline v1:

```json
{
  "schema_version": 1,
  "id": "TK-031",
  "kind": "ticket",
  "revision": 9,
  "contract_revision": 4,
  "title": "Phân loại lỗi refresh token",
  "status": "ready",
  "priority": "P0",
  "risk": "medium",
  "materialization": "R2",
  "content_dir": "works/TK-031",
  "role": "implementation",
  "implementation": {
    "mode": "guided",
    "work_surface": "code",
    "plan_policy": "worker_optional",
    "verification_profile": "service-change",
    "brief": {
      "path": "works/TK-031/ticket.md",
      "content_hash": "sha256:..."
    }
  },
  "qa": {
    "impact": {
      "posture": "required",
      "behavioral_owner": "ST-014",
      "affected_case_ids": ["QA-001", "QA-004"]
    }
  },
  "created_at": "2026-07-18T01:00:00Z",
  "updated_at": "2026-07-18T02:00:00Z"
}
```

Normative fields tối thiểu: `schema_version`, `id`, `kind`, `revision`,
`contract_revision`, `title`, `status`, `content_dir`, timestamps. Kind-specific
fields được JSON Schema validate. Ticket role là `implementation` hoặc
`decision_work`; không thêm top-level work kind thứ năm.

`revision` là CAS revision cho mọi node mutation. `contract_revision` chỉ tăng
khi semantic execution/shaping input đổi: role, risk/materialization,
implementation hoặc decision-work contract, docs/QA impact, required
Decision/approach reference hoặc contract-owned content binding. Status,
timestamp và current shaping pointer chỉ tăng `revision`; nếu chúng cũng tăng
`contract_revision`, shaping apply hoặc ready transition sẽ tự làm proof vừa
được dùng trở thành stale.

Bootstrap và create/edit path chỉ ghi node schema hiện tại (`schema_version: 1`).
`risk` và `materialization` có thể dùng `unassessed` như giá trị enum domain khi
classification chưa đủ chắc; đây là trạng thái explicit của contract, không phải
default được suy ra từ bytes on-disk cũ. Khi repository schema hoặc canonical
node bytes không khớp embedded current templates/schema, kernel refuse drift theo
default-deny và chỉ ghi đè qua thao tác chủ ý có approval/backup/policy; không
tự nâng cấp hoặc đoán missing fields.

Node không embed child list, inverse relations, frontier list hoặc readiness
boolean. Những giá trị đó được derive từ edge/evidence/docs/policy projections
để tránh hai nguồn sự thật.

## Edge contract

```json
{
  "schema_version": 1,
  "id": "blocked-by--TK-031--TK-029",
  "type": "blocked_by",
  "from": "TK-031",
  "to": "TK-029",
  "revision": 1,
  "created_at": "2026-07-18T01:30:00Z",
  "created_by": "human:quannv"
}
```

Edge ID là deterministic từ `(type, from, to)`, nên retry `edge add` là idempotent.

| Type | `from` | `to` | Reverse projection |
|---|---|---|---|
| `parent` | child | parent | `children` |
| `blocked_by` | dependent | blocker | `blocks` |
| `preferred_after` | work chạy sau | foundation chạy trước | `preferred_before` |
| `superseded_by` | work cũ | work hấp thụ | `supersedes` |
| `related` | ID nhỏ hơn | ID lớn hơn | symmetric |
| `duplicates` | duplicate | canonical candidate | `has_duplicate` |

Rules:

- Một node có tối đa một live `parent` edge.
- `blocked_by`, `parent` và `superseded_by` không được tạo cycle theo rules của từng loại.
- `related` được canonicalize theo thứ tự ID để hai caller không tạo hai edge ngược nhau.
- Dangling edge làm `graph validate` fail.
- Reverse edges chỉ là query projection, không được persist lần hai.

## Work item contracts

### Epic

Epic giữ outcome, success signals, scope boundary, stakeholders, constraints, major risks và links tới design/Decision. Epic không chứa checklist implementation chi tiết.

### Story

Story là một capability hoặc behavioral slice có thể chứng minh. Nó giữ user/system outcome, acceptance ở mức hành vi, shared approach, behavioral QA baseline và close gate. `approach.md` sở hữu design dùng chung cho nhiều Tickets; `qa.md` sở hữu QA scope, acceptance/risk coverage matrix, persistent behavioral cases, applicability và Story qualification exit criteria.

Story là default owner của behavioral baseline. Child Ticket reference Story case IDs và khai báo QA impact thay vì copy expected behavior vào một baseline khác. Receipt là immutable evidence dưới `.pulse/evidence/`, không được ghi đè vào `qa.md`.

### Ticket

Ticket là executable unit nhỏ nhất mà một Agent có thể nhận lease. Nó phải đủ rõ để Agent không phát minh lại objective hoặc đoán vùng code cần thay đổi.

Ticket có hai typed roles:

- `implementation`: thay đổi repository/product/harness theo executable contract;
- `decision_work`: resolve một precise `fact_gap|intent_gap|tradeoff_gap|fidelity_gap|prerequisite_gap` cho destination, có precise question, expected output/evidence và provenance branch/fog khi có.

Decision-work Ticket không bị buộc phải có implementation anchors/invariants và
có thể vào decision frontier từ `draft` khi precise, structurally executable và
unblocked; nếu bắt nó phải có một shaping receipt riêng trước khi shape thì tạo
recursive readiness loop. Implementation Ticket mới dùng ready gate đầy đủ.

Implementation Ticket có hai lớp:

1. `ticket.md` là approved implementation brief: objective, current/target behavior, work surface, code/equivalent anchors, required changes, invariants, acceptance, scope, implementation freedom, required Decisions/shared approach, verification profile, expected evidence và handoff.
2. `plan.md` là exact execution plan do Worker materialize sau khi đọc current checkout; `plan_policy=none|worker_optional|required_before_execution` chỉ khai báo lúc nào Phase 2 phải materialize nó.

Story/Ticket shaping phải gán disposition cho mọi critical ambiguity trước `ready`. Pulse không bắt buộc một artifact brainstorm riêng: shared direction thuộc Story/`approach.md`, implementation contract thuộc Ticket, hard-to-reverse choice thuộc Decision, còn unknown cần evidence thuộc linked Discovery/Spike Ticket. Kỹ thuật shaping/grilling thuộc [`04-runtime-harness.md`](04-runtime-harness.md); file này sở hữu canonical disposition và ready semantics.

### Decision

Decision ghi context, options, decision, consequence và supersession. Nó không phải Ticket trá hình. Nếu cần implementation, tạo Ticket liên kết. Hard-to-reverse Decision chỉ thỏa ready gate khi có immutable acceptance proof bind Decision ID, `contract_revision`, content hash, accepted outcome và actor có `decision.accept`; node tồn tại hoặc shaping approver nhắc tới Decision chưa phải acceptance.

### Shaping map và decision work

Một Epic/Story có thể reference persisted shaping map khi decision space lớn, nhiều dependency hoặc cần resume qua nhiều session. Map là human-facing index dưới owning `content_dir`, thường là section trong `approach.md` hoặc file `shaping.md` khi risk policy yêu cầu; node metadata chỉ giữ identity/revision/reference cần cho query và gate.

Map sở hữu:

- destination, scope boundary và shaping exit condition;
- pointers/gists tới accepted resolutions;
- bounded `not_yet_specified` và out-of-scope statements;
- read projection của decision frontier.

Map không sở hữu lại full Decision rationale, research evidence, prototype artifact hoặc Ticket contract. Các nội dung đó sống ở canonical node/content/receipt tương ứng.

Decision work dùng existing graph primitives thay vì tạo một hierarchy thứ hai:

- precise research/prototype/enabling question là Ticket có role/subtype tương ứng;
- human grilling question có thể là shaping branch trong map hoặc Ticket khi cần claim/resume/dependency riêng;
- hard-to-reverse accepted answer là Decision;
- decision work link về owning Epic/Story bằng `parent`/`related`; prerequisite dùng `blocked_by`;
- implementation Ticket có thể `blocked_by` Decision hoặc decision-work Ticket chưa resolve.

CLI derive hai projection riêng:

- **decision frontier:** precise decision work ở `draft|shaped|ready`, structurally executable, unblocked và phục vụ một destination;
- **execution frontier:** implementation Tickets status `ready` có current readiness pass; đây vẫn chưa phải dispatch authorization.

Claim/lease vẫn dùng runtime coordination contract; `unclaimed` không được persist như một status giả trong canonical graph. Trước Phase 2 lease resolver, frontier output phải ghi `claim_state=not_evaluated`, không giả `unclaimed=true`. Frontier là deterministic derived projection có relevant graph/shaping/readiness fingerprint, không phải writable list trong Markdown. Priority/foundation ordering thuộc semantic reconciliation, không được kernel biến thành hidden score.

`not_yet_specified` không phải node và không được coi là blocker tự động. Khi evidence làm một fog statement đủ sắc nét, shaping reconciliation tạo canonical decision work bằng CAS, gắn provenance về fog entry/map revision và cập nhật map. Nếu fog lộ ra blocker cho current execution, affected implementation Ticket mất readiness hoặc chuyển `blocked` theo transition policy.

## Nội dung Ticket

`works/TK-031/ticket.md`:

```markdown
# TK-031 Phân loại lỗi refresh token

## Objective
Phân biệt token hết hạn với token không hợp lệ để client xử lý đúng.

## Current behavior
`RefreshTokenHandler` đang map cả hai trường hợp thành `InvalidToken`.

## Target behavior
- Token hết hạn thành `TokenExpired`.
- Token giả mạo/revoked vẫn là `InvalidToken`.

## Implementation contract

### Code anchors
- `src/auth/RefreshTokenHandler.ts`
- `src/auth/errors.ts`
- `src/http/AuthErrorMapper.ts`
- `tests/auth/refresh-token.test.ts`

### Required changes
- Bổ sung domain error cho token hết hạn.
- Giữ nguyên public response envelope.
- Thêm contract tests cho expired và tampered token.

### Invariants
- Không trả chi tiết xác thực nhạy cảm.
- Không thay đổi refresh-token rotation.
- Client cũ vẫn xử lý được generic invalid-token path.

### Implementation freedom
`guided`: Agent chọn internal structure nhưng không đổi public contract.

## Scope
- Domain error, HTTP mapping và contract tests.

## Non-scope
- UI đăng nhập.

## Acceptance
- Hai failure modes có mã lỗi ổn định và không leak thông tin.

## Verify
- `pnpm test auth --runInBand`

## Documentation impact
- Posture: `required`.
- Applicable: `DOC-AUTH-ARCH`, `DOC-AUTH-DOMAIN`.
- Required update: ghi durable token failure taxonomy.

## QA impact
- Behavioral owner: `ST-014`.
- Posture: `required`.
- Affected cases: `QA-001`, `QA-004`.
- New proposed cases: `QA-009`.
- Checkpoint: `required`.
- Reason: thay đổi refresh-token recovery và public error mapping.

## Expected handoff
- Source snapshot, acceptance-to-evidence mapping, QA checkpoint receipts, documentation findings và compatibility risks.
```

Implementation modes:

- `locked`: phải theo Decision/approach đã khóa; lệch phải gửi `decision_request`.
- `guided`: anchors, required changes và invariants đã rõ; Agent chọn chi tiết.
- `open`: Agent được chọn approach trong boundary đã ghi. Nếu uncertainty có thể đổi objective, acceptance, invariant, public contract hoặc irreversible direction, shape thành Discovery/Spike Ticket hoặc Decision trước.

Critical decision branches dùng disposition semantic sau; projection có thể nằm trong content/Decision/linked work thay vì bắt buộc embed một mảng mới vào node schema:

| Disposition | Nghĩa | Điều kiện hợp lệ |
|---|---|---|
| `resolved` | Đã chọn direction/constraint | Canonical artifact ghi lựa chọn, rationale cần thiết và authority |
| `rejected` | Nhánh đã xem xét nhưng loại | Lý do đủ để downstream không mở lại vô cớ |
| `delegated` | Worker được quyền chọn | Nằm rõ trong implementation freedom và không thể đổi contract/invariant |
| `deferred` | Chưa cần giải quyết trong phạm vi execution hiện tại | Có reason, owner/target và trigger hoặc linked work; không làm current implementation sai |
| `blocking` | Chưa thể dispatch an toàn | Ticket không được `ready` cho tới khi resolve, supersede hoặc chuyển thành discovery work phù hợp |

`plan.md` chỉ materialize khi policy yêu cầu hoặc Worker cần durable plan. `validation.md` ghi developer verification và proof của Ticket; nó không thay `Story/qa.md`.

Mỗi behavior-affecting Ticket phải khai báo QA impact:

| Posture | Nghĩa | Close implication |
|---|---|---|
| `required` | Ticket ảnh hưởng material behavior/risk và cần targeted QA checkpoint | Required affected/new cases phải có valid receipt trước Ticket close |
| `covered_by_story_close` | Ticket chưa tạo runnable/material slice hoặc checkpoint không economical | Có rationale + behavioral owner Story + approval grant `qa.defer_to_story_close`; full Story qualification vẫn bắt buộc |
| `none` | Không ảnh hưởng behavioral/public-risk contract | Có rationale + approval grant `qa.none.approve`; developer verification vẫn bắt buộc |
| `unknown` | Chưa phân tích impact | Không được `ready` |

QA impact reference canonical Story case IDs, new proposed cases và checkpoint reason. Worker được đề xuất case mới nhưng không tự đổi expected behavior/acceptance ngoài authority. Trong Phase 1, `required` được parse structurally nhưng gate là `unavailable` cho tới Phase 3 baseline/case resolver; kernel không giả case IDs là valid. Chi tiết execution scopes và change control thuộc [`03-story-qa.md`](03-story-qa.md).

## Ready gate

Implementation Ticket chỉ `ready` khi CLI xác nhận deterministic conditions và shaping/reviewer receipt xác nhận semantic conditions theo policy:

- Objective, current/target behavior và acceptance không mâu thuẫn.
- Code anchors đủ để orient hoặc Ticket rõ ràng là discovery work.
- Required changes, invariants và scope boundary đủ cụ thể.
- Không còn critical ambiguity ở trạng thái `blocking` hoặc không được disposition.
- Mọi planning-critical branch đã `resolved`, `rejected`, `delegated` hoặc `deferred` hợp lệ; `delegated` không vượt implementation freedom và `deferred` không làm current contract sai.
- Nếu policy yêu cầu persisted shaping map: destination/exit condition có approval phù hợp, map revision khớp, decision frontier không còn item blocking Ticket, và `not_yet_specified` đã được review là bounded/non-blocking cho execution contract hiện tại.
- Hard blockers đều terminal/satisfied.
- Required Decisions/Story approach tồn tại, có approval/authority phù hợp và revision khớp.
- Implementation mode, plan policy, verification profile và handoff contract rõ.
- QA impact không còn `unknown`; `none` và `covered_by_story_close` có rationale, owner khi cần và approval grant tương ứng.
- `required` chỉ pass khi installed QA baseline/case resolver xác nhận affected/new references; trước Phase 3 family này là `unavailable`, không bypass.
- Documentation impact là `required`, `none` có rationale hoặc `deferred` hợp policy; không còn `unknown`.
- Applicable durable docs, owner và required Decisions resolve được khi risk/surface yêu cầu.
- Mọi content/document reference tồn tại và node/edge graph valid.

Kernel có thể validate schema, links, required fields, contract revisions, hashes, receipt integrity/bindings và policy grants; nó không tự phán đoán một conversation đã đạt shared understanding. Semantic shaping result do skill/reviewer tạo, có actor, contract/source/content bindings, branch summary và policy-required approval để ready gate kiểm tra.

Readiness là derived, versioned report với gate-family findings và narrow fingerprint trên relevant Ticket/blocker/shaping/Decision/docs/QA/policy/content projections; không dùng global graph fingerprint làm sole currentness key. `status=ready` nhưng input fingerprint đổi là `ready_state_stale`, bị loại khỏi execution frontier và không tự rewrite canonical status trong read path.

`ready` không có nghĩa Agent phải làm đúng từng dòng plan cũ hoặc đã được dispatch. Nó nghĩa Agent có một current executable contract, không phải phát minh lại mục tiêu, và biết lựa chọn nào được tự quyết hay lúc nào phải xin Decision. `dispatch_authorized` vẫn false cho tới khi Phase 2 xác nhận lease/capability/source workspace; work `qa.impact=required` còn chờ Phase 3 resolver.

## Progressive materialization

| Mức | Dùng khi | Artifact bắt buộc |
|---|---|---|
| `R0` | Việc nhỏ, risk thấp, direction rõ | Node + concise implementation brief + acceptance + verify; short ambiguity self-check, không mặc định hỏi human hoặc tạo map |
| `R1` | Thay đổi thông thường | `ticket.md`, validation receipt; focused shaping/branch list khi contract còn implicit |
| `R2` | Cross-module, multi-session hoặc ambiguity cao | Thêm `plan.md`/`approach.md`; persisted destination + decision frontier/fog khi cần, decision-tree grilling và independent review |
| `R3` | Architecture, migration, security, destructive change | Full shaping map, Decision/design, deep grilling, rollout/rollback, QA sâu và explicit authority |

Materialization có thể nâng khi Agent phát hiện risk hoặc critical ambiguity. Downgrade required artifacts hoặc bỏ qua policy-required shaping/review phải có authority và audit. Không tạo full brainstorm artifact chỉ để thỏa ceremony; disposition có thể được ghi trực tiếp vào canonical owner phù hợp.

## CLI là query surface bắt buộc

Agent và Orchestrator không tự `find`, `grep` hoặc parse raw graph files để dựng work state. Raw files phục vụ Git/human inspection; kernel/CLI sở hữu graph semantics.

```text
pulse work list --json
pulse work show TK-031 --json
pulse work packet TK-031 --json
pulse work ready --json
pulse work children ST-014 --json

pulse graph neighborhood TK-031 --depth 2 --json
pulse graph affected-by TK-029 --json
pulse graph validate --json
pulse graph export --json
```

`pulse work packet` trả một bounded execution packet:

- Ticket node và revision.
- Nội dung Ticket/plan cần thiết.
- Parent Story/Epic summaries.
- Applicable Decisions và shared approach.
- Approved destination/shaping revision, relevant branch dispositions và bounded remaining uncertainty; không inline toàn bộ shaping map nếu summary/reference đủ.
- Forward/reverse relations và blocker states.
- Writable scope, implementation mode, verification profile, QA impact, affected Story cases và checkpoint/Story-close gates.
- Expected evidence và handoff.
- Required/optional/write-candidate durable docs với content hashes và exclusion reasons.
- Ranked section refs/snippets theo retrieval budget; không inline toàn bộ applicable documents.
- Required/recommended/suggested learning refs, applicability explanation, required ratchet checks và knowledge prompt budget theo [`12-knowledge-compounding.md`](12-knowledge-compounding.md).

Machine output luôn có `schema_version`, stable field names và non-zero exit code khi graph invalid. Prompt builder dùng packet này; Agent không phải tìm Ticket bằng filesystem search.

## Full graph projection và cache

`pulse graph export --json` materialize một object gồm sorted `nodes`, `edges`, inverse indexes và derived readiness. Đây là read model, không writable truth.

`.pulse/cache/workgraph.snapshot.json` có thể tăng tốc query nhưng phải:

- Gitignored.
- Key theo graph fingerprint.
- Atomic replace.
- Bỏ qua/rebuild khi stale hoặc corrupt.
- Không cần thiết cho correctness.

Core v1 không dùng SQLite, kể cả làm canonical index. Chỉ cân nhắc một cache engine khác sau khi benchmark chứng minh file scan/index in-memory không đủ; cache vẫn phải disposable.

## Mutation protocol

- Mọi mutation đi qua CLI/library API, không hand-edit trong Agent workflow bình thường.
- Node update dùng expected `revision`; successful write tăng revision.
- Edge add/remove dùng deterministic ID và expected revision khi update.
- Validate schema, referential integrity và cycle rules trước atomic rename.
- Ghi một immutable semantic event sau mutation thành công.
- Worker trên isolated worktree không tự sửa canonical graph; nó gửi status/handoff để Orchestrator hoặc kernel-owned control workspace apply.
- Human hand-edit vẫn recover được qua `pulse graph validate`, nhưng không phải normal path.

## Lifecycle

```text
draft -> shaped -> ready -> active -> verifying -> done
                              |          |
                              v          v
                            blocked    rework

draft/shaped/ready/blocked -> cancelled
any non-terminal -> superseded
```

- `draft -> shaped`, `blocked -> shaped` và `shaped -> ready` chỉ mở qua typed shaping/readiness recomputation dưới repository lock; không có `--force`.
- Không mở direct `blocked -> ready`; resume phải đi qua `shaped` để structural executability được đánh giá lại.
- `ready` pass current executable-contract/readiness gates; stale projection không đủ để vào execution frontier.
- `active` phải có assignment lease hợp lệ và vẫn thuộc Phase 2.
- `verifying` khóa source snapshot.
- `done` do close gate tính, không do Worker tự khai báo. Ticket có QA posture `required` cần valid targeted checkpoint receipts; Story cần full applicable qualification receipts.
- `superseded` có `superseded_by` edge hoặc Decision giải thích.
- Epic/Story roll-up derive từ graph và QA, không lưu child counters chỉnh tay.

## Mutation ownership

- Human có thể tạo/sửa mọi work item và override có audit.
- Orchestration Agent có thể shape, link, prioritize, assign, requeue trong policy được cấp.
- Worker Agent chỉ gửi progress, blocker, proposed graph/registry mutation, documentation findings và handoff cho Ticket được lease.
- Reviewer/QA Agent ghi receipts, findings và learning candidates, trừ khi có Ticket riêng.
- Compound Agent/human synthesize candidates, validate applicability/provenance và đề xuất promotion; không tự override docs/Decision authority.
- Kernel validate schema, revision, transition, learning relations và gate; kernel không tự quyết semantic priority hoặc root cause.

## External tracker và Symphony boundary

GitHub Issues, Linear hoặc Jira là optional adapters. External sync phải chọn field ownership rõ; local và remote không cùng writable một field nếu thiếu conflict protocol.

Symphony-style orchestration nhận normalized issues từ tracker. Trong Pulse:

```text
Pulse JSON graph -> Pulse CLI query -> normalized issue/packet -> Orchestrator -> Agent
```

Vì vậy local work graph đóng vai trò tracker source mà Linear đóng trong Symphony, nhưng vẫn Git-native và offline. External tracker không tự động vượt qua local verification/QA gate.

## Close rules

Ticket chỉ `done` khi acceptance map tới valid evidence, developer verification và required review pass, QA posture được disposition, required targeted checkpoint receipts pass trên source snapshot hiện tại, documentation impact đã update/classify/defer hợp policy, không còn blocking finding và handoff ghi remaining risk trung thực. Ticket checkpoint chỉ chứng minh affected change scope; nó không thay full Story qualification.

Story chỉ `done` khi child outcome đủ, current `qa.md` coverage không còn gap bắt buộc và full applicable behavioral baseline pass trên integrated/frozen candidate snapshot theo [`03-story-qa.md`](03-story-qa.md). Epic đóng khi success signals được đánh giá, không chỉ vì mọi child Ticket mang nhãn `done`.
