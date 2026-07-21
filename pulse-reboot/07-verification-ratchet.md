# Verification, Doctor Và Harness Ratchet

[Trang vào](../PULSE_REBOOT.md) | [Runtime harness](04-runtime-harness.md) | [Story QA](03-story-qa.md) | [Documentation system](10-documentation-system.md) | [Knowledge compounding](12-knowledge-compounding.md)

**Đọc khi:** cần thiết kế verify/review/doctor/eval hoặc biến failure thành cải tiến harness.
**Sở hữu:** verification profiles, review layers, doctor dimensions, failure taxonomy, harness backlog và metrics.

## Outcome invariants

Bất kể run path nào, Pulse phải giữ:

- Source snapshot được nhận diện chính xác.
- Acceptance map tới evidence.
- Required checks pass hoặc waiver có authority/audit.
- Không claim `done` bằng text tự do.
- Failure giữ đủ context để reproduce.
- Evidence không chứa secret ngoài policy.
- Required durable docs phản ánh intended/implemented behavior và có source/content-bound proof.

## Hai trục assurance

Pulse không đồng nhất test level với assurance purpose.

### Test level/tool

- Unit/component.
- Integration/contract.
- End-to-end/system.
- Static/build/schema checks.
- Exploratory/manual observation.

### Assurance purpose/gate

- **Developer verification:** Worker chứng minh implementation Ticket được xây đúng.
- **Independent code review:** reviewer tìm correctness, regression, security và design gap.
- **Ticket QA checkpoint:** focused behavioral replay cho affected Story cases khi impact/risk yêu cầu.
- **Story qualification:** full applicable behavioral baseline trên integrated/frozen candidate snapshot.
- **Release qualification:** optional cross-Story/platform/config regression theo release policy.

Một Playwright/API/CLI test có thể phục vụ nhiều purpose, nhưng receipt phải ghi rõ scope, actor, source snapshot và acceptance/risk mapping. Developer E2E pass không tự động thay Story qualification receipt.

## Verification profiles

Profile nằm trong target repository, không hard-code trong Pulse:

```yaml
profiles:
  docs-only:
    commands: ["pnpm lint:docs"]
    review: light
  service-change:
    commands: ["pnpm lint", "pnpm test --filter service"]
    review: standard
  web-behavior:
    commands: ["pnpm lint", "pnpm test"]
    ticket_qa_checkpoint: impact-driven
    story_qa: required
    qa_environment: local-web
  migration-high-risk:
    commands: ["pnpm test:migrations"]
    review: independent
    rollback_receipt: required
    docs_review: independent
  documentation-only:
    commands: ["pnpm lint:docs", "pnpm check:links"]
    review: light
  public-contract-change:
    commands: ["pnpm check:links", "pnpm test:docs-examples", "pnpm generate:api --check"]
    docs_review: independent
```

Profile resolution dùng Ticket risk, changed paths và explicit policy. Agent có thể đề xuất nâng profile; kernel validate required minimum.

## Review layers

- **Self-check:** Worker rà diff, scope, acceptance, tests.
- **Mechanical review:** lint, typecheck, tests, forbidden patterns, schema.
- **Independent code review:** correctness, regression, security, missing tests, architecture fit.
- **Ticket QA checkpoint:** replay affected/new Story cases khi Ticket impact/risk yêu cầu.
- **Story qualification:** replay full applicable Story baseline trên integrated/frozen runnable surface.
- **Release qualification:** cross-Story/platform/config regression nếu release policy yêu cầu.
- **Documentation review:** check links/generated freshness, intended-vs-observed behavior, ownership, duplicate truth và promotion.
- **Human gate:** business/security/destructive/production boundaries.

Không bắt mọi Ticket đi qua mọi layer. Policy map risk sang required layers.

## `pulse doctor`

Doctor đánh giá repository readiness theo capability:

- **Discoverability:** entrypoints, docs map, ownership.
- **Buildability:** clean install/build từ declared command.
- **Testability:** focused tests, deterministic fixtures, timeout.
- **Debuggability:** logs, local repro, error taxonomy.
- **Verifiability:** profiles, evidence adapters, QA surfaces, environment lifecycle, fixture isolation/reset, executor compatibility và coverage gaps.
- **Safety:** secrets, destructive commands, protected areas.
- **Recoverability:** work state, workspace cleanup, resumable runs.
- **Agent legibility:** naming, boundaries, generated code, dependency rules.

Output không chỉ là score:

```text
finding -> evidence -> impact -> suggested work item -> suggested verification
```

Doctor không tự sửa hàng loạt. Nó đề xuất harness work vào cùng local work graph, có priority và relation với product work.

## Failure classification

Sau run/review/QA failure, phân loại:

- `context_gap`: Agent không tìm thấy rule/file cần thiết.
- `tool_gap`: thiếu capability/executor.
- `guardrail_gap`: invariant chỉ được phát hiện muộn.
- `verification_gap`: checks pass nhưng behavior sai.
- `task_shape_gap`: Ticket mơ hồ/quá lớn/sai dependency.
- `policy_gap`: authority hoặc expected behavior không rõ.
- `environment_gap`: fixture/service/setup không ổn định.
- `test_failure`: case implementation, selector, assertion hoặc executor sai.
- `inconclusive_evidence`: execution không tạo đủ observation/artifact để kết luận.
- `flaky_behavior_or_test`: cùng source/environment class cho kết quả không ổn định, cần triage product vs harness.
- `model_execution_error`: capability có nhưng Agent dùng sai.
- `product_defect`: code hiện tại sai, harness không nhất thiết thiếu.
- Documentation findings chi tiết dùng taxonomy `docs_missing`, `docs_stale`, `docs_conflict`, `docs_orphaned`, `docs_duplicate_truth`, `docs_generated_stale`, `docs_unverified_example`, `docs_work_leak`, `docs_policy_gap`, `docs_context_gap`.

Một failure có primary class và contributing classes.

## Ratchet loop

Failure ratchet sở hữu đường từ classified failure tới executable prevention. Learning record schema, applicability recall và general compounding lifecycle thuộc [`12-knowledge-compounding.md`](12-knowledge-compounding.md).

```text
failure
  -> classify with evidence
  -> capture/update failure-pattern learning
  -> decide local fix vs harness fix
  -> create/link harness Ticket
  -> add docs/Decision/skill/script/check/eval
  -> verify on original case
  -> run regression eval
  -> promote stable guardrail
  -> retrieve ratchet on future applicable work
```

Promotion ladder:

```text
one-off finding
  -> reviewer checklist
  -> skill guidance
  -> deterministic script/check
  -> blocking hook/policy
```

Chỉ promote khi signal ổn định và false-positive chấp nhận được. Không biến mọi review comment thành hook.

## Harness backlog

Harness work dùng cùng Epic/Story/Ticket model, không có backlog bí mật thứ hai. Ví dụ:

- `TK-H-012`: tạo focused test command cho auth package.
- `TK-H-013`: thêm browser fixture reset.
- `ST-H-004`: giảm context gap trong payment domain.

Quan hệ foundation được model bằng `preferred_after`/`blocked_by`, giúp semantic reconciliation cân nhắc harness work trước product work khi hợp lý.

## Eval pyramid

1. Unit tests cho schema, transition, receipt validator, lease/CAS.
2. Integration tests cho CLI, git/worktree, process runner, event recovery.
3. Scenario evals trên fixture repositories, gồm docs routing, impact, drift, generated freshness và retrieval top-K/context-budget quality.
4. Replay failure thật đã được sanitize.
5. End-to-end agent eval, single-agent trước rồi peer-agent.

## Metrics

Metrics chính:

- Correct completion rate theo risk profile.
- Human interventions trên một completed Ticket.
- Rework rate và escaped regression.
- Time to first useful action.
- Verification/QA/documentation receipt validity rate.
- Acceptance/risk-to-QA coverage gap rate.
- Ticket checkpoint defect yield và Story-close escaped integration defect rate.
- Flaky/inconclusive rate theo executor, environment và case.
- Applicable-doc routing accuracy và repeated docs-context gap rate.
- Docs retrieval Recall@K/MRR, stale/retired exclusion accuracy và context bytes trước first useful section.
- Resume/recovery success rate.
- Repeated failure rate theo class.
- Applicable validated-learning retrieval miss và required-ratchet miss rate.
- Same failure after learning injection, phân biệt retrieval/apply/guardrail gap.
- Harness improvement lead time và learning-to-guardrail/eval lead time.
- Multi-agent duplicate/conflict dispatch rate.

Không tối ưu trực tiếp:

- Số Agent chạy song song.
- Số generated artifacts.
- Số tool calls.
- Tỷ lệ Ticket mang `done` nếu gate yếu.

## Feedback ownership

- Worker đề xuất failure class, learning candidate và harness improvement.
- Reviewer/QA bổ sung independent evidence, correction/ratchet candidate.
- Compound capability deduplicate, validate applicability/provenance và route promotion.
- Orchestration Agent link pattern giữa nhiều runs.
- Human quyết định promotion có ảnh hưởng policy lớn.
- Kernel aggregate/validate/search; không tự kết luận root cause semantic hoặc nâng learning thành policy.
