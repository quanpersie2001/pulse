# Documentation Knowledge System

[Trang vào](../PULSE_REBOOT.md) | [Bản đồ tài liệu](./README.md) | [Work graph](02-work-graph.md) | [Runtime harness](04-runtime-harness.md) | [Documentation retrieval](11-documentation-retrieval.md) | [Knowledge compounding](12-knowledge-compounding.md)

**Đọc khi:** cần biết durable repository knowledge được phân loại, sở hữu, định tuyến vào Agent, cập nhật, kiểm chứng, promote và retire như thế nào.
**Sở hữu:** documentation taxonomy, source hierarchy, folder boundaries, document registry, authority, lifecycle, Ticket documentation impact, context routing, validation receipts, drift detection và brownfield migration.

## Khẳng định thiết kế

Documentation trong Pulse là một **first-class repository capability**. Nó giúp human và Agent hiểu đúng product, architecture, domain, operations và policy trên một source snapshot cụ thể.

Documentation không phải phần trang trí sau implementation. Nếu một thay đổi làm đổi public behavior, invariant, architecture boundary, operator procedure hoặc human authority mà durable docs không được cập nhật hay defer hợp lệ, thay đổi đó chưa hoàn tất.

Nguyên tắc ngắn:

> **Durable knowledge có owner, applicability, authority và proof; work prose, runtime state và evidence không được giả làm current documentation truth.**

Pulse không biến mọi Markdown file thành graph node và không bắt Agent đọc toàn bộ `docs/`. Kernel định tuyến bounded context từ một registry nhỏ; `pulse docs search/get` cung cấp section-level progressive retrieval theo [`11-documentation-retrieval.md`](11-documentation-retrieval.md); semantic judgment về nội dung vẫn thuộc Agent/human.

## Documentation plane trong kiến trúc

Pulse tách năm loại dữ liệu:

| Plane | Trả lời câu hỏi | Writable truth |
|---|---|---|
| Durable documentation | Repository hiện được hiểu như thế nào? | `docs/`, `AGENTS.md`, `PULSE.md` |
| Work content | Thay đổi nào đang được đề xuất/thực hiện? | `works/<work-id>/` |
| Work graph | Work item, relation và lifecycle hiện ở trạng thái nào? | `.pulse/workgraph/nodes/` + `edges/` |
| Evidence | Điều gì đã được quan sát/chứng minh trên snapshot nào? | `.pulse/evidence/` receipts/artifacts |
| Runtime coordination | Ai đang làm gì, lease/workspace/cursor nào còn sống? | local `.pulse/runtime/` |
| Reusable learning | Future work nên biết/hành động gì khi trigger tương tự xuất hiện? | `.pulse/knowledge/entries/` |

Ranh giới bắt buộc:

- `docs/` mô tả current durable repository knowledge, không chứa active plan hoặc run log.
- `works/` chứa human-facing Epic/Story/Ticket/Decision prose, không phải machine status source.
- `.pulse/workgraph/` chứa canonical graph metadata, không chứa durable product/architecture docs.
- `.pulse/evidence/` chứa immutable proof, không phải explanatory documentation.
- `.pulse/runtime/` là coordination state có TTL/recovery, không phải memory dài hạn.
- `.pulse/knowledge/` giữ reusable guidance + provenance/applicability; nó không phải current product/domain/architecture truth.

## Canonical layout

```text
AGENTS.md                         # repository map, ngắn và route tới knowledge owners
PULSE.md                          # repository policy, authority và verification boundaries

docs/
  manifest.json                  # optional tracked registry projection; xem Document Registry
  product/                       # current product/public behavior contract
  architecture/                  # boundaries, components, dependencies và invariants
  domain/                        # vocabulary, rules, state machines và domain constraints
  operations/                    # setup, deploy, recovery, troubleshooting và runbooks
  reference/                     # authored reference material
  _index.md                      # generated navigation projection, không phải truth
  generated/                     # generated docs nếu repo chọn route này
  decisions/                     # optional human-readable Decision projections/ADRs

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
  DEC-006/
    decision.md

.pulse/
  workgraph/
    manifest.json
    schemas/
    nodes/
    edges/
  docs/
    registry.json                # canonical machine registry nếu repo bật managed docs
    schemas/
      document.schema.json
  events/
  evidence/
  runtime/                       # gitignored/shared-local according to repo policy
  migrations/
    docs-backups/
  cache/
    docs-search/                 # disposable section-level lexical index
```

Chi tiết `_index.md`, section records, BM25 search/get, cache và semantic adapter thuộc [`11-documentation-retrieval.md`](11-documentation-retrieval.md).

### Quyết định folder boundary

Human-facing work content nằm ở top-level `works/`, không nằm trong `.pulse/workgraph/works/`.

Lý do:

- Human tìm, review và apply CODEOWNERS dễ hơn.
- Machine graph metadata không trộn với prose.
- Worker có thể sửa source và work prose trong worktree, trong khi canonical graph mutation vẫn qua control workspace.
- Promotion từ active work sang durable docs có ranh giới rõ.
- Brownfield migration từ layout Pulse hiện tại đơn giản hơn.

Work graph node dùng path reference:

```json
{
  "id": "TK-031",
  "content_dir": "works/TK-031"
}
```

`content_dir` phải ở trong configured work-content root, không được path traversal hoặc trỏ vào migration backup.

## Documentation taxonomy

### Repository map

- **Path mặc định:** `AGENTS.md`.
- **Mục đích:** chỉ ra entrypoints, commands, docs map, code boundaries và nơi tìm policy.
- **Không dùng cho:** knowledge dump, full architecture hoặc copied domain rules.
- **Authority:** maintainer/human; Agent có thể đề xuất hoặc sửa map trong Ticket scope nếu route thay đổi.

### Pulse repository policy

- **Path mặc định:** `PULSE.md`.
- **Mục đích:** risk policy, protected areas, authority boundaries, verification profiles và human gates.
- **Không dùng cho:** product design hoặc command implementation details.
- **Authority:** human hoặc explicitly granted Orchestrator capability.

### Product contract

- **Path:** `docs/product/`.
- **Mục đích:** current user/system-visible behavior, compatibility promises, supported flows và product constraints.
- **Authority:** product owner/human policy; Worker không mặc nhiên được đổi approved outcome.
- **Freshness:** behavior-changing Story/Ticket phải đánh giá impact.

### Architecture documentation

- **Path:** `docs/architecture/`.
- **Mục đích:** system boundaries, dependency direction, cross-cutting invariants, data ownership và integration contracts.
- **Authority:** architecture owner hoặc policy-defined reviewer.
- **Freshness:** cross-module, migration, security và boundary changes thường bắt buộc review độc lập.

### Domain documentation

- **Path:** `docs/domain/`.
- **Mục đích:** glossary, state machines, business rules, error taxonomy và domain invariants.
- **Authority:** domain owner hoặc human-defined policy.
- **Freshness:** semantic behavior thay đổi phải cập nhật cùng source snapshot.

### Operations documentation

- **Path:** `docs/operations/`.
- **Mục đích:** setup, deployment, incident response, recovery, migration, rollback và troubleshooting.
- **Authority:** operator/platform owner.
- **Freshness:** command, environment, migration hoặc recovery behavior thay đổi phải có runnable proof khi policy yêu cầu.

### Reference documentation

- **Path:** `docs/reference/` hoặc repository-defined paths.
- **Mục đích:** stable authored API/config/protocol reference.
- **Authority:** owning code/domain team.
- **Freshness:** public API/config changes phải đánh giá impact.

### Generated documentation

- **Path:** repository-defined, thường `docs/generated/` hoặc package-local reference output.
- **Mục đích:** projection từ code/schema/source khác.
- **Authority:** generator source; output không hand-edit nếu manifest ghi `editable: false`.
- **Freshness:** deterministic generation check.

### Decisions

Decision node là durable reasoning identity trong work graph. Human-readable ADR/projection có thể nằm ở `works/DEC-006/decision.md` hoặc `docs/decisions/`, nhưng chỉ một path được khai báo writable source.

Decision trả lời “vì sao chọn phương án này”; architecture/product docs trả lời “repository hiện hoạt động theo contract nào”. Accept Decision thường kéo theo cập nhật durable docs, nhưng Decision không thay thế current-state documentation.

### Work artifacts

`brief.md`, `story.md`, `ticket.md`, `approach.md`, `plan.md`, `validation.md` và `qa.md` mô tả work. Chúng có thể chứa knowledge candidate nhưng không tự trở thành current repository truth.

### Evidence và runtime artifacts

Receipts, traces, screenshots, logs, handoffs, mailbox messages và runtime summaries không phải documentation truth. Chúng có thể được link từ docs hoặc dùng làm proof cho docs validation.

## Source hierarchy và contradiction semantics

Không có một thứ tự đơn giản kiểu “code luôn thắng”. Pulse phân biệt **intent**, **implementation**, **observation** và **explanation**:

1. Accepted Decision và approved product contract mô tả intended behavior/constraint đã khóa.
2. Code và tests trên source snapshot mô tả implemented behavior.
3. Valid receipts mô tả observed proof trên snapshot và environment cụ thể.
4. Architecture/domain/operations docs mô tả durable explanatory truth.
5. Work artifacts mô tả planned hoặc in-progress change.
6. Conversation, generated summaries và migration backups không phải current truth.

Nếu các lớp mâu thuẫn:

- Kernel không tự chọn tài liệu “đúng”.
- Mâu thuẫn ảnh hưởng acceptance, safety hoặc public behavior làm ready/close gate fail.
- Tạo `docs_conflict` finding và Decision hoặc repair Ticket.
- Owner/human xác định code là defect, docs là stale, hay contract cần thay đổi.
- Conflicting doc được cập nhật hoặc retire/supersede có audit.

## Document Registry

Pulse dùng một registry machine-readable nhỏ để route durable docs. Registry không cần liệt kê mọi Markdown file; chỉ đăng ký docs có vai trò contract, ownership, generated freshness hoặc automatic routing.

Canonical path mặc định:

```text
.pulse/docs/registry.json
```

Repository có thể materialize một human-facing projection ở `docs/manifest.json`, nhưng không được có hai writable registries. Nếu projection tồn tại, nó phải generated và freshness-checkable.

Ví dụ:

```json
{
  "schema_version": 1,
  "documents": [
    {
      "id": "DOC-AUTH-ARCH",
      "path": "docs/architecture/authentication.md",
      "kind": "architecture",
      "authority": "approved",
      "owner": "team:identity",
      "scope": {
        "paths": ["src/auth/**"],
        "domains": ["authentication"],
        "work_labels": ["auth"]
      },
      "review_policy": "independent",
      "verification_profile": "architecture-doc",
      "generated": false,
      "superseded_by": null
    }
  ]
}
```

### Document record tối thiểu

- `id`: stable ID, không dựa hoàn toàn vào path.
- `path`: repository-relative path, không traversal/symlink escape.
- `kind`: taxonomy enum hoặc repository extension.
- `authority`: `draft`, `approved`, `informational` hoặc `generated`.
- `owner`: human/team/role hoặc fallback escalation owner.
- `scope`: path/domain/work-label applicability.
- `review_policy`: none/light/standard/independent/human.
- `verification_profile`: optional docs verification profile.
- `generated`: boolean và generator contract nếu true.
- `superseded_by`: optional replacement document ID.

### Registry rules

- Duplicate document ID hoặc duplicate canonical path làm validate fail.
- Retired/superseded docs không được route như current context.
- Missing owner trên approved high-risk docs là finding.
- Registry mutation đi qua CLI/API khi managed mode bật.
- Content vẫn là normal Git file; registry không embed toàn bộ Markdown.
- Registry cache/projection disposable; canonical ownership phải rõ trong config.

## Applicability và bounded context routing

`pulse work packet` resolve docs bằng:

- Explicit Ticket/Story/Decision references.
- Code anchors và allowed changed paths.
- Parent Story/Epic domains/labels.
- Document registry path/domain scopes.
- Verification profile và risk.
- Repository policy.

Packet phân loại:

- `required`: Agent phải đọc trước mutation liên quan.
- `optional`: context có thể nạp khi implementation discovery cần.
- `write_candidates`: docs có khả năng cần cập nhật.
- `excluded`: stale, retired, migration backup hoặc không-authoritative material.

Ví dụ:

```json
{
  "documentation": {
    "required": [
      {
        "id": "DOC-AUTH-DOMAIN",
        "path": "docs/domain/token-lifecycle.md",
        "sections": ["Refresh token lifecycle", "Failure taxonomy"],
        "content_hash": "sha256:...",
        "reason": "domain and source-scope match"
      }
    ],
    "write_candidates": ["DOC-AUTH-DOMAIN"],
    "excluded": [
      {
        "path": ".pulse/migrations/docs-backups/mig-001/docs/auth.md",
        "reason": "migration backup is not current truth"
      }
    ]
  }
}
```

Agent không phải grep toàn bộ docs tree để tìm contract. Agent vẫn được search source repository cho implementation discovery; nếu registry thiếu applicable docs, đó là `docs_context_gap` hoặc `context_gap` để ratchet.

## Ticket documentation impact contract

Mọi implementation Ticket phải có một trong ba posture:

1. `required`: durable docs phải thay đổi trong Ticket này.
2. `none`: không ảnh hưởng docs, có rationale.
3. `deferred`: follow-up docs work được policy cho phép và đã tạo/link.

Ví dụ trong `ticket.md`:

```markdown
## Documentation impact

### Posture
required

### Applicable durable docs
- DOC-AUTH-ARCH — `docs/architecture/authentication.md`
- DOC-AUTH-DOMAIN — `docs/domain/token-lifecycle.md`

### Required updates
- Bổ sung phân biệt expired và revoked refresh token.
- Giữ nguyên public response compatibility contract.

### Unchanged docs
- `docs/product/login.md`: user-visible login flow không đổi.

### Promotion candidates
- Nếu implementation tạo invariant mới, cập nhật DOC-AUTH-DOMAIN trước close.
```

Với `none`:

```markdown
### Posture
none

### Rationale
Internal refactor; không đổi behavior, public interface, architecture boundary,
domain invariant, operator command hoặc repository navigation.
```

### Ready gate

Implementation Ticket không `ready` khi:

- Documentation impact là `unknown`.
- Public behavior/API/config thay đổi nhưng không có product/reference docs assessment.
- Architecture/security/migration/operator change không có applicable docs hoặc explicit rationale.
- Required document/owner/reference không tồn tại.
- `deferred` nhưng không có linked follow-up hoặc policy không cho defer.
- High-risk Ticket có contradiction chưa resolve giữa contract docs, Decisions và acceptance.

Discovery/Spike Ticket có thể `ready` với `documentation_impact: investigate`, nhưng expected output phải gồm docs findings.

### Close gate

Ticket close gate yêu cầu:

- Required docs đã update trên cùng source snapshot hoặc receipt policy-compatible snapshot.
- Docs validation/review receipts hợp lệ.
- `none` rationale vẫn đúng sau diff review.
- Promotion candidates đã apply, được classify non-durable, hoặc defer bằng linked work có authority.
- Không còn blocking `docs_conflict`, `docs_stale` hoặc `docs_work_leak` finding.

Policy không nên cho defer vô điều kiện đối với public API, security behavior, destructive migration, rollback/runbook hoặc new architecture constraint.

## Documentation lifecycle

```text
discover -> classify -> register -> current
               |          |
               v          v
             draft -> proposed -> approved
                                  |
                                  v
                           suspected_stale -> stale
                                  |              |
                                  +---- revise --+
                                                 |
                                                 v
                                              retired
```

Không phải mọi doc cần persisted status. Pulse có thể derive freshness từ receipts, source changes và registry metadata. Normative semantics:

- `draft`: chưa phải contract.
- `approved/current`: được route như authoritative context.
- `suspected_stale`: có drift evidence, cần review; policy quyết định có block hay không.
- `stale`: không được dùng như current truth cho affected scope.
- `retired`: giữ history nhưng không route; có replacement/reason khi phù hợp.

Path rename không đổi document identity. Registry update phải preserve ID và validate references.

## Documentation authority

| Action | Human | Orchestrator | Worker | Reviewer/QA | Kernel |
|---|---:|---:|---:|---:|---:|
| Sửa informational docs trong Ticket scope | Có | Theo policy | Có theo lease | Đề xuất | Validate scope/hash |
| Sửa approved product contract | Có | Chỉ khi explicit grant | Không mặc định | Review | Enforce gate |
| Sửa architecture/domain constraint | Có | Theo policy | Chỉ khi Ticket/Decision cho phép | Independent review | Validate references |
| Tạo draft Decision/docs | Có | Có | Có thể đề xuất | Có thể đề xuất | Validate schema |
| Accept Decision hoặc policy change | Có | Chỉ khi explicit grant | Không | Không | Record authority |
| Retire canonical doc | Có | Theo policy | Không | Đề xuất | Validate replacement |
| Regenerate generated docs | Có | Có | Có theo scope | Verify freshness | Check generator contract |
| Tạo docs receipt/finding | Có | Có thể dispatch | Self-check only | Có | Validate receipt |

Transport role không quyết định authority. Worker có writable source scope không đồng nghĩa được đổi approved product contract.

## Promotion từ work artifact sang durable knowledge

Work artifact có thể phát hiện:

- Product behavior mới.
- Architecture boundary/invariant mới.
- Domain term, rule hoặc state transition mới.
- Setup/recovery/operator procedure mới.
- Repository navigation gap.
- Policy/authority boundary mới.
- Repeated debugging/context knowledge.

Handoff ghi `documentation_findings`:

```json
{
  "documentation_findings": [
    {
      "kind": "promotion_candidate",
      "source": "works/TK-031/plan.md",
      "target": "DOC-AUTH-DOMAIN",
      "summary": "Expired and revoked refresh tokens now have distinct domain semantics",
      "required_for_close": true
    }
  ]
}
```

Routing mặc định:

| Knowledge | Đích |
|---|---|
| Current user-visible behavior | `docs/product/` |
| Component boundary/dependency/invariant | `docs/architecture/` |
| Domain terms/rules/state machines | `docs/domain/` |
| Setup/deploy/recovery/troubleshooting | `docs/operations/` |
| Repository navigation | `AGENTS.md` |
| Authority/risk/verification policy | `PULSE.md` |
| Durable fork và rationale | Decision node |
| One-off implementation detail | Giữ trong work artifact |
| Reproducible observation/proof | Evidence receipt |
| Repeated Agent friction | Learning record + Harness Ticket/skill/script/eval |

Mỗi documentation candidate phải được:

- `promoted`: cập nhật target docs.
- `non_durable`: ghi rationale vì sao chỉ là local detail.
- `deferred`: tạo linked Ticket với authority và deadline/revisit trigger.

Nếu insight reusable nhưng chưa phải current documentation truth, tạo/link learning record theo [`12-knowledge-compounding.md`](12-knowledge-compounding.md). Chỉ lưu learning không thỏa documentation close gate khi candidate thực chất là durable product/domain/architecture/operations knowledge.

## Documentation validation

### Mechanical checks

Repository harness có thể cung cấp:

- Broken internal/external links.
- Missing files, commands hoặc symbols được reference.
- Duplicate registry IDs/canonical paths.
- Invalid frontmatter/registry schema.
- Dangling Decision/work/document references.
- Retired docs vẫn được link như current.
- Code snippets hoặc commands không chạy khi profile yêu cầu.
- Generated docs stale hoặc bị hand-edit trái policy.
- `AGENTS.md` route tới path không tồn tại.
- Migration backup bị đăng ký/routed như current docs.

### Semantic review

Reviewer Agent/human đánh giá:

- Docs có mô tả behavior được receipts chứng minh không?
- Product contract và Story QA baseline có mâu thuẫn không?
- Architecture docs còn phù hợp với dependency/source change không?
- New invariant được đặt đúng owner document chưa?
- Có duplicate truth hay work artifact leakage không?
- `none` rationale có trung thực với diff không?

Kernel validate receipt structure, source/content hashes và required layers; kernel không tự kết luận prose đúng về semantic.

## Typed documentation receipt

```json
{
  "receipt_version": 1,
  "kind": "documentation_validation",
  "run_id": "run_01J...",
  "source": {
    "commit": "7d31c2a",
    "dirty_diff_hash": null
  },
  "documents": [
    {
      "id": "DOC-AUTH-DOMAIN",
      "path": "docs/domain/token-lifecycle.md",
      "content_hash": "sha256:...",
      "result": "passed"
    }
  ],
  "checks": [
    {"kind": "link_check", "result": "passed"},
    {
      "kind": "semantic_review",
      "result": "passed",
      "reviewer": "agent_reviewer_02"
    }
  ],
  "finished_at": "2026-07-18T03:00:00Z"
}
```

Receipt hợp lệ khi:

- Source snapshot khớp close target hoặc policy-compatible ancestor.
- Document content hash khớp file hiện tại.
- Required checks/reviewer authority đúng profile.
- Referenced artifacts tồn tại và hash đúng.
- Generated docs freshness check dùng declared generator.
- Receipt chưa quá TTL nếu docs phụ thuộc external environment.

Doc thay đổi sau review làm receipt cũ không còn hợp lệ cho content mới.

## Generated documentation contract

Generated docs khai báo:

```yaml
generated_docs:
  - id: DOC-API-REFERENCE
    sources:
      - src/public-api/**
    command: pnpm generate:api
    outputs:
      - docs/generated/api/**
    editable: false
    freshness_check: pnpm generate:api --check
```

Rules:

- Source và generator command là truth; output là projection.
- Hook/check có thể chặn stale output hoặc forbidden hand edit.
- Generator phải deterministic đủ cho repository policy.
- Generated docs lớn có thể không đăng ký từng file; registry record có thể đại diện output set.
- Không dùng generated docs làm nơi duy nhất giữ semantic rationale.

## `pulse docs` CLI

Core surface đề xuất:

```text
pulse docs list [--kind <kind>] [--json]
pulse docs show <doc-id> [--json]
pulse docs index|status
pulse docs search <query> [--json]
pulse docs get <document-or-section-ref> [--json]
pulse docs tree [path] [--json]
pulse docs applicable --work <id> [--json]
pulse docs validate [--changed] [--json]
pulse docs impact <ticket-id> [--json]
```

Các mutation/lifecycle command có thể thêm sau khi contract ổn định:

```text
pulse docs register|edit|retire
pulse docs promote <work-id> --target <doc-id>
pulse docs drift [--json]
```

Machine output có `schema_version`, stable fields và non-zero exit khi registry/validation invalid. CLI không tự rewrite semantic prose hoặc tự resolve contradiction. `search` trả ranked section metadata/snippets; `get` mới đọc bounded canonical content, tránh đưa full docs vào context mặc định.

## Doctor findings và failure taxonomy

`pulse doctor` và review có thể tạo:

- `docs_missing`: behavior/interface quan trọng không có durable owner doc.
- `docs_stale`: content có drift evidence.
- `docs_conflict`: hai authoritative sources mâu thuẫn.
- `docs_orphaned`: approved doc không owner hoặc không reachable từ map.
- `docs_duplicate_truth`: cùng contract được maintain ở nhiều writable places.
- `docs_generated_stale`: generated output khác source.
- `docs_unverified_example`: snippet/command cần proof nhưng chưa có.
- `docs_work_leak`: durable knowledge chỉ còn trong closed work artifact.
- `docs_policy_gap`: authority/freshness/defer rule không rõ.
- `docs_context_gap`: execution packet không route applicable doc.

Mỗi finding phải có:

```text
finding -> evidence -> affected scope -> impact -> owner/escalation
        -> suggested work item -> suggested verification -> severity
```

Repeated docs failures đi vào ratchet như harness work trong cùng graph. Ví dụ `docs_context_gap` lặp lại có thể dẫn tới registry scope fix, `AGENTS.md` map fix, skill guidance hoặc deterministic check.

## Story QA và documentation

Story behavioral baseline có thể link product/domain docs nhưng không copy toàn bộ contract. Rules:

- `works/<STORY-ID>/qa.md` giữ behavioral cases/coverage; durable product/domain docs giữ long-lived product truth. Hai loại artifact phải reference nhau nhưng không duplicate toàn bộ nội dung.
- Child Ticket chỉ reference affected Story case IDs và khai báo QA impact; không copy expected behavior sang Ticket baseline thứ hai.
- Product contract và baseline expectation mâu thuẫn làm planning/close gate fail.
- QA observation/checkpoint có thể tạo promotion candidate hoặc `docs_stale` finding.
- Story behavior hoặc expected observation thay đổi phải có product docs posture và owning authority rõ; Worker/QA Agent không tự sửa cả baseline lẫn docs để hợp thức hóa implementation.
- Story không đóng nếu required docs receipts thiếu/invalid hoặc known limitation chưa được record.
- QA receipt chứng minh behavior; docs receipt chứng minh documentation update/consistency. Hai receipt không thay nhau.

## Cross-agent coordination

- Assignment packet khai báo docs `read_scope` và `write_scope` riêng.
- Approved contract docs có thể cần exclusive edit advisory hoặc dedicated lease theo repo policy.
- Worker không được mở rộng docs scope hoặc đổi authority silently.
- Reviewer dùng frozen source snapshot và document content hashes.
- Contract docs revision thay đổi giữa run tạo typed `redirect`; Worker acknowledge revision mới hoặc handoff rồi dừng.
- Human takeover tạo event; Orchestrator không tiếp tục gửi conflicting docs edits.
- Canonical registry mutation chỉ qua control workspace/CLI; Worker gửi proposed registry changes trong handoff.

## Brownfield onboarding và migration

Brownfield flow:

```text
scan existing docs
  -> classify likely roles and duplicate truths
  -> propose source hierarchy/owners
  -> human approves semantic moves or merges
  -> snapshot affected docs
  -> register durable docs
  -> update AGENTS.md map
  -> validate links/references/generated outputs
```

Trước restructure hoặc rewrite:

```text
.pulse/migrations/docs-backups/<migration-id>/
  manifest.json
  ...original paths...
```

Backup manifest chứa source paths, hashes, timestamp và reason. Backup:

- Không phải current truth.
- Không được route vào normal execution packet.
- Chỉ dùng rollback, audit hoặc manual recovery.
- Không được tự xóa cho tới retention policy cho phép.

Pulse không tự move/merge docs có semantic ambiguity nếu chưa có human approval. Safe fixes chỉ gồm tạo missing registry scaffold, deterministic projection, link index hoặc non-destructive map entries.

## Documentation-specific capabilities

Repository harness có thể cung cấp các capability references, không nhất thiết đều là public skills:

- `pulse-docs-orient`: tìm canonical docs, owners và authority.
- `pulse-docs-impact`: đánh giá Ticket/Story documentation impact.
- `pulse-docs-update`: sửa đúng owner doc, tránh duplicate truth.
- `pulse-docs-review`: semantic consistency, discoverability và freshness.
- `pulse-docs-promote`: chuyển durable learning từ work sang docs/Decision/policy/harness.

Các capability này được dùng bởi `pulse-orient`, `pulse-shape`, `pulse-plan`, `pulse-implement`, `pulse-review` và `pulse-improve-harness`.

## Acceptance scenarios

1. Public API Ticket không `ready` khi documentation impact là `unknown`.
2. Internal refactor risk thấp dùng `none` + rationale và không bị ép tạo docs thừa.
3. `work packet` route đúng architecture/domain docs từ explicit references và code anchors.
4. Migration backup không xuất hiện trong current execution packet.
5. Product contract và Story QA baseline mâu thuẫn làm gate fail với actionable finding.
6. Broken link hoặc stale generated docs chặn close khi verification profile yêu cầu.
7. Documentation receipt invalid sau khi file thay đổi và content hash không còn khớp.
8. Work artifact chứa invariant mới nhưng chưa promote/classify/defer làm close gate fail.
9. Hai authoritative docs mâu thuẫn tạo `docs_conflict`; kernel không tự chọn một bên.
10. Retired/superseded doc không được route như current context.
11. Brownfield onboarding không overwrite/move semantic docs trước human approval.
12. `AGENTS.md` vẫn là map ngắn; detailed knowledge được route tới owned docs.
13. Agent nhận bounded docs context và không phải grep toàn bộ `docs/` để tìm assignment contract.
14. Documentation-only Ticket chạy lightweight verification profile và tạo valid receipt.
15. Public behavior change không close nếu required product docs/examples chưa cập nhật.
16. Generated docs bị sửa tay hoặc stale được phát hiện deterministic.
17. Repeated docs finding tạo harness Ticket và regression eval.
18. Offline vẫn query được docs registry, owner, authority và applicability.

## Core v1 boundary

Core v1 tối thiểu phải có:

- Folder/source hierarchy được khóa.
- Top-level `works/` tách khỏi `.pulse/workgraph/`.
- Document registry schema và read/query support.
- Ticket documentation impact posture.
- Applicable docs trong `work packet`.
- Docs validation receipt source/content-bound.
- Generated freshness contract tối thiểu.
- Promotion candidates trong handoff.
- Docs-specific doctor findings.
- Brownfield backup/exclusion policy.

Có thể defer sau Core v1:

- Automatic semantic contradiction detection nâng cao.
- Section-level extraction hoàn toàn tự động.
- Cross-repository documentation graph.
- Automatic ownership inference.
- Rich analytics và AI-generated maintenance.
