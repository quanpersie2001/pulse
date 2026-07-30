# Knowledge Compounding Và Applicability-Aware Recall

[Trang vào](../PULSE_REBOOT.md) | [Bản đồ tài liệu](README.md) | [Work graph](02-work-graph.md) | [Runtime harness](04-runtime-harness.md) | [Verification ratchet](07-verification-ratchet.md) | [Documentation system](10-documentation-system.md) | [Documentation retrieval](11-documentation-retrieval.md)

**Đọc khi:** cần biết Pulse capture bài học sau execution/review/QA thế nào, learning record có schema gì, được promote về đâu, search/apply vào future work ra sao và tránh memory noise/staleness bằng cách nào.
**Sở hữu:** compounding lifecycle, learning taxonomy/schema/store, provenance, applicability, confidence, promotion, knowledge search/get/applicable, bounded context routing, feedback, contradiction, freshness và retirement.

## Khẳng định thiết kế

Pulse phải không chỉ hoàn thành work mà còn làm repository harness tốt lên sau mỗi success, failure, review, QA và recovery có giá trị.

> **Compounding biến observations có evidence thành reusable knowledge, route knowledge tới durable owner hoặc executable guardrail, rồi retrieve đúng learning cho future work bằng applicability-aware search.**

Chỉ lưu nhiều file learnings không tạo thành self-improving system. Compounding chỉ hoàn chỉnh khi có vòng kín:

```text
work / review / QA / failure / recovery
  -> capture candidates
  -> synthesize + classify
  -> validate provenance/applicability
  -> deduplicate/reconcile
  -> publish learning records
  -> promote to docs/Decision/skill/check/eval/policy
  -> retrieve for applicable future work
  -> observe usefulness and repeated failures
  -> reinforce/revise/supersede/retire
```

## Compounding không phải gì

Compounding không phải:

- transcript/session archive;
- summary chung chung “lần sau cẩn thận hơn”;
- một global Markdown file mọi Agent phải đọc;
- nguồn sự thật cạnh tranh với product/domain/architecture docs;
- quyền để Agent biến một observation đơn lẻ thành blocking policy;
- semantic search không có applicability/authority filtering;
- lý do để inject toàn bộ historical memory vào prompt;
- gate buộc mọi completed Ticket phải tạo learning dù không có gì reusable.

Một completed slice có thể hợp lệ với kết luận `no_reusable_learning`, miễn synthesis đã xem evidence theo policy và ghi disposition ngắn khi compound được yêu cầu.

## Bốn loại artifact không được nhập làm một

| Artifact | Trả lời câu hỏi | Canonical owner |
|---|---|---|
| Durable documentation | Repository hiện được hiểu/contract như thế nào? | `docs/`, `AGENTS.md`, `PULSE.md` |
| Decision | Lựa chọn khó đảo ngược nào đã được chấp nhận và vì sao? | Decision node + `works/DEC-*/` |
| Evidence/finding | Điều gì đã được quan sát trên source/environment nào? | `.pulse/evidence/` |
| Learning record | Future work nên nhận biết và hành động thế nào khi trigger tương tự xuất hiện? | `.pulse/knowledge/entries/` |

Learning record giữ provenance, reusable guidance, applicability và promotion history. Nếu learning phát hiện current product behavior, domain invariant, architecture boundary hoặc operator procedure, current truth phải được promote vào durable owner. Learning không được override docs/Decision bằng việc được rank cao hơn.

## Hai vòng compounding

### Knowledge loop

Knowledge loop tối ưu recall và reuse:

```text
candidate
  -> reviewed
  -> validated
  -> retrieved on applicable work
  -> helpful/noisy/contradicted feedback
  -> reinforced/revised/superseded/retired
```

### Enforcement loop

Enforcement loop giảm lỗi lặp lại:

```text
observation/failure
  -> failure pattern or correction
  -> ratchet with required checks
  -> skill/reviewer guidance
  -> deterministic script/check
  -> regression eval
  -> blocking hook/policy when signal is stable
```

Không phải learning nào cũng đi tới hook. Promotion strength phải tương ứng confidence, recurrence, cost và false-positive risk.

## Capture liên tục, compound có chủ đích

### Continuous capture

Trong execution lifecycle, các actor có thể tạo candidate:

- Worker: pattern, integration constraint, debugging technique, unexpected behavior.
- Reviewer: correction, missing invariant, design/review heuristic.
- QA: regression risk, environment constraint, behavioral failure signature.
- Doctor: context/tool/verification gap.
- Orchestration Agent: coordination/recovery/scheduling pattern.
- Human: intent/authority lesson hoặc approved generalization.

Candidate có thể được tạo ngay khi observation còn tươi, nhưng mặc định chưa được inject vào future Worker prompt.

### Post-cycle compound

Sau review/Story close hoặc một costly failure resolution, `pulse-compound`:

1. Gather work, Decisions, diffs, receipts, findings, QA, review và prior related learnings.
2. Extract distinct candidate insights.
3. Reject vague, duplicate hoặc non-actionable items.
4. Classify reusable kind và applicability.
5. Link provenance/evidence.
6. Reconcile contradiction với docs/Decision/prior learning.
7. Chọn promotion targets và required follow-up work.
8. Publish reviewed/validated records theo authority.
9. Update search index và reusable-work context.

Compounding là judgment capability; storage, validation, retrieval và mutation là deterministic kernel/CLI mechanisms.

## Learning taxonomy

`kind` tối thiểu:

| Kind | Dùng khi | Typical routing |
|---|---|---|
| `success_pattern` | Approach đã hiệu quả và reusable | docs, skill guidance, work context |
| `failure_pattern` | Failure signature/root cause có thể tái diễn | correction, ratchet, eval |
| `correction` | Tactical replacement cho wrong move cụ thể | implementer/reviewer context |
| `ratchet` | Non-regression must-check đã earned | verification/review, script/eval/hook |
| `decision_heuristic` | Cách nhận biết trade-off/direction tương tự | planner/shaper context; Decision khi hard rule |
| `debugging_technique` | Repro/isolation/diagnostic technique hiệu quả | debugger context, operations docs |
| `verification_technique` | Check/evidence strategy bắt được gap | verification profile, eval |
| `tooling_constraint` | Tool/runtime behavior ảnh hưởng execution | harness docs/skill/script |
| `environment_constraint` | Fixture/platform/service condition quan trọng | operations/QA environment |
| `integration_constraint` | Boundary/protocol/order/idempotency constraint | architecture/domain docs + tests |
| `performance_insight` | Load/latency/resource behavior reusable | performance docs/eval |
| `security_insight` | Security boundary/failure mode | security docs/policy/check |
| `process_insight` | Work shaping/review/coordination practice | planner/orchestrator skill |
| `context_routing_insight` | Agent thiếu/sai context và cách route đúng | docs registry, work packet, eval |

Taxonomy phải đủ nhỏ để ổn định. Nếu một candidate chỉ là event-specific implementation detail không reusable, classify `non_durable` thay vì tạo learning record.

## Canonical storage

Learning metadata dùng sharded JSON, tương tự nguyên tắc work graph nhưng là plane riêng:

```text
.pulse/
  knowledge/
    manifest.json
    schemas/
      learning.schema.json
      relation.schema.json
    entries/
      LRN-001.json
      LRN-002.json
    relations/
      derived-from--LRN-001--receipt-01J.json
      promoted-to--LRN-001--DOC-AUTH-DOMAIN.json
      superseded-by--LRN-001--LRN-009.json
  cache/
    knowledge-search/
      state.json
      records.jsonl
      lexical-index/

knowledge/
  learnings/
    LRN-001.md                 # optional human-facing detail
```

Rules:

- JSON entry là canonical machine semantics.
- Optional Markdown detail giữ narrative dài, examples và investigation context; node reference path qua `content_path`.
- Không bundle nhiều semantic learnings dưới một metadata record.
- Search cache là derived, gitignored và disposable.
- Learning ID ổn định qua rename/content rewrite.
- Relation ID deterministic từ `(type, from, to)`.
- Mutation qua CLI/API với expected revision, schema validation và immutable event.
- Evidence artifact không copy vào learning; learning reference receipts/content hashes.

Learning không phải work item. Nếu cần implementation/promotion/research, tạo Ticket/Decision và link từ learning.

## Learning record schema

Ví dụ minh họa:

```json
{
  "schema_version": 1,
  "id": "LRN-001",
  "revision": 3,
  "title": "Token rotation requires atomic state mutation",
  "status": "validated",
  "kind": "failure_pattern",
  "severity": "critical",
  "summary": "Concurrent refresh requests can issue immediately-invalid tokens when rotation uses a non-atomic check-then-act sequence.",
  "guidance": {
    "do": [
      "Use a transaction, row lock, or optimistic conflict detection.",
      "Add a parallel-request integration test."
    ],
    "avoid": [
      "Do not implement token rotation as separate read and write operations."
    ],
    "required_checks": [
      "Run at least 10 concurrent refresh attempts.",
      "Assert exactly one rotation succeeds."
    ]
  },
  "applicability": {
    "domains": ["authentication"],
    "surfaces": ["api"],
    "paths": ["src/auth/**"],
    "symbols": ["RefreshTokenHandler", "rotateRefreshToken"],
    "work_labels": ["auth", "session"],
    "technologies": ["postgresql"],
    "operations": ["token-rotation", "session-renewal"],
    "risks": ["concurrency", "idempotency"],
    "signals": [
      "check-then-act",
      "parallel refresh requests",
      "token already invalidated"
    ],
    "exclusions": ["stateless access-token verification"]
  },
  "provenance": {
    "source_work": ["ST-014", "TK-031"],
    "source_runs": ["run_01J..."],
    "source_receipts": ["receipt_01J..."],
    "source_findings": ["finding_01J..."],
    "source_commits": ["7d31c2a"]
  },
  "validation": {
    "confidence": "high",
    "validated_by": ["human:quannv", "eval:auth-concurrency"],
    "validated_at": "2026-07-18T10:00:00Z",
    "reproduction_count": 2,
    "contradiction_status": "none"
  },
  "routing": {
    "audiences": ["planner", "implementer", "validator", "reviewer"],
    "moments": ["shape", "plan", "execute", "verify"],
    "prompt_priority": "required_when_applicable",
    "max_summary_tokens": 90
  },
  "promotion": {
    "state": "promoted",
    "targets": [
      {"kind": "document", "id": "DOC-AUTH-DOMAIN", "content_hash": "sha256:..."},
      {"kind": "eval", "id": "EVAL-AUTH-CONCURRENCY"}
    ]
  },
  "freshness": {
    "review_after": "2027-01-18",
    "invalidated_by_paths": [
      "src/auth/refresh/**",
      "docs/domain/token-lifecycle.md"
    ]
  },
  "content_path": "knowledge/learnings/LRN-001.md",
  "created_at": "2026-07-18T09:30:00Z",
  "updated_at": "2026-07-18T10:00:00Z"
}
```

### Required semantic fields

Mỗi published learning cần:

- Stable ID, revision, title, status và kind.
- Concise summary.
- Actionable guidance: `do`, `avoid` hoặc `required_checks` phù hợp kind.
- Applicability có ít nhất một concrete trigger dimension.
- Provenance tới work/run/receipt/finding/Decision có thể inspect.
- Confidence/validation posture.
- Audience/moment routing.
- Promotion disposition.
- Freshness/review posture khi knowledge phụ thuộc technology/version/architecture.

Một entry chỉ có tags và prose `applicable_when` là chưa đủ cho deterministic routing. Human-readable trigger summary có thể tồn tại, nhưng machine applicability cần typed dimensions.

## Applicability model

Applicability trả lời:

> Learning này có liên quan vật chất tới current work/context không?

Typed dimensions gồm:

- `domains`: product/domain area.
- `surfaces`: web, API, CLI, data, mobile, library, infrastructure.
- `paths`: repository globs.
- `symbols`: identifiers/public interfaces.
- `work_kinds` và `work_labels`.
- `technologies`: framework, database, runtime, protocol.
- `operations`: migration, token rotation, cache invalidation, deployment, parsing...
- `risks`: concurrency, compatibility, destructive change, security...
- `signals`: error names, symptoms, observed patterns.
- `platforms/configurations/versions` khi cần.
- `exclusions`: explicit non-match boundaries.

Rules:

- Applicability phải concrete enough để false-positive thấp.
- Broad tag như `backend`, `testing`, `frontend` không đủ đứng một mình cho required injection.
- Exclusion thắng weak lexical similarity.
- Explicit work/Decision/doc relation mạnh hơn inferred metadata.
- Path/symbol đổi có thể tạo suspected-stale finding, không silent remove.
- Kernel match typed fields; Agent có thể đề xuất semantic applicability nhưng không tự nâng candidate thành required context.

## Lifecycle và confidence

### Status lifecycle

```text
candidate -> reviewed -> validated -> promoted
    |           |           |          |
    v           v           v          v
non_durable   disputed   superseded   retired
```

- `candidate`: extracted nhưng chưa qua quality/provenance review; không auto-inject.
- `reviewed`: actionable, scoped, non-duplicate và provenance đủ; có thể suggested recall.
- `validated`: corroborated bằng independent review, reproduction, eval hoặc authority; eligible cho automatic applicable context.
- `promoted`: durable target/guardrail đã cập nhật và có proof.
- `non_durable`: event-local detail, giữ event/disposition nhưng không publish searchable learning mặc định.
- `disputed`: contradiction chưa resolve; search có thể trả có label, không auto-inject.
- `superseded`: learning mới thay thế; không route mặc định.
- `retired`: không còn applicable/current; giữ history.

### Confidence

Confidence tách khỏi lifecycle:

- `low`: một observation, causal link chưa chắc.
- `medium`: review/reproduction hỗ trợ.
- `high`: multiple evidence, eval hoặc authoritative confirmation.
- `enforced`: deterministic check/policy hiện bảo vệ rule.

`promoted` không tự động nghĩa universal. Một promoted learning vẫn có applicability hẹp.

## Provenance và evidence quality

Learning không được trở thành folklore. Provenance nên link tới:

- source work IDs/revisions;
- source run/source snapshot;
- verification/QA/review receipts;
- finding IDs;
- Decision/docs revisions;
- relevant commands/files/artifacts;
- prior learning bị supersede hoặc corroborate.

Quality policy:

- Critical ratchet cần evidence/reproduction hoặc explicit human authority.
- `success_pattern` cần nêu trade-off và boundary, không chỉ “approach này chạy được”.
- Root cause chưa chắc phải ghi là hypothesis và confidence thấp/medium.
- Secret/sensitive data phải redact trước khi index.
- Learning summary không được copy raw prompt/transcript.

## Deduplication và relations

Relations tối thiểu:

- `derived_from`: learning -> work/run/receipt/finding.
- `corroborates`: learning -> learning.
- `contradicts`: learning -> learning/doc/Decision.
- `superseded_by`: old learning -> replacement.
- `promoted_to`: learning -> document/Decision/skill/script/check/hook/policy/eval.
- `implemented_by`: learning -> Ticket.
- `applied_to`: learning -> future work/run.
- `caused_by`: correction/ratchet -> failure pattern khi evidence đủ.

Compound synthesis phải search prior learnings trước khi create:

- Same insight + same applicability: update/corroborate existing entry.
- Same guidance nhưng different applicability: có thể tách records.
- Contradictory guidance: mark disputed/reconcile; không chọn theo recency tự động.
- New learning narrows/replaces old: supersede với migration note.

Không merge records chỉ vì title/tags giống; semantic scope và evidence phải được review.

## Promotion và routing

Learning phải có đúng một current disposition, nhưng có thể nhiều promotion targets.

### Promotion targets

| Learned knowledge | Primary durable/executable target |
|---|---|
| Current public/user behavior | `docs/product/` + behavioral QA cases |
| Architecture boundary/invariant | `docs/architecture/` hoặc Decision |
| Domain rule/state transition | `docs/domain/` |
| Setup/deploy/recovery technique | `docs/operations/` |
| Repository navigation/context gap | `AGENTS.md`, docs registry/index |
| Authority/risk/verification rule | `PULSE.md` hoặc policy config |
| Hard-to-reverse rationale | Decision |
| Reusable agent judgment process | Skill guidance |
| Stable mechanical invariant | Script/check/hook |
| Historical failure prevention | Eval/replay fixture |
| Future implementation work | Ticket trong same work graph |

### Promotion ladder

```text
observation
  -> candidate learning
  -> reviewed learning
  -> validated pattern/correction
  -> docs/Decision/skill guidance
  -> deterministic check + regression eval
  -> blocking guardrail/policy
```

Promotion không nhất thiết tuyến tính; một entry có thể cùng lúc promote tới docs, eval và correction. Stronger enforcement cần:

- repeatability/cost evidence;
- acceptable false-positive rate;
- clear applicability trigger;
- owner/authority;
- proof trên original case + regression eval.

### Dispositions

Mỗi candidate phải thành:

- `published`: create/update learning record.
- `promoted_directly`: durable owner được cập nhật, learning record optional nếu provenance/reuse không cần.
- `non_durable`: local detail với rationale.
- `duplicate`: link canonical learning.
- `disputed`: contradiction work/Decision cần resolve.
- `deferred`: linked work, owner và revisit trigger.

## Knowledge retrieval architecture

Knowledge dùng cùng class search infrastructure với docs nhưng là typed corpus riêng.

```text
canonical learning JSON/content
  -> normalized knowledge records
  -> applicability metadata index
  -> BM25+ lexical index
  -> typed filter + rank + explain
  -> bounded result summaries
  -> explicit get for full learning/provenance
```

Không merge docs và learnings vào một untyped result list. Docs giữ current truth/authority; learning giữ reusable historical guidance. Aggregator có thể combine chúng trong work packet nhưng phải preserve type, authority và reason.

### Search cache

```text
.pulse/cache/knowledge-search/
  state.json
  records.jsonl
  lexical-index/
```

Cache phải:

- Gitignored/disposable.
- Fingerprint theo knowledge schema/config/entry hashes.
- Atomic replace.
- Incremental rebuild được.
- Exclude candidate/superseded/retired/disputed theo default policy.
- Rebuild deterministic về eligible set và stable tie-break semantics.

Core v1 có thể reuse Tantivy abstraction của docs retrieval, nhưng field weights/filter schema khác.

## CLI contract

### Capture và lifecycle

```text
pulse compound <work-id> [--include-children]
pulse compound --run <run-id>
pulse compound --since <commit>
pulse compound status <work-id> [--json]
pulse compound review --candidates [--json]

pulse knowledge create
pulse knowledge capture --from-work <id>
pulse knowledge capture --from-run <run-id>
pulse knowledge list [--status <status>] [--kind <kind>] [--json]
pulse knowledge show <learning-id> [--json]
pulse knowledge edit <learning-id> --expected-revision <n>
pulse knowledge review <learning-id>
pulse knowledge validate <learning-id> --evidence <receipt-or-eval-id>
pulse knowledge promote <learning-id> --target <typed-id>
pulse knowledge supersede <learning-id> --by <learning-id>
pulse knowledge retire <learning-id> --reason <text>
pulse knowledge index|status
```

`pulse compound`/`pulse-compound` thực hiện semantic synthesis. `pulse knowledge ...` là deterministic query/mutation surface.

### Search và get

```text
pulse knowledge search "refresh token race"
pulse knowledge search "rollback migration" --kind ratchet
pulse knowledge search "SIGINT partial file" --surface cli
pulse knowledge search "detached DOM" --signal "detached DOM"
pulse knowledge search "auth concurrency" --domain authentication --limit 8 --json
pulse knowledge get LRN-001
pulse knowledge get LRN-001 --summary --json
```

Search mặc định:

- chỉ `reviewed`, `validated`, `promoted` theo policy;
- exclude `candidate`, `superseded`, `retired`, `disputed`;
- trả summary/metadata/reason, không full content;
- full learning/provenance cần explicit `get`.

### Applicable recall

```text
pulse knowledge applicable --work TK-031 --json
pulse knowledge applicable --work ST-014 --audience planner --moment plan --json
pulse knowledge applicable --paths src/auth/** --risk concurrency --json
```

Output phân tầng:

```json
{
  "schema_version": 1,
  "work_id": "TK-031",
  "knowledge_fingerprint": "sha256:...",
  "required": [],
  "recommended": [],
  "suggested": [],
  "excluded": []
}
```

Mỗi hit explain:

```json
{
  "learning_id": "LRN-001",
  "status": "validated",
  "kind": "ratchet",
  "summary": "Token rotation requires an atomic state transition.",
  "why_applicable": [
    {"dimension": "domain", "value": "authentication"},
    {"dimension": "path", "value": "src/auth/**"},
    {"dimension": "operation", "value": "token-rotation"},
    {"dimension": "risk", "value": "concurrency"}
  ],
  "required_action": "Use an atomic mutation and run the parallel refresh eval.",
  "required_checks": ["Exactly one of 10 concurrent rotations succeeds."],
  "promotion_targets": ["DOC-AUTH-DOMAIN", "EVAL-AUTH-CONCURRENCY"],
  "score": 0.91
}
```

`excluded` có thể chứa top lexical candidates bị loại vì exclusion, stale status, wrong version/platform hoặc audience mismatch để explain retrieval behavior mà không inject chúng.

## Retrieval algorithm

### 1. Build query context

Với `--work`, kernel lấy bounded typed context từ work packet:

- Work kind/labels/risk/materialization.
- Parent Story/Epic domain/outcome.
- Code anchors và allowed/changed paths.
- Relevant symbols/interfaces.
- Surface, technology, operation/config nếu declared/derived.
- Verification/QA profile.
- Applicable docs/Decisions.
- Error/symptom signals trong debug context.

Kernel không cần LLM để match explicit metadata. Agent adapter có thể đề xuất additional semantic query terms có provenance, nhưng không được thay typed filters.

### 2. Eligibility filtering

```text
status allowed
AND audience/moment allowed
AND freshness policy allowed
AND no explicit exclusion match
AND required version/platform/config constraints satisfied
AND no unresolved contradiction with authoritative current context
```

### 3. Applicability scoring

Signal priority khái niệm:

```text
explicit relation/reference
  > operation + risk match
  > path/symbol match
  > domain/surface/technology match
  > signal/error match
  > lexical body similarity
```

Confidence, promotion/enforcement và successful reuse có thể boost. Staleness, contradiction, broad applicability và context cost giảm rank.

### 4. Lexical ranking

BM25+ fields đề xuất:

- `title`: rất cao.
- `summary`: cao.
- `operations`, `signals`, `symbols`, `aliases`: cao.
- `guidance.do`, `required_checks`: medium-high.
- `domains`, `surfaces`, `technologies`, `risks`: medium.
- Optional narrative body: thấp hơn.

Không cộng raw scores từ incompatible systems thành public semantics. Result phải explain typed matches và lexical contribution ở mức đủ debug.

### 5. Bucket mapping

- `required`: applicable enforced ratchet/policy hoặc explicit Ticket/Decision reference.
- `recommended`: validated/promoted high-confidence match.
- `suggested`: reviewed or weaker semantic match, không auto-impose.
- `excluded`: matched lexically nhưng bị deterministic rule loại.

Agent không tự nâng suggested thành required trong canonical packet; có thể request retrieval/escalation hoặc đề xuất work update.

## Bounded context routing

Không inject raw memory corpus hoặc compile knowledge vào runner bootstrap prompt.
Work packet và `pulse knowledge applicable` dùng progressive disclosure:

```text
required knowledge refs/summaries
  -> recommended refs/summaries trong context budget
  -> explicit `knowledge get` khi Agent cần detail
  -> linked evidence/docs/eval chỉ khi cần inspect
```

### Role-specific routing

| Audience/moment | Default knowledge |
|---|---|
| Shape/plan | decision heuristics, success/failure patterns, foundation/context lessons |
| Execute | required corrections/ratchets, narrow implementation patterns |
| Debug | symptom-matched failures, repro/isolation techniques, environment constraints |
| Verify/QA | required checks, past false-negative patterns, regression evals |
| Review | known architecture/security/compatibility failure patterns |
| Orchestrate | coordination/recovery/scheduling learnings |

### Context entry contract

```json
{
  "id": "LRN-001",
  "kind": "ratchet",
  "title": "Token rotation requires atomic mutation",
  "why_applicable": "Ticket changes token rotation under concurrent API requests.",
  "action": "Use an atomic state transition and run the parallel refresh eval.",
  "required_checks": [
    "Exactly one of 10 concurrent rotations succeeds."
  ],
  "authority": "validated",
  "detail_ref": "knowledge:LRN-001"
}
```

Budget/policy:

- Required entries luôn nằm trước recommended.
- Deduplicate guidance đã có nguyên văn trong applicable authoritative docs; giữ link/provenance thay vì copy.
- Limit entry count và summary tokens theo audience/risk/context budget.
- Nếu required entries vượt budget, packet fail/advisory theo policy thay vì truncate silent.
- Full evidence không inline mặc định.
- Candidate/disputed/stale entries không auto-inject.

## Work packet integration

`pulse work packet` bổ sung:

- `knowledge_fingerprint`.
- Required/recommended/suggested learning refs.
- Applicability explanation.
- Required ratchet checks.
- Promoted docs/Decision/eval targets.
- Exclusion/contradiction advisories khi relevant.
- Context token budget và omitted count.

Planning/shaping packet có thể rộng hơn execution packet. Worker không tự scan
`.pulse/knowledge/entries`; bootstrap yêu cầu nó load lease-bound WorkPacket rồi
gọi `pulse knowledge applicable --work <id> --audience <role> --moment <moment>`
hoặc `knowledge get` theo refs. Knowledge content không được copy vào runner
prompt.

Nếu a required learning references stale/missing promotion target hoặc conflicts với accepted Decision/current docs, readiness/packet generation tạo actionable finding thay vì chọn một bên.

## Authority và contradiction

Authority order khi semantic content mâu thuẫn:

1. Explicit human authority/accepted Decision và approved product/policy contract.
2. Current authoritative durable docs cho affected scope.
3. Enforced validated learning/ratchet, nếu không contradict higher authority.
4. Validated/reviewed learning.
5. Candidate observation.

Đây không phải heuristic “docs luôn thắng” trong mọi tình huống; source code có thể đang defect và docs có thể stale. Nhưng learning không được silently override intended contract. Contradiction phải tạo typed finding và route tới owner/Decision/docs review.

Contradiction states:

- `none`
- `suspected`
- `confirmed`
- `resolved`

`confirmed` unresolved làm entry `disputed` hoặc exclude khỏi automatic routing.

## Freshness và invalidation

Learning có thể stale khi:

- Applicable paths/symbols bị xóa/đổi lớn.
- Technology/framework/database version vượt declared range.
- Architecture/Decision/product contract thay đổi.
- Promoted docs/check/eval target stale hoặc missing.
- Reproduction/eval không còn chạy.
- Repeated usage cho thấy false applicability/noise.

Freshness mechanisms:

- `review_after`.
- Version/platform/config ranges.
- `invalidated_by_paths` hoặc linked document/Decision hashes.
- Doctor findings cho dangling symbols/paths/targets.
- Usage feedback.
- Explicit `pulse knowledge status`.

Stale suspicion không tự xóa history. Policy quyết định exclude, advisory hoặc require review theo severity.

## Usage feedback và reinforcement

Self-improvement cần đo reuse, không chỉ số entries.

Run/handoff có thể ghi:

```json
{
  "knowledge_usage": [
    {
      "learning_id": "LRN-001",
      "injected": true,
      "opened": true,
      "applied": true,
      "outcome": "helpful",
      "evidence": ["receipt_01K..."]
    }
  ]
}
```

Allowed outcome:

- `helpful`
- `not_needed`
- `not_applicable`
- `misleading`
- `contradicted`
- `unknown`

Feedback không được dựa duy nhất vào self-report. Có thể derive/support từ:

- Agent có `get` learning không.
- Handoff/plan có reference không.
- Required check có chạy không.
- Reviewer xác nhận relevance/noise.
- Same failure có tái diễn không.
- Eval/guardrail có bắt được regression không.

Reinforcement policy:

- Successful reuse trên distinct work có thể tăng confidence/reuse count.
- Repeated `not_applicable`/`misleading` trigger applicability review.
- Same failure tái diễn dù required learning đã inject tạo `guardrail_gap`, `retrieval_gap` hoặc `model_execution_error` tùy evidence.
- Retrieval miss trên known expected learning trở thành eval fixture.

Không để popularity tự biến learning thành authority.

## Compound output contract

Một compound run phải output machine-readable summary:

```json
{
  "schema_version": 1,
  "compound_run_id": "cmp_01J...",
  "source_work": ["ST-014"],
  "source_snapshot": "7d31c2a",
  "candidate_count": 6,
  "published": ["LRN-001", "LRN-002"],
  "updated": ["LRN-009"],
  "duplicates": [
    {"candidate": "tmp-4", "canonical": "LRN-004"}
  ],
  "non_durable": [
    {"candidate": "tmp-5", "reason": "One-off implementation detail"}
  ],
  "disputed": [],
  "promotion_actions": [
    {"learning_id": "LRN-001", "target": "DOC-AUTH-DOMAIN"},
    {"learning_id": "LRN-001", "target": "EVAL-AUTH-CONCURRENCY"}
  ],
  "follow_up_work": ["TK-H-012"],
  "knowledge_fingerprint": "sha256:..."
}
```

Compound phải cho biết:

- Sources/evidence đã đọc.
- Candidates và dispositions.
- Existing learnings đã search/reconcile.
- Published/updated/superseded IDs.
- Applicability/confidence/routing.
- Promotion targets và follow-up work.
- Contradictions/remaining uncertainty.
- Không có reusable learning nếu thật sự không có.

## Close/gate posture

Compound mặc định là post-cycle capability, không phải merge gate cho mọi Ticket. Tuy nhiên close gate đã yêu cầu disposition durable documentation candidates theo [`10-documentation-system.md`](10-documentation-system.md).

Policy có thể require compound trước close/retrospective khi:

- R3/high-cost incident.
- Security or destructive failure.
- Escaped regression.
- Same failure class lặp lại.
- Significant harness/context/tool gap.
- Human explicitly requests reusable learning capture.

Nếu required compound bị defer, cần linked work, owner và trigger. Compound không reopen completed product work trừ khi synthesis phát hiện blocking contradiction/defect cần work item mới.

## Doctor và health checks

`pulse doctor` knowledge dimensions:

- Schema/relations valid.
- Candidate backlog quá lâu.
- Published entry thiếu provenance/applicability/actionable guidance.
- Duplicate/contradictory learnings.
- Dangling path/symbol/doc/Decision/eval targets.
- Stale review/version/hash.
- Required ratchet không có executable check/eval khi policy đòi.
- Retrieval index corrupt/stale.
- Over-broad entries có high false-applicability feedback.
- Critical global entries vượt prompt budget.
- Durable knowledge mắc trong work artifacts nhưng chưa promote/disposition.
- Repeated failure có learning nhưng retrieval/apply không xảy ra.

Finding format:

```text
finding -> evidence -> affected learning/scope -> impact
        -> suggested work/promotion -> suggested verification/eval
```

## Retrieval evals

Eval suite phải cover:

1. Exact operation/symbol match.
2. Natural-language paraphrase.
3. Path/domain/surface/risk typed match.
4. Error/symptom signal match.
5. Explicit exclusion thắng lexical similarity.
6. Candidate/superseded/retired/disputed exclusion.
7. Version/platform/config incompatibility.
8. Required ratchet bucket.
9. Role/moment routing khác nhau.
10. Docs/Decision contradiction không auto-inject learning.
11. Context budget và dedup với durable docs.
12. Known historical failure retrieves expected learning top-K.
13. No-result/no-applicable behavior.
14. Cache delete/rebuild deterministic eligible set.
15. Feedback-driven stale/applicability review.

Metrics:

- Applicable learning Precision@K/Recall@K/MRR.
- Known-failure recall.
- Required ratchet miss rate.
- False applicability/noise rate.
- Context bytes trước first useful learning.
- Injected-but-unused rate.
- Opened/applied/helpful rate.
- Repeated failure after applicable validated learning.
- Candidate-to-validated lead time.
- Learning-to-guardrail/eval lead time.
- Promotion coverage và stale/disputed rate.
- Duplicate learning rate.

Không tối ưu số learning records hoặc prompt entries.

## Security và prompt-safety

Knowledge corpus là prompt input nên phải chống:

- Raw secret/credential/customer data.
- Untrusted external text được promotion không review.
- Prompt injection trong narrative/evidence.
- Malicious aliases/tags để boost ranking.
- Broad `required` routing không authority.

Rules:

- Redact trước storage/index.
- Candidate từ untrusted source không auto-publish.
- Search index field limits và schema validation.
- `required_when_applicable` chỉ cho validated/enforced entry với authority.
- Prompt builder render typed fields, không blindly concatenate raw Markdown.
- Full narrative/evidence chỉ explicit get và vẫn tagged provenance/trust.

## Acceptance scenarios

1. Completed work có reusable failure pattern tạo một learning record với stable ID, actionable guidance, typed applicability và source receipts.
2. Compound tìm thấy canonical prior learning cùng semantic scope thì corroborate/update thay vì tạo duplicate.
3. Một one-off implementation detail được classify `non_durable` và không xuất hiện trong default search.
4. Candidate learning chưa reviewed không auto-inject vào Worker packet.
5. `pulse knowledge search` trả bounded summary/metadata; full content/provenance chỉ qua explicit `get`.
6. `pulse knowledge applicable --work TK-031` match domain/path/operation/risk và explain từng dimension.
7. Strong lexical hit bị explicit exclusion hoặc version mismatch loại và xuất hiện trong `excluded` explanation.
8. Validated ratchet applicable được route `required` cho validator/reviewer; success pattern hẹp route `recommended` cho implementer.
9. Planner và Worker nhận different bounded knowledge sets từ cùng work theo audience/moment policy.
10. Accepted Decision/current product contract mâu thuẫn learning làm entry excluded/disputed và tạo finding; kernel không chọn learning để override.
11. Learning promote tới docs + eval giữ links/content hashes; target stale/missing làm health finding.
12. Failed attempt tái diễn dù learning required đã inject được classify retrieval/apply/guardrail gap dựa trên evidence.
13. Same failure không tái diễn trên distinct applicable work và required checks pass tạo reinforcement evidence nhưng không tự đổi authority.
14. Superseded/retired learning không xuất hiện default search/applicable packet; history vẫn query explicit.
15. Xóa knowledge search cache rồi rebuild giữ eligible set, fingerprint semantics và stable tie-break behavior.
16. Critical entries vượt context budget làm explicit packet finding/failure theo policy, không truncate silent.
17. Raw secret hoặc untrusted prompt text trong candidate bị redaction/review gate chặn trước publish/index.
18. R3 incident required compound nhưng bị bỏ qua làm gate/defer finding theo policy; normal low-risk work có thể hợp lệ với `no_reusable_learning`.
19. Work artifact chứa durable invariant nhưng compound chỉ lưu learning, chưa promote docs/Decision, vẫn bị documentation gate chặn.
20. Retrieval eval trên historical fixture tìm đúng expected learning top-K và đo false-applicability/context budget.
