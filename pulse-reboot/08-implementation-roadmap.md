# Implementation Roadmap

[Trang vào](../PULSE_REBOOT.md) | [Decisions và DoD](09-decisions-and-dod.md)

**Đọc khi:** cần chọn technology, salvage code, chia phase hoặc kiểm tra scope hoàn thành.
**Sở hữu:** implementation strategy, phases, acceptance scenarios và delivery risks.

## Technology direction

### Runtime

- Pulse core (kernel, graph store, CAS/lifecycle, docs extraction, BM25 retrieval, CLI, process runner, events, evidence) bằng **Rust stable** để đa nền tảng, deterministic và phát hành một binary tĩnh `pulse`. Lý do: #1 rủi ro kỹ thuật là multi-process CAS/locking trên JSON (xem D-21 và mục Delivery risks), là chỗ `Result` exhaustive + RAII guard (`Drop`) của Rust mạnh nhất. Storage primitive triển khai fresh; `references/` chỉ tham khảo pattern (RAII guard + lock + validate-before-rename), không port code.
- Harness layer (skills `.mjs`, hooks, target-repo scripts, optional MCP adapter) giữ **JavaScript/ESM** vì đây là ngôn ngữ tự nhiên của agent ecosystem và semantic judgment (D-07). Lớp này gọi core qua CLI boundary (JSON in/out), không chia process state với kernel.
- CLI là interface chính; library API ở dưới, không nhét logic vào command handlers.
- JSON Schema hoặc tương đương cho work items/events/receipts.
- Process execution qua một runner duy nhất có timeout, cancellation, redaction và structured result.

### Storage

- Một JSON file cho mỗi canonical node và typed edge; top-level `works/` chứa human-facing Markdown work content.
- Immutable one-event-per-file JSON cho semantic audit; raw runtime logs có thể gitignored.
- Content-addressed filesystem cho evidence/artifacts.
- Shared local JSON state + lock/atomic rename cho lease, registry và cursors.
- Không dùng SQLite trong Core v1. Full graph snapshot/cache phải disposable và gitignored.

### Distribution

- Bắt đầu bằng workspace package + executable `pulse`.
- Không build daemon bắt buộc trong Core v1.
- Orchestration có thể thêm local service khi cần long-lived wait/heartbeat.

## Giữ, viết lại, archive

### Giữ ý tưởng và salvage có chọn lọc

- Config discovery và repository bootstrap hữu ích.
- Process/log/evidence utilities đã có test tốt.
- Worktree helpers và redaction.
- Verification/profile concepts.
- Reference snapshots trong `references/`.

Mỗi phần salvage phải đi qua contract/test mới; không kéo cả dependency graph cũ vào vì tiện. `references/` là reference-only: học pattern, không port code.

### Viết lại

- Work graph schema và mutations.
- CLI graph projection và bounded execution packet.
- Run state machine theo events/receipts.
- CLI surface nhỏ và nhất quán.
- Skill/script/tool/hook contracts.
- Doctor output thành actionable findings.
- Agent Registry, thread transport và specific-assignee lease.

### Archive khỏi active architecture

- Fixed phase pipeline bắt mọi task tạo cùng artifacts.
- State bị chia cho nhiều adapter cùng quyền ghi.
- Provider abstraction chưa có usage thật.
- Implicit transition dựa trên chat text.
- UI dispatch giả nhưng không tạo/điều phối Agent thật.

## Phases

### Phase 0 - Khóa direction và archive

- Accept decision register.
- Chụp current behavior/tests để biết phần salvage được.
- Đưa code cũ không còn direction vào archive branch/tag, không duy trì song song.
- Tạo fixture repository và baseline evals.

**Exit:** direction/DoD rõ; không còn tranh cãi code cũ là compatibility target mặc định.

### Phase 1 - Local work graph và documentation foundations

**Implementation status:** foundation này đã được implement qua Slice 1–7.
Slice 7 shaping/readiness/frontier contract được verify tại commit `677c593`
với 340 tests pass. Exit này chỉ xác nhận Phase 1 kernel foundations; Phase 2
runner/lease/work packet và Phase 3 QA baseline/case resolver vẫn chưa hoàn
thành.

- `manifest.json`, node/edge JSON Schemas và sharded file layout.
- Top-level `works/` boundary, document registry schema và durable docs source hierarchy.
- Node/edge create/show/list/edit với revision CAS, deterministic edge IDs và atomic rename.
- Lifecycle, inverse projections, dependency cycles, readiness và supersession.
- Final node schema baseline v1 fields: separate `contract_revision`, typed Ticket role `implementation|decision_work`, `risk`/`materialization` cho phép explicit `unassessed` domain value khi classification chưa đủ chắc, implementation/decision-work contracts và minimal QA-impact posture.
- `graph validate`, `graph neighborhood`, `graph export` và disposable fingerprinted cache.
- Immutable semantic event files.
- Receipt store/validator tối thiểu, gồm source/content-bound documentation receipt và shaping receipt schema/reference contract.
- Knowledge manifest/schema/store tối thiểu cho one-learning-per-record, revision CAS, provenance relations, status/confidence/applicability/promotion và disposable index boundary.
- Ready projection cho critical branch disposition, authority/approval và remaining uncertainty; kernel chỉ validate typed contracts, bindings, revisions, hashes và policy, không tự đánh giá semantic clarity.
- Shaping receipt contract + immutable Decision acceptance proof, receipt-first current pointer apply/invalidate và narrow readiness fingerprint không tự stale khi status/pointer-only revision đổi.
- Tracked default-deny `.pulse/policy/authority.json` tách khỏi `PULSE.md` intent và `.pulse/config.yaml` operational settings; gồm explicit shaping/Decision/transition/docs/QA grants.
- Minimal QA readiness boundary: `unknown` block; `none` cần rationale + `qa.none.approve`; `covered_by_story_close` cần Story owner/rationale + `qa.defer_to_story_close`; `required` unavailable tới Phase 3 baseline/case resolver.
- Shaping-map reference/revision, destination/exit condition, canonical resolution pointers, bounded fog entries và derived decision/execution frontier queries; pre-lease output dùng `claim_state=not_evaluated`.
- `pulse docs list|show|applicable` và exclusion của retired/migration docs.
- Heading-aware section extraction, generated `_index.md`, disposable BM25+ index và `pulse docs index|status|search|get|tree`.

**Exit:** Ticket local-first được tạo, sửa, link, query, shape và transition tới current `ready` deterministic; authority/QA/docs/Decision/shaping inputs được projection thành explainable readiness; decision/execution frontiers rebuild deterministic; applicable durable docs và ranked sections được resolve deterministic; xóa cache/projection rồi rebuild không làm đổi canonical truth hoặc expected retrieval semantics. `ready -> active`, lease/workspace, full work packet, full QA baseline/case validation và close gate vẫn thuộc Phase 2/3.

### Phase 2 - Single-agent run

**Current implementation status:** Phase 2 Slice 1 preview `WorkPacketV1` and
Slice 2 atomic reservation + workspace binding are implemented and verified.

Slice 1 (packet foundation): implemented in
[`proposals/phase2-slice1-work-packet-dispatch-foundation.md`](../proposals/phase2-slice1-work-packet-dispatch-foundation.md),
verified through commit `6d3076b` (458 tests).

Slice 2 (atomic reservation + workspace binding): implemented in
[`proposals/phase2-slice2-atomic-reservation-workspace-binding.md`](../proposals/phase2-slice2-atomic-reservation-workspace-binding.md),
verified through implementation hardening commit `e6c6402`, I11 documentation
commit `d21282c`, and final verifier/fixer commit `428f149` (675 tests).
Implementation includes assignment value contracts, runtime lease/workspace/
prepared-assignment store, multi-target transaction extensions, capability
inventory matching, in-place and isolated worktree binding, fence-aware packet
revalidation, `ready -> active` lifecycle gate, `work claim`/`work release`/
`work leases`/`work leases recover` commands, concurrency hardening and
claim-state enriched execution frontier.

Phase 2 as a whole is not complete: Pulse still has no runner/cancel/resume,
handoff/verification receipts, proof-driven close gate or documentation impact
promotion. Those remain the next Phase 2 slices.

- Codex adapter cho một run.
- Minimal `pulse-shape` path để tạo/review shaping result trước dispatch; đủ cho R0/R1 và làm nền cho capability pack đầy đủ ở Phase 3.
- Shaping reconciliation mutation path: persist resolution, update pointers, graduate precise fog, supersede invalid branches và recompute frontier/readiness bằng graph CAS.
- `pulse work packet` và prompt builder dùng bounded execution packet gồm shaping result/branch dispositions/destination revision cùng required/optional/write-candidate docs, section refs và read budget.
- Process runner, cancellation, logs và resume.
- Handoff, verification profile, documentation impact/promotion candidates và close gate.
- Worktree isolation cho risk vừa/cao (Slice 2 đã hỗ trợ cơ bản qua workspace binding).

**Exit:** một Ticket đi từ `ready` đến `done/rework/blocked` bằng proof, resume được sau interruption.

### Phase 3 - Harness capability packs

- Core skills: orient, shape, plan, implement, debug, review, QA.
- Hoàn thiện `pulse-shape` + reusable decision-tree grilling/wayfinding primitive trên contract Phase 1/2: repo-grounded questions, one-question-at-a-time flow, recommended answers, destination, frontier, fog và risk-adaptive materialization.
- Gap routing cho research, human grilling, Decision, prototype và enabling work; persisted map chỉ dùng khi multi-session/risk policy yêu cầu.
- Reviewer/eval cho shaping receipt để ready gate kiểm tra source revisions, branch summary, destination, bounded fog, authority và remaining uncertainty mà kernel không giả làm semantic planner.
- Script/tool/hook manifests.
- `pulse init` và `pulse doctor`.
- Docs impact/update/review capability references, link/generated freshness checks và `pulse docs validate`.
- Story QA baseline parser/projection với QA scope, acceptance/risk coverage matrix, stable case IDs, applicability và exit criteria.
- Ticket QA impact + affected-case selection; targeted checkpoint và full Story qualification là hai execution scopes trên cùng baseline.
- QA environment lifecycle: start, healthcheck, fixture seed/reset, cleanup và source-to-build/deployment binding.
- Executor capability manifests/adapters cho Playwright, browser agent, Chrome DevTools observation, structured HTTP/API, shell/PTY CLI và structured manual fallback; data/platform adapters có thể thêm theo fixture needs.
- QA receipt validation cho case/baseline revision, environment/fixture identity, required observations/artifacts, actor independence, retry/flaky và waiver policy.
- QA case generation/review skill ground trên acceptance, Decisions, risks, prior defects, supported matrix và child Ticket impacts.

**Exit:** target repo mới bootstrap được; ít nhất một web scenario, một API hoặc CLI non-browser scenario và một documentation validation tạo receipt hợp lệ; targeted Ticket checkpoint và full Story qualification được phân biệt bằng gate/receipt.

### Phase 4 - Knowledge compounding và ratchet loop

- Continuous learning-candidate capture từ Worker/reviewer/QA/doctor/failure handoffs.
- `pulse-compound` synthesis: gather, deduplicate, classify, validate provenance/applicability, reconcile contradiction và disposition candidates.
- Knowledge lifecycle/mutations, typed relations, promotion history và freshness/retirement.
- `pulse knowledge search|get|applicable|index|status` với applicability filter trước BM25 ranking, explainable required/recommended/suggested/excluded buckets.
- Role/moment-specific bounded knowledge injection vào shaping/planning/execution/debug/verification/review packets.
- Usage feedback, reinforcement/noise signals và historical known-failure retrieval evals.
- Failure classification và harness work items trong cùng graph.
- Eval runner và fixture/replay suites.
- Promotion workflow từ finding/learning sang docs/Decision/skill/check/hook/policy/eval.
- Documentation drift/promotion/retrieval findings và metrics report.
- Optional semantic adapter spike chỉ khi lexical + typed applicability eval cho thấy recall gap đáng kể.

**Exit:** một failure thật, gồm ít nhất một context/docs failure, được capture thành validated applicable learning, retrieve cho fixture work, promote thành harness/docs change + eval và chứng minh không tái phát; irrelevant work không bị inject learning đó.

### Phase 5 - Peer-agent orchestration

- Codex independent-task transport.
- Agent Registry/presence.
- Specific-assignee lease và typed mailbox.
- Single-Worker Orchestration Agent loop.
- Independent Reviewer/QA Agent, gồm frozen source/content docs review.
- Source/docs scope, contract revision redirect và canonical-doc conflict advisory.
- Multi-Worker conflict advisory, recovery và human takeover.

**Exit:** các scenarios trong [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md) pass; task/thread đều user-visible và recoverable.

## Core acceptance scenarios

1. Init một fixture repo tạo đúng map/policy/config tối thiểu, không overwrite file user.
2. Tạo standalone Ticket không cần Story/Epic.
3. Tạo Epic -> Story -> Tickets thành independent node/edge files và derive roll-up/inverse relations.
4. Hard blocker ngăn dispatch; soft preference chỉ ảnh hưởng reconciliation.
5. Hai writer cùng node revision: một CAS mutation bị từ chối rõ ràng; hai node khác nhau không tạo shared-file conflict.
6. Edge retry với cùng `(type, from, to)` không tạo duplicate; dangling/cyclic edge bị reject.
7. Xóa `.pulse/cache/workgraph.snapshot.json`, chạy lại `graph export` cho cùng canonical fingerprint và semantics.
8. `work packet` trả đủ Ticket, parent context, Decisions, applicable docs, edges và gates mà không yêu cầu Agent search raw graph files hoặc toàn bộ docs tree.
9. Implementation Ticket không `ready` khi thiếu code anchors/invariants/implementation mode, trừ discovery Ticket hợp lệ.
10. Ticket risk thấp chạy không bị ép tạo plan/design thừa.
11. Ticket risk cao không close khi thiếu required Decision/review/rollback proof.
12. Agent bị interrupt và resume đúng run/source/work state.
13. Handoff `passed` nhưng artifact hash sai bị gate từ chối.
14. Ticket validation pass nhưng Story baseline fail thì Story không đóng.
15. Browser/Playwright receipt gắn đúng source snapshot và artifacts.
16. Ticket X bị Y hấp thụ chuyển `superseded`, acceptance chưa cover được chuyển đúng.
17. Reconciliation chọn P2 foundation trước P0 và ghi timebox/rationale/revisit trigger.
18. Doctor finding và failure replay tạo harness Ticket + eval ngăn tái phát.
19. Public API Ticket không `ready` khi documentation impact là `unknown`; internal refactor có thể dùng `none` + rationale.
20. `work packet` route đúng applicable docs và không route migration backup/retired docs.
21. Product contract và Story QA baseline mâu thuẫn làm gate fail thay vì kernel tự chọn một bên.
22. Documentation receipt invalid khi source/content hash đổi sau review.
23. Work artifact chứa durable invariant nhưng chưa promote/classify/defer làm close fail.
24. Broken link, stale generated docs hoặc forbidden generated hand edit bị detect theo profile.
25. Brownfield docs restructure snapshot trước và không move/overwrite semantic docs nếu chưa có human approval.
26. Offline query được docs registry, owner, authority và applicability.
27. `pulse docs search` trả đúng section với ID/heading/range/hash và không đọc full corpus vào Agent context.
28. Xóa docs-search cache và generated `_index.md`, rebuild cho deterministic fingerprint/projection và equivalent expected ranking.
29. Retired/stale/migration/generated-navigation docs được exclude/label đúng policy.
30. Work packet trả required + suggested section refs cùng read budget, không inline toàn bộ top hits.
31. Incremental reindex chỉ cập nhật changed docs; corrupt cache bị discard/rebuild.
32. Retrieval eval cover exact identifier, natural-language paraphrase, Vietnamese/tokenization, no-result và context budget.
33. Feature request mơ hồ được `pulse-shape` ground bằng owning work, Decisions, durable docs và code evidence trước khi hỏi human; các câu hỏi còn lại đi từng câu theo dependency order và có recommended answer khi có strong default.
34. Ticket không `ready` khi một critical branch chưa disposition; `delegated` vượt implementation freedom hoặc `deferred` thiếu owner/target, reason, trigger/linked work đều bị reject.
35. R0 correction rõ, low-risk qua concise contract + ambiguity self-check mà không bị ép tạo `work-brief.md`, ADR, plan hoặc một interview với human.
36. Worker phát hiện ambiguity có thể đổi objective/acceptance/invariant thì dừng với `decision_request` hoặc re-shape proposal; reversible choice trong implementation freedom không làm gián đoạn execution.
37. Multi-session shaping khóa destination + exit condition trước khi fan out; map chỉ gist/link canonical resolutions và decision frontier được derive từ open, unblocked, unclaimed decision work.
38. In-scope uncertainty chưa thể viết thành precise question ở `not_yet_specified`; khi evidence làm nó sắc nét, reconciliation tạo typed decision work có provenance thay vì map speculative Tickets upfront.
39. Resolve một decision cập nhật canonical answer/pointer, graph dependencies và affected readiness; invalidated branches được reject/cancel/supersede, newly visible questions được materialize hoặc giữ fog có rationale.
40. `pulse work frontier --kind decision` và `--kind execution` trả hai projection khác nhau trên cùng graph fingerprint; xóa cache/rebuild không đổi semantics và claim/lease không bị persist thành canonical status.
41. Story `qa.md` map mọi required acceptance/protected risk sang behavioral case, Ticket proof hoặc authorized limitation; coverage gap bắt buộc làm gate fail.
42. Behavior-affecting Ticket không `ready` khi QA impact là `unknown`; `none` hoặc `covered_by_story_close` cần rationale hợp policy.
43. Ticket checkpoint chỉ chọn affected/new Story cases và receipt ghi `qa_scope=ticket_checkpoint`; pass checkpoint không tự thay full Story qualification.
44. Story close replay full applicable baseline trên integrated/frozen candidate và receipt ghi `qa_scope=story_close`; all Ticket checkpoints pass nhưng cross-Ticket case fail vẫn chặn Story.
45. Cùng một Playwright/API/CLI test có thể tạo developer-verification evidence và QA receipt, nhưng gate không nhập hai purpose nếu thiếu scope/actor/source/acceptance mapping.
46. Web critical case chạy qua deterministic Playwright assertions; browser agent/Chrome DevTools có thể bổ sung semantic/diagnostic evidence nhưng “looks good” không tạo pass.
47. API case validate status/schema/side effect qua structured HTTP executor; CLI interactive case validate prompt/signal/exit/filesystem qua PTY executor.
48. Environment start/healthcheck/reset failure được classify `environment_failure`, selector/assertion/executor hỏng là `test_failure`; không tự requeue product implementation như `product_failure`.
49. Attempt đầu fail rồi retry pass giữ cả hai receipts và case thành `flaky` theo policy, không che failed attempt; required flaky/inconclusive case chặn close nếu chưa waiver hợp lệ.
50. Case hoặc baseline revision, fixture identity, source/build artifact hay required artifact hash đổi làm receipt cũ invalid theo validity policy.
51. Standalone user-visible Ticket có thể own QA baseline khi không có Story; internal-only Ticket có thể chỉ dùng validation với QA `none` rationale.
52. Worker/QA Agent được đề xuất case hoặc applicability mới nhưng không tự đổi expected behavior/acceptance; semantic change thiếu authority làm reconciliation/close fail.
53. Compound tạo one-learning-per-record có actionable guidance, typed applicability và provenance tới work/run/receipt/finding; vague/non-reusable item được classify `non_durable`.
54. Compound search prior canonical learnings trước create; same semantic scope update/corroborate thay vì duplicate, contradiction tạo disputed/reconciliation path.
55. Candidate chưa reviewed, superseded, retired hoặc disputed không auto-inject; default `knowledge search` trả bounded summaries và explicit `get` mới trả detail/provenance.
56. `knowledge applicable --work` explain typed domain/path/symbol/operation/risk matches; explicit exclusions/version mismatch thắng lexical similarity.
57. Same work trả role-specific buckets: planner có decision/failure patterns, Worker có narrow corrections/ratchets, validator có required checks.
58. Validated applicable ratchet route `required`; suggested learning không bị Agent/kernel tự nâng thành required canonical context.
59. Learning mâu thuẫn accepted Decision/current product contract bị exclude/dispute và tạo finding, không override authority bằng ranking.
60. Learning promote tới docs/eval/check giữ typed target links/hashes; chỉ lưu learning không vượt documentation gate cho durable invariant.
61. Xóa/rebuild knowledge index giữ eligible set/fingerprint/tie-break semantics; corrupt/stale cache được discard.
62. Historical failure fixture retrieve expected learning top-K; irrelevant fixture không bị false inject và context budget được đo.
63. Usage feedback ghi injected/opened/applied/outcome; repeated miss/noise tạo retrieval/applicability/guardrail work thay vì tăng corpus vô hạn.
64. Secret/untrusted prompt text trong candidate bị redact/review gate chặn trước publish/index; required routing cần validated/enforced authority.
65. R3/costly repeated incident required compound nhưng bị skip tạo defer/gate finding; low-risk cycle có thể kết luận `no_reusable_learning`.

## Orchestration acceptance scenarios

- **66.** Orchestrator tạo independent Codex task và assign Ticket cho identity cụ thể từ CLI execution packet.
- **67.** Assignment không acknowledged hết hạn, Ticket trở về executable và không ghost lease.
- **68.** Orchestrator restart recover Agent/thread/lease/mailbox cursors.
- **69.** Hai dispatch cạnh tranh không tạo hai exclusive Workers.
- **70.** Worker blocker được route tới Orchestrator/human và resume cùng thread.
- **71.** Reviewer/QA Agent dùng frozen snapshot, tạo valid receipt.
- **72.** Human takeover task; Orchestrator chuyển observe, không gửi lệnh xung đột.
- **73.** Worker không thể đóng Story, đổi acceptance/approved docs, merge hoặc deploy vượt policy.
- **74.** Direct delivery fail chỉ mang `fallback_stored`, không giả acknowledged.
- **75.** Active Ticket bị supersede giữ partial handoff/evidence và cleanup an toàn.

## Delivery risks

### Abstraction quá sớm

Provider-neutral interface có thể che mất Codex capabilities. Giảm thiểu: Codex-first, extract interface sau hai implementations hoặc nhiều usage thật.

### Local file locking và corruption

Multi-process CAS trên JSON có thể mong manh nếu mỗi command tự invent locking. Giảm thiểu: một storage library sở hữu repository-scoped lock, expected revision, fsync/atomic rename, deterministic recovery và crash/concurrency tests. Không dùng SQLite để né việc định nghĩa đúng mutation protocol.

### Doctor trở thành checklist đẹp

Giảm thiểu: finding phải có evidence, impact, proposed Ticket và verification.

### Evidence phình và lộ secret

Giảm thiểu: content-addressing, retention policy, redaction, protected artifacts và size budget.

### Documentation registry/search thành ceremony hoặc nguồn sự thật thứ hai

Giảm thiểu: chỉ register durable/routeable docs, content vẫn là normal Git files, một writable registry, generated `_index.md`/search cache disposable, section search trả bounded snippets và Ticket risk thấp được phép `none` + rationale.

### Semantic search kéo model stack vào Core quá sớm

Giảm thiểu: Core v1 chỉ lexical BM25+ pure-Rust (tantivy); embeddings, QMD adapter, RRF hybrid và reranker chỉ thêm khi retrieval eval chứng minh lexical recall không đủ.

### Orchestrator có quyền quá lớn

Giảm thiểu: capability matrix, conductor-owned gates, human decision boundaries, audit mọi override.

### Concurrency trước reliability

Giảm thiểu: Phase 5 không bắt đầu trước khi resume, lease, receipt và close gate single-agent pass.

### Duy trì hai kiến trúc

Giảm thiểu: archive rõ, brownfield/product migration một chiều khi cần, không compatibility layer vô thời hạn; implementation slices nội bộ không tạo schema bridge hoặc compatibility target riêng.
