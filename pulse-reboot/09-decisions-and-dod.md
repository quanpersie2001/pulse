# Decisions Và Definition Of Done

[Trang vào](../PULSE_REBOOT.md) | [Roadmap](08-implementation-roadmap.md)

**Đọc khi:** cần khóa direction hoặc quyết định một milestone đã thực sự hoàn thành chưa.
**Sở hữu:** decision register và milestone DoD.

## Decision register đề xuất

| ID | Quyết định | Trạng thái đề xuất |
|---|---|---|
| D-01 | Pulse là local-first harness engineering system, không phải workflow engine | Accept |
| D-02 | Repository + `.pulse` là system of record; tracker ngoài là adapter | Accept |
| D-03 | Epic/Story là optional hierarchy; Ticket là executable unit | Accept |
| D-04 | Hierarchy, dependency, priority và supersession là các khái niệm riêng | Accept |
| D-05 | Artifact materialize theo risk `R0..R3`, không theo phase cố định | Accept |
| D-06 | Canonical work, runtime coordination và immutable evidence tách lớp | Accept |
| D-07 | Deterministic mechanism thuộc kernel; semantic judgment thuộc Agent skills | Accept |
| D-08 | Codex-native provider trước; Claude-native và ACP generic thêm sau contract/use thật, không ép lowest-common-denominator | Accept |
| D-09 | Core, Runtime và Orchestration là ba milestone: Core repository semantics trước, Daemon single-Agent reliability tiếp theo, multi-Agent sau cùng | Accept |
| D-10 | Multi-agent dùng independent daemon-managed sessions làm orchestration units; runtime parentage không tạo business authority | Accept |
| D-11 | Orchestrator có user-equivalent transport nhưng bounded authority | Accept |
| D-12 | Assignment là Core reservation bind exact Ticket revision/packet và opaque Daemon workspace/session; delivery khác acknowledgement | Accept |
| D-13 | Worker chỉ submit handoff; Ticket/Story close qua conductor-owned gate | Accept |
| D-14 | QA là behavioral ledger với typed receipts; Playwright/browser/API chỉ là executors | Accept |
| D-15 | Priority là input semantic; reconciliation có thể chọn foundation work trước P0 | Accept |
| D-16 | Failure lặp lại phải đi vào cùng work graph như harness improvement + eval | Accept |
| D-17 | Node bị hấp thụ dùng `superseded`, không giả `done` hoặc xóa lịch sử | Accept |
| D-18 | Canonical graph là sharded `nodes/*.json` + `edges/*.json`; human-facing work content là Markdown dưới top-level `works/` | Accept |
| D-19 | Full graph JSON là derived CLI projection/cache, không phải writable tracked truth | Accept |
| D-20 | Agent/Orchestrator đọc và mutate graph qua CLI/API, không tự search/parse raw graph files | Accept |
| D-21 | Core canonical correctness không phụ thuộc SQLite; Daemon runtime persistence là implementation decision riêng và không được trở thành work truth | Accept |
| D-22 | Pulse Core và Pulse Daemon cùng bằng Rust stable trong một executable `pulse`; skills/hooks/repository scripts có thể dùng JavaScript/ESM qua stable boundary | Accept |
| D-23 | Durable repository documentation là first-class Pulse capability | Accept |
| D-24 | `docs/`/`AGENTS.md`/`PULSE.md` giữ durable knowledge; work prose, evidence và runtime không phải current docs truth | Accept |
| D-25 | Human-facing work content nằm ở top-level `works/`; machine graph metadata nằm trong `.pulse/workgraph/` | Accept |
| D-26 | Mỗi implementation Ticket có documentation impact posture và close gate | Accept |
| D-27 | Execution packet route applicable docs; Agent không tự scan toàn bộ docs tree | Accept |
| D-28 | Accepted Decision/product contract biểu diễn intent; code divergence có thể là defect | Accept |
| D-29 | Documentation receipt phải source-bound và content-bound | Accept |
| D-30 | Durable knowledge trong work artifacts phải promote, classify non-durable hoặc defer có authority | Accept |
| D-31 | Brownfield docs restructure cần snapshot và human approval khi semantic mapping không chắc chắn | Accept |
| D-32 | Generated docs có declared source, generator, output và freshness contract | Accept |
| D-33 | Documentation contradiction được resolve qua owner/Decision, không bằng heuristic kernel | Accept |
| D-34 | `AGENTS.md` là navigation map, không phải monolithic knowledge base | Accept |
| D-35 | Docs retrieval dùng progressive disclosure: tree/search trước, bounded section get sau | Accept |
| D-36 | Core v1 dùng generated `_index.md` + section-level lexical BM25+ cache, không dùng vector/model stack | Accept |
| D-37 | Docs search cache và `_index.md` là disposable/generated projections, không phải writable truth | Accept |
| D-38 | Retrieval unit mặc định là Markdown section với stable document identity, line range và content hash | Accept |
| D-39 | `pulse docs search` trả metadata/snippet; `pulse docs get` mới đọc bounded canonical content; full file là explicit | Accept |
| D-40 | Tantivy (BM25+, field boosting, pure-Rust) làm search engine direction và comrak làm Markdown section parser direction, đằng sau interface; MiniSearch (JS) chỉ còn là reference lesson, không phải contract. Public contract vẫn là section-level lexical ranking, deterministic rebuild, no-native-model dependency | Accept |
| D-41 | Semantic/hybrid retrieval là optional adapter chỉ thêm khi lexical eval chứng minh recall gap | Accept |
| D-42 | Hybrid retrieval dùng rank fusion như RRF, không cộng raw BM25 và vector scores trực tiếp | Accept |
| D-43 | Critical ambiguity phải được disposition trước execution; `pulse-shape` dùng repo-grounded, one-question-at-a-time decision-tree grilling theo risk, không tạo fixed brainstorm phase hoặc bắt buộc một artifact riêng cho mọi Ticket | Accept |
| D-44 | Substantial multi-session shaping dùng approved destination, derived decision frontier và bounded `not_yet_specified`; chỉ materialize precise questions, reconcile graph/readiness sau mỗi resolution, và giữ local work graph làm canonical thay vì tracker map | Accept |
| D-45 | Story là default owner của persistent behavioral QA baseline; child Ticket khai báo impact và reference Story cases thay vì duplicate expected behavior | Accept |
| D-46 | QA có hai execution scopes trên cùng baseline: impact-driven Ticket checkpoint và full Story qualification bắt buộc trước Story close | Accept |
| D-47 | Developer tests và behavioral QA được phân biệt bằng purpose, gate, source/actor/evidence binding, không bằng framework; Playwright/API/CLI test có thể phục vụ cả hai | Accept |
| D-48 | QA executor được resolve theo surface, required capabilities, environment applicability và evidence contract; browser/Playwright/API/CLI chỉ là adapters | Accept |
| D-49 | Failed QA attempts là immutable; retry pass không che failure, required flaky/inconclusive case chặn close trừ waiver có authority | Accept |
| D-50 | Worker/QA Agent có thể đề xuất cases nhưng không tự đổi acceptance/expected behavior; semantic baseline changes cần owning authority và revision reconciliation | Accept |
| D-51 | Standalone behavioral Ticket có thể own QA baseline khi không có Story; internal-only Ticket có thể dùng validation + QA-none rationale | Accept |
| D-52 | Compounding là first-class loop: continuous candidate capture, deliberate synthesis, selective promotion và applicability-aware recall | Accept |
| D-53 | Learning record là reusable guidance + provenance/applicability, không phải current docs/Decision truth hoặc work item | Accept |
| D-54 | Canonical learning store dùng one-learning-per-record sharded JSON + typed relations; optional Markdown chỉ là detail content | Accept |
| D-55 | Knowledge recall filter typed applicability/lifecycle/authority trước lexical BM25 ranking và phải explain match/exclusion | Accept |
| D-56 | `pulse knowledge search/get/applicable` là typed CLI; work packet inject bounded role/moment-specific summaries, không whole memory corpus | Accept |
| D-57 | Candidate/disputed/superseded/retired learning không auto-inject; required routing chỉ từ explicit reference hoặc validated/enforced ratchet/policy | Accept |
| D-58 | Accepted Decision/current authoritative docs không bị learning override; contradiction tạo finding và reconciliation | Accept |
| D-59 | Compound search/deduplicate prior learnings, giữ immutable evidence links, support promotion tới docs/Decision/skill/check/hook/policy/eval | Accept |
| D-60 | Usage/retrieval feedback tham gia reinforce/revise/retire nhưng popularity không tự tạo authority | Accept |
| D-61 | Docs và knowledge có thể reuse lexical engine/cache abstractions nhưng giữ typed corpora, filters, authority và result contracts riêng | Accept |
| D-62 | Node dùng `contract_revision` riêng cho semantic shaping/readiness freshness; lifecycle, timestamp và shaping-pointer-only mutation chỉ tăng normal CAS `revision` | Accept |
| D-63 | Ticket có typed role `implementation|decision_work`; decision work precise có thể vào decision frontier từ draft mà không cần recursive shaping receipt | Accept |
| D-64 | Enforceable authority dùng tracked default-deny `.pulse/policy/authority.json`; `PULSE.md` giữ human intent, `.pulse/config.yaml` giữ operational config, receipt không tự khai grant cần thiết | Accept |
| D-65 | Phase 1 ready có minimal QA gate: `unknown` block; QA `none` cần `qa.none.approve`; `covered_by_story_close` cần `qa.defer_to_story_close`; `required` chờ Phase 3 baseline/case resolver | Accept |
| D-66 | Readiness/frontiers là versioned derived projections với narrow relevant-input fingerprint; stale ready bị loại khỏi execution frontier, claim trước lease resolver là `not_evaluated` | Accept |
| D-67 | Hard-to-reverse Decision reference cần immutable acceptance proof bind contract revision/content và actor có `decision.accept`; existence hoặc shaping mention không đủ | Accept |
| D-68 | Trước initial Core v1, mỗi persisted/public contract family có một current baseline; Phase/Slice không phải version và internal development state không tạo predecessor/migration support | Accept |
| D-69 | Session bootstrap là versioned workflow bootstrap với assignment identity/authority; exact Ticket contract đến từ lease-bound Core query, không compile full context vào prompt | Accept |
| D-70 | Long-lived Rust Daemon, không phải hidden per-run supervisor, sở hữu Project/Workspace/Session/Provider/process/timeline runtime | Accept |
| D-71 | Linux, macOS và Windows là Tier-1 daemon process-owner targets; platform adapters prove process identity/tree cancellation và fail closed | Accept |
| D-72 | Workspace là opaque stable container; worktree chỉ là local isolation mode và workspace creation tách session creation | Accept |
| D-73 | Stable Pulse `session_id` tách provider-native handle; `closed` tách `archived`; cancellation chỉ commit sau provider acknowledgement/terminal event | Accept |
| D-74 | Runtime timeline dùng live delta cho immediacy và authoritative paged cursor fetch cho correctness; presence không phải delivery gate | Accept |
| D-75 | Tool catalog thuộc Daemon application layer; native provider tools, MCP, CLI và HTTP/WebSocket là adapters | Accept |
| D-76 | Core reservation và Daemon provisioning tạo explicit idempotent saga có compensation/recovery; không có distributed transaction giả | Accept |
| D-77 | Orchestrator dispatch một independent Worker per assignment; multiple Workers chỉ song song khi dependency/lease/workspace/write-scope cho phép, Reviewer/QA là peer sessions trên frozen snapshot với Core gate | Accept |

Khi một quyết định đổi, tạo Decision work item, cập nhật file chủ đề sở hữu và
root summary. Không sửa riêng bảng này. D-68 được ghi trong
[Decision 0003](../docs/decisions/0003-pre-release-contract-baselines.md);
D-69 trong [Decision 0004](../docs/decisions/0004-cli-mediated-agent-context.md);
D-70 tới D-76 trong
[Decision 0005](../docs/decisions/0005-rust-daemon-runtime-control-plane.md);
D-77 trong
[Decision 0006](../docs/decisions/0006-peer-agent-assurance-topology.md).

## Core v1 Definition of Done

Core v1 hoàn thành khi:

- [ ] `pulse init` bootstrap fixture repository mà không phá file user.
- [x] Work graph lưu/đọc/diff Epic, Story, Ticket, Decision bằng independent JSON node/edge files. Verified by Phase 1 Slices 1–2 and active graph/store integration tests.
- [ ] `docs/`, top-level `works/`, `.pulse/workgraph/`, evidence và runtime có folder/source hierarchy không mâu thuẫn. **Foundation complete:** domain layouts/bootstrap và target-repository fixture boundary đã có; safe top-level `pulse init` composition vẫn thuộc Phase 3.
- [x] Document registry query được ID/path/kind/owner/authority/scope/summary/aliases và không cần đăng ký mọi Markdown file. Verified by Slice 4 registry/applicability implementation and docs CLI tests.
- [x] Generated root/selected-area `_index.md` projections deterministic, marked generated và rebuildable. Verified by Slice 5 projection/index tests; broader declared-generated freshness enforcement remains a later docs-validation gate.
- [x] Markdown heading parser tạo section refs có document ID, heading path, line range và content hash. Verified by Slice 5 section extraction tests.
- [x] Pure-Rust lexical BM25+ index (tantivy) search được section-level, offline và không tải model. Verified by Slice 5 search/get/tree and retrieval-eval tests.
- [x] Lifecycle, deterministic edges, inverse projection, revision CAS, atomic recovery và supersession có unit/integration tests. Verified by Phase 1 graph/storage/process suites.
- [x] `graph export` rebuild deterministic sau khi xóa cache; SQLite không cần cho correctness/performance target v1. Verified by workgraph projection/cache tests.
- [ ] Agent nhận lease-bound committed `work packet` đầy đủ, gồm required/suggested section refs và read budget; bootstrap prompt chỉ mô tả retrieval/workflow/authority và không copy Ticket/docs/QA/knowledge content.
- [x] Node schema có normal CAS `revision` và semantic `contract_revision`; create/edit/draft flows ghi assessed values hoặc explicit `unassessed` domain value cho risk/materialization khi classification chưa đủ chắc, không fabricate defaults và không derive values từ older on-disk shapes. Verified by Slice 7 commit `677c593`.
- [x] Ticket role `implementation|decision_work` có typed contract riêng; precise decision work không bị recursive readiness loop. Verified by Slice 7 commit `677c593`.
- [x] Implementation Ticket ready gate kiểm tra objective/current/target, work surface/anchors, required changes, invariants, acceptance, mode, plan policy, verification/evidence/handoff contract. Verified by Slice 7 commit `677c593`.
- [x] Ready gate từ chối critical ambiguity chưa disposition; `delegated` phải nằm trong implementation freedom, `deferred` phải có owner/target + trigger hoặc linked work, và semantic shaping receipt phải source/revision-bound khi policy yêu cầu. Verified by Slice 7 commit `677c593`.
- [ ] `pulse-shape` đọc repo/docs trước khi hỏi, đi decision branches theo dependency order, hỏi human từng câu kèm recommendation khi có strong default, và materialize kết quả vào đúng Story/Ticket/Decision/docs owner theo risk.
- [ ] R0 clear/low-risk work qua short ambiguity self-check mà không bị ép tạo full brainstorm artifact hoặc hỏi human không cần thiết. **Foundation complete:** concise shaping mode/receipt contract đã có; Agent capability path vẫn thuộc Phase 2/3.
- [ ] R2/R3 multi-session shaping hỗ trợ approved destination/exit condition, canonical resolution pointers, derived decision frontier, bounded `not_yet_specified` và out-of-scope boundary. **Foundation complete:** typed destination/map/branch/fog/frontier contracts đã có; conversational multi-session workflow vẫn chưa implement.
- [ ] Precise fact/intent/trade-off/fidelity/prerequisite gaps được route đúng sang research, grilling, Decision, prototype hoặc enabling work; fog chưa precise không bị materialize sớm thành speculative Tickets. **Foundation complete:** typed gap/branch/fog vocabulary đã có; semantic routing capability vẫn chưa implement.
- [ ] Resolve decision work reconcile dependencies, graduate newly precise fog, supersede/cancel invalidated branches và recompute readiness với CAS/audit. **Foundation complete:** shaping apply/invalidate và readiness/frontier recomputation primitives đã có; full reconciliation mutation choreography vẫn chưa implement.
- [x] CLI phân biệt decision frontier với execution frontier và không persist claim state hoặc frontier list thành writable graph truth; trước lease resolver claim state là `not_evaluated`. Verified by Slice 7 commit `677c593`.
- [x] `.pulse/policy/authority.json` validate/fingerprint deterministic, default-deny, không có implicit human superuser và kernel derive grant từ operation/posture. Verified by Slice 7 commit `677c593`.
- [x] Hard-to-reverse Decision cần current immutable acceptance proof; Decision existence hoặc shaping approval không đủ. Verified by Slice 7 commit `677c593`.
- [x] QA impact `unknown` chặn ready; `none`/`covered_by_story_close` cần rationale/owner và grant tương ứng; `required` không pass giả trước baseline/case resolver. Verified by Slice 7 commit `677c593`; full required-case resolution remains Phase 3.
- [x] Readiness dùng narrow relevant-input fingerprint; status `ready` bị stale thì không vào execution frontier và read path không tự rewrite canonical node. Verified by Slice 7 commit `677c593`.
- [ ] Risk policy chọn materialization/verification gate đúng.
- [x] Evidence receipts immutable, source-bound và hash-validated. Verified for current Phase 1 receipt kinds by Slice 3/7 evidence and binding tests; run/handoff/verification/QA receipt kinds remain later phases.
- [x] Documentation impact hỗ trợ `required`, `none` + rationale và policy-governed `deferred`. Verified by Slice 4 docs-impact model/CLI/policy tests.
- [x] Documentation receipts source/content-bound; file thay đổi làm receipt cũ invalid. Verified by Slice 4/5 registry-aware receipt tests.
- [ ] Generated docs freshness có deterministic contract/check. **Foundation complete:** generated `_index.md` projection state/check đã có; broader declared-generated source/link/profile enforcement remains Phase 3.
- [ ] Promotion candidates từ work handoff được promote, classify non-durable hoặc defer có authority.
- [ ] `pulse doctor` phát hiện docs missing/stale/conflict/orphan/duplicate/generated/work-leak/context-gap và index/retrieval findings.
- [x] `pulse docs index|status|search|get|tree` có stable human/JSON contracts. Verified by Slice 5 CLI contract and retrieval integration tests.
- [x] `search` trả bounded snippets; `get` mặc định trả section; full document cần explicit opt-in. Verified by Slice 5 search/get/tree tests.
- [x] Docs-search cache content-hash keyed, atomic, disposable và incremental-rebuildable. Verified by Slice 5 cache/index concurrency, corruption and incremental rebuild tests.
- [ ] Retrieval eval đo Recall@K/MRR, exclusions, latency và context bytes trước useful section. **Partial:** Recall@K/MRR, exclusions và context-byte fixtures đã có; production latency threshold/reporting vẫn chưa hoàn tất.
- [ ] Story QA baseline có scope, acceptance/risk coverage, stable cases, applicability và exit criteria parse/validate được.
- [ ] Behavior-affecting Ticket khai báo QA impact; targeted checkpoint chọn đúng affected/new cases.
- [ ] Story QA baseline chạy được ít nhất qua một deterministic browser/Playwright executor và một structured non-browser API/CLI executor.
- [ ] QA environment start/healthcheck/fixture reset/cleanup và source-to-build identity tham gia receipt validity.
- [ ] Story không đóng nếu required behavioral receipt thiếu/invalid/fail/flaky/inconclusive hoặc coverage gap chưa disposition.
- [ ] Retry giữ failed attempts; waiver/non-applicability có rationale, authority và audit.
- [x] Knowledge store validate one-learning-per-record, revision CAS, typed applicability/provenance/promotion/freshness và relations. Verified by Slice 6 knowledge schema/store/relation/concurrency/recovery tests; compound and retrieval remain Phase 4.
- [ ] `pulse compound` synthesize/deduplicate/disposition candidates và cho phép `no_reusable_learning` trung thực.
- [ ] `pulse knowledge search|get|applicable|index|status` có stable human/JSON contracts, bounded output và explainable match/exclusion.
- [ ] Work packet inject role/moment-specific required/recommended learning summaries trong budget; candidate/stale/disputed không auto-inject.
- [ ] Learning contradiction với Decision/current docs tạo finding; learning-only không thỏa durable documentation promotion gate.
- [ ] Historical failure retrieval eval đo top-K, false applicability, exclusions và context bytes; cache rebuild deterministic.
- [ ] Usage feedback có thể reinforce/revise/retire và repeated failure sau injection được classify retrieval/apply/guardrail gap.
- [ ] `pulse doctor` tạo actionable findings có proposed work.
- [ ] Một failure thật được chuyển thành harness improvement + replay eval.
- [x] Offline vẫn query work, semantic history và evidence metadata. Verified by local graph/event/evidence CLI and store tests.
- [x] Docs progressive-disclosure đủ để Agent tìm đúng command/policy mà không nạp toàn bộ. Verified at registry/section search/get/tree layer; automatic work-packet routing remains Phase 2.
- [ ] Brownfield docs migration snapshot trước semantic restructure và không overwrite khi chưa approve.
- [ ] Các cross-phase acceptance scenarios thuộc Core hiện tại pass tự động
  hoặc có receipt hợp lệ; Runtime/QA/knowledge scenarios chỉ được claim ở
  milestone sở hữu.

Core **không cần daemon để query/mutate repository truth**. Data contracts phải
đủ để Runtime và Orchestration không phá work/evidence identity.

## Pulse Runtime Definition of Done

Runtime hoàn thành khi:

- [ ] Một executable `pulse` chạy Core commands offline và daemon commands qua
  local endpoint.
- [ ] Daemon có versioned handshake, request IDs, idempotency và capability
  negotiation.
- [ ] Project Registry dùng stable opaque `project_id`.
- [ ] Workspace Registry dùng stable opaque `workspace_id`; local/worktree
  isolation, archive và restore có ownership proof.
- [ ] Workspace creation tách session creation; một Workspace chứa nhiều
  sessions/terminals/services.
- [ ] Session Manager giữ stable `session_id`, provider handle, workspace,
  parentage, persistence handle và lifecycle.
- [ ] `initializing|idle|running|error|closed` và orthogonal archive semantics
  được persist/recover đúng.
- [ ] Cancel chỉ commit sau provider acknowledgement hoặc terminal event; failed
  interrupt không tạo false idle.
- [ ] Provider Registry có Codex-native implementation, capability/catalog
  contract và không spawn process từ metadata-only refresh.
- [ ] Daemon ProcessOwner sở hữu process từ spawn, bounded logs, timeout,
  identity, tree cancellation và managed-process ledger.
- [ ] Native Linux/macOS/Windows process identity/cancel/recovery suites pass.
- [ ] Runtime timeline có epochs/sequences, live WebSocket delivery và
  authoritative paged cursor catch-up.
- [ ] Web/Desktop/CLI/MCP dùng cùng daemon application/tool behavior.
- [ ] Core reservation + Daemon workspace/session provisioning là idempotent
  saga có compensation ở mọi failure boundary.
- [ ] Worker nhận lease-bound packet qua workflow bootstrap và gửi typed
  acknowledgement/handoff.
- [ ] Một Ticket standalone đi qua `ready -> reserved -> active -> verifying ->
  done|rework|blocked` bằng Core proof gate.
- [ ] Daemon restart recover registry, process, session, timeline và assignment
  saga không cần chat memory.
- [ ] Hidden per-run supervisor và duplicate Core-owned
  workspace/session/process runtime path đã bị xóa.
- [ ] Runtime acceptance scenarios trong `04-runtime-harness.md` pass.

## Orchestration v2 Definition of Done

Orchestration v2 hoàn thành khi:

- [ ] Orchestration Agent, Worker, Reviewer và QA là independent user-visible tasks/threads.
- [ ] Orchestration run có stable identity, control owner và recoverable state.
- [ ] Runtime ownership tree hỗ trợ parentage, detach, notify-on-finish và
  cascade archive mà không mutate work truth.
- [ ] Communication graph default-deny direct peer messaging ngoài typed policy.
- [ ] Specific-assignee assignment ngăn duplicate exclusive ownership.
- [ ] Typed mailbox phân biệt delivery, acknowledgement và fallback.
- [ ] Mỗi implementation Agent có workspace/source/docs-scope binding rõ.
- [ ] Worker handoff map acceptance sang proof và remaining risks.
- [ ] Orchestrator crash/restart recover từ local state, không từ chat memory.
- [ ] Human có thể observe/takeover mà không tạo hai control loops xung đột.
- [ ] Worker không vượt acceptance/approved-docs/close/merge/deploy authority.
- [ ] Reviewer/QA Agent kiểm tra frozen source snapshot độc lập.
- [ ] Multiple Workers chỉ dispatch song song khi dependency, exclusive lease, workspace và write-scope conflict policy cho phép.
- [ ] Reviewer, QA và documentation assurance có thể dispatch song song trên cùng frozen read-only snapshot; source đổi làm affected receipts stale.
- [ ] Reconciliation chạy theo cadence và ghi rationale/graph revision.
- [ ] Reviewer có thể review durable docs trên frozen source/content snapshot.
- [ ] Contract docs revision redirect và human takeover không tạo hai control loops xung đột.
- [ ] R2/R3 deliberation giữ independent proposals, evidence-based challenge,
  explicit disagreement và human authority khi assessors không hội tụ.
- [ ] Orchestration acceptance scenarios 66-75 pass (10 scenarios trong roadmap, tiếp sau 65 Core scenarios).

## Go/no-go trước mỗi phase

### Trước Phase 2

- Work graph mutations deterministic và recoverable.
- Node/edge schema, CLI projection và graph fingerprint ổn định.
- Documentation source hierarchy, registry identity, section identity và applicable/search/get query ổn định.
- Lexical retrieval fingerprint/rebuild semantics và generated `_index.md` contract ổn định.
- Event/evidence/documentation receipt identity không còn đổi schema tùy tiện.

### Trước Phase 3

- Single-Agent Core-Daemon assignment saga, cancel/resume, timeline recovery và
  close gate tin cậy.
- Hidden supervisor/direct provider launch cũ đã bị xóa sau replacement proof.
- Không còn fixed phase artifact dependency trong kernel.

### Trước Phase 4

- Failure evidence đủ để replay.
- Doctor/verification output machine-readable.

### Trước Phase 5

- Assignment lease, source binding, receipt validity và recovery đã pass trong single-agent.
- Human authority boundary đã được codify trong policy/kernel.
- Native Codex task transport được spike bằng integration test thật.

## Các câu hỏi cần prototype

1. Repository-scoped lock, atomic rename và crash recovery cần implementation nào trên các platform mục tiêu?
2. Ngưỡng node/edge nào khiến cache in-memory/file snapshot cần tối ưu, dựa trên benchmark thật?
3. Codex task/thread lifecycle API nào đủ ổn định để làm first-class transport?
4. Dirty worktree snapshot nên canonicalize và hash thế nào để receipt replay được?
5. Evidence retention/redaction mặc định bao nhiêu là hợp lý?
6. Story QA applicability/TTL/ancestor-snapshot policy cần mức linh hoạt nào trước khi thành ceremony?
7. Case/baseline revision compatibility nên invalid toàn bộ receipt hay cho phép impact-aware reuse ở mức nào?
8. Flaky threshold, retry budget và waiver expiry mặc định nào phù hợp theo risk profile?
9. Executor capability schema tối thiểu nào đủ cho Playwright/browser/API/CLI/data mà không thành adapter bureaucracy?
10. Document registry tối thiểu cần scope granularity nào để route tốt mà không thành metadata ceremony?
11. Documentation receipt TTL và ancestor-snapshot policy nào phù hợp cho authored/generated/external docs?
12. Tantivy có đáp ứng latency/memory/tokenization target trên fixture corpora mục tiêu không (MiniSearch JS chỉ còn là reference lesson)?
13. Section max size/overlap nào cho best Recall@K với context budget thấp?
14. Vietnamese/CJK, hyphenated identifier và dotted version cần tokenizer/alias fallback nào?
15. Lexical eval threshold nào đủ để defer semantic adapter, và gap nào trigger QMD/embedding spike?
16. Learning applicability schema tối thiểu cần dimension/version/symbol granularity nào để precision tốt mà không thành metadata ceremony?
17. Candidate-to-validated authority/reproduction thresholds nên khác thế nào cho success pattern, correction và blocking ratchet?
18. Knowledge prompt budgets và required-overflow policy nên scale thế nào theo risk/audience?
19. Usage feedback nào đủ tin cậy để reinforce/retire mà không dựa quá nhiều vào Agent self-report?
20. Docs và knowledge indexes nên share crate/interface/cache components ở mức nào mà không làm lẫn authority/result semantics?
21. macOS public libproc/kernel process start marker và boot-session marker nào
    đủ mạnh cho PID-reuse-safe cancellation trên supported OS versions?
22. Windows named Job Object/ACL/parent-lifetime design nào vừa giữ whole-tree
    ownership vừa recover an toàn sau daemon interruption?
23. Daemon local protocol/store nào đạt crash recovery và operability target
    mà không đẩy runtime concerns ngược vào Core?
24. Codex App Server session, interrupt, permission và timeline capabilities
    nào cần native extension thay vì normalize vào common provider contract?

Các câu hỏi này không làm thay đổi product thesis. Mỗi câu nên có timeboxed spike Ticket và Decision receipt.

## Dấu hiệu đang đi sai hướng

- Số phase/artifact tăng nhưng correct completion không tăng.
- Agent phải đọc một file hướng dẫn khổng lồ trước mọi việc.
- “Done” vẫn phụ thuộc câu báo cáo thay vì receipt/gate.
- Orchestrator chỉ là một script sort priority rồi spawn hàng loạt.
- User không mở hoặc takeover được Worker task.
- Hai hệ thống cùng sửa status mà không có field ownership.
- Hidden supervisor và daemon cùng tồn tại như hai launch/lifecycle authorities.
- Runtime Session/Workspace/Timeline state bị ghi ngược vào canonical work graph.
- Harness backlog bị giấu khỏi product work graph.
- Durable knowledge tiếp tục mắc kẹt trong closed work artifacts.
- Document registry bắt mọi Markdown mang metadata dù không cần routing/authority.
- `_index.md` trở thành file khổng lồ hoặc writable truth thứ hai.
- Search mặc định trả full documents thay vì section refs/snippets có budget.
- Semantic/model dependencies vào Core trước khi lexical eval chứng minh cần.
- Agent vẫn phải grep toàn bộ docs tree hoặc đọc `AGENTS.md` khổng lồ.
- Multi-agent demo đẹp nhưng restart là mất ownership/context.

Gặp một trong các dấu hiệu này phải dừng feature work liên quan, tạo Decision/failure classification và sửa harness trước khi scale.
