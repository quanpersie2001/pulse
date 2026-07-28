# Pulse Reboot

> Trạng thái: bản thiết kế để thảo luận, chưa phải compatibility contract.
> Cập nhật: 2026-07-18.
> Đọc tiếp: [`pulse-reboot/README.md`](pulse-reboot/README.md).

## Pulse sẽ là gì?

Pulse là một **local-first harness engineering system** giúp một repository trở nên dễ hiểu, dễ sửa và dễ kiểm chứng đối với coding agent.

Pulse kết hợp sáu thứ:

1. **Local work graph** lưu Epic, Story, Ticket, Decision và quan hệ giữa chúng.
2. **Documentation knowledge system** giữ durable repository knowledge có owner, applicability, authority và proof.
3. **Repository harness** gồm skills, scripts, tools, hooks, policy và evals.
4. **Evidence loop** biến mỗi lần chạy, verify, review và QA thành bằng chứng có thể kiểm tra lại.
5. **Knowledge compounding loop** biến success/failure có evidence thành learning có schema, promote guardrail/docs/eval và retrieve đúng lúc cho future work.
6. **Peer-agent orchestration** cho phép một Orchestration Agent điều phối các Agent độc lập qua task/thread riêng khi single-agent đã đủ tin cậy.

Pulse không phải Jira thu nhỏ, không phải một fixed phase workflow và không phải một general-purpose agent framework.

## Product thesis

> Pulse biến repository thành một môi trường mà agent có thể chọn đúng việc, hiểu đủ ngữ cảnh, dùng đúng capability, tạo bằng chứng đáng tin và cải thiện chính harness sau mỗi failure.

North star không phải là số bước tự động hóa. North star là:

> **Agent hoàn thành thay đổi đúng với ít can thiệp của con người hơn, trong khi mọi quyết định và bằng chứng quan trọng vẫn local, inspectable và recoverable.**

## Kiến trúc một trang

```text
Human
  |
  v
Local Work Graph <---- decisions / priorities / supersession
  |
  +----> Documentation Knowledge System ----> docs / AGENTS.md / PULSE.md
  |              |                                  |
  |              + ownership / applicability        + durable repository truth
  v
Pulse Kernel -----> Repository Harness -----> Source Repository
  |                       |                         |
  |                       + skills/scripts/tools   + tests/app
  |                       + hooks/policy/evals     + docs/config
  v
Run Events + Evidence -----> Verify / Review / QA -----> Close or Requeue
          |                              |
          +----> Compound Learnings <----+
                       |
                       +----> Docs / Decisions / Skills / Checks / Evals
                       +----> Applicable recall for future work packets

Optional, sau khi core ổn định:
Human -> Orchestration Agent -> independent Agent tasks/threads
                    |                    |
                    + leases/mailbox     + isolated worktrees
                    + reconcile          + handoff receipts
```

## Các quyết định nền tảng

- Repository là system of record; cloud service chỉ là adapter tùy chọn.
- Canonical work graph dùng sharded `nodes/*.json` + `edges/*.json`; human-facing work prose nằm ở top-level `works/`, không dùng một tracked JSON monolith hay SQLite.
- Durable repository knowledge nằm ở `docs/`, `AGENTS.md` và `PULSE.md`; work prose, evidence và runtime state không được giả làm current docs truth.
- CLI/kernel là query và mutation surface bắt buộc. Agent nhận execution packet gồm applicable docs; `pulse docs search/get` định tuyến section-level context, không tự search raw graph files hoặc đọc toàn bộ docs tree.
- Ticket là đơn vị executable. Epic và Story giữ outcome, design, approach và behavioral baseline dài hạn.
- Hierarchy không thay thế dependency graph; priority là tín hiệu, không phải phép sort tuyệt đối.
- Artifact được materialize theo risk, không ép mọi Ticket đi qua cùng một bộ ceremony.
- Critical ambiguity phải được resolve trước execution bằng shaping repo-grounded, one-question-at-a-time và risk-adaptive; đây là readiness discipline, không phải fixed brainstorm phase.
- Readiness/frontiers là derived projections trên typed contracts, receipts, docs, Decisions và policy; semantic freshness dùng `contract_revision` riêng để lifecycle/pointer mutation không tự làm proof stale.
- Shaping effort lớn khóa destination, quản lý decision frontier và bounded `not_yet_specified`; graph được mở rộng theo evidence sau mỗi resolution thay vì speculative decomposition upfront.
- Deterministic work thuộc kernel/script; judgment work thuộc agent skill.
- Developer verification chứng minh implementation Ticket; impact-driven QA checkpoint kiểm tra affected behavior sớm; full Story qualification chứng minh integrated capability vẫn đúng qua nhiều Ticket.
- Mỗi implementation Ticket có documentation impact posture; public behavior, invariant, architecture hoặc operator procedure thay đổi phải cập nhật, classify hoặc defer durable docs qua gate.
- Learning record giữ reusable guidance, applicability và provenance; nó không thay current docs/Decision. Compound capture liên tục, synthesize sau cycle, promote selectively và retrieve bằng typed applicability trước lexical ranking.
- Documentation validation dùng source/content-bound receipts; accepted Decision và product contract diễn tả intent, code divergence có thể là defect chứ không tự động biến docs thành stale.
- Core docs retrieval dùng generated `_index.md` + disposable section-level BM25 cache; semantic/hybrid retrieval là optional adapter sau khi lexical eval chứng minh cần.
- Story sở hữu persistent QA baseline; Ticket reference affected cases. QA dùng typed receipts và resolve executor theo surface/capability/environment, gồm Playwright, browser agent/DevTools observation, API, CLI/PTY, data hoặc structured manual adapter.
- Work graph là nguồn sự thật của trạng thái công việc; message chỉ là transport và coordination evidence.
- Worker Agent là task/thread độc lập, có danh tính, lease và worktree riêng; không đồng nghĩa với sub-agent.
- Orchestration Agent hành xử như user ở lớp transport nhưng chỉ có bounded authority ở lớp nghiệp vụ; `PULSE.md` giữ human intent còn enforceable local grants nằm trong default-deny `.pulse/policy/authority.json`.
- QA impact `unknown` không được ready; `none` và Story-close deferral cần explicit local grants, còn behavior work `required` chờ baseline/case resolver thay vì được bypass.
- Worker không tự đổi acceptance, đóng Story, merge hoặc deploy nếu chưa được cấp quyền.
- Ưu tiên single-agent reliability trước, sau đó mới thêm concurrency và peer-agent orchestration.

## Bản đồ đọc

| Khi cần hiểu | Tài liệu sở hữu chi tiết |
| --- | --- |
| Vì sao reboot, học gì từ OpenAI và các reference | [`01-foundations.md`](pulse-reboot/01-foundations.md) |
| Epic, Story, Ticket, Decision được lưu và chạy thế nào | [`02-work-graph.md`](pulse-reboot/02-work-graph.md) |
| QA, Playwright/browser agent và bằng chứng đóng Story | [`03-story-qa.md`](pulse-reboot/03-story-qa.md) |
| Kernel, skills, scripts, tools, hooks, CLI và events | [`04-runtime-harness.md`](pulse-reboot/04-runtime-harness.md) |
| Orchestrator điều phối các Agent độc lập thế nào | [`05-cross-agent-coordination.md`](pulse-reboot/05-cross-agent-coordination.md) |
| Priority, dependency, foundation work và supersession | [`06-priority-reconciliation.md`](pulse-reboot/06-priority-reconciliation.md) |
| Verify, review, doctor, eval và ratchet loop | [`07-verification-ratchet.md`](pulse-reboot/07-verification-ratchet.md) |
| Công nghệ, migration, phases, scenarios và risks | [`08-implementation-roadmap.md`](pulse-reboot/08-implementation-roadmap.md) |
| Quyết định cần khóa và Definition of Done | [`09-decisions-and-dod.md`](pulse-reboot/09-decisions-and-dod.md) |
| Durable docs, ownership, context routing, validation, promotion và drift | [`10-documentation-system.md`](pulse-reboot/10-documentation-system.md) |
| Docs index, section search/get, BM25 cache và optional semantic retrieval | [`11-documentation-retrieval.md`](pulse-reboot/11-documentation-retrieval.md) |
| Learning schema, compounding, promotion, applicable recall và prompt injection | [`12-knowledge-compounding.md`](pulse-reboot/12-knowledge-compounding.md) |

## Ranh giới phiên bản

**Pulse Core v1** phải hoàn thiện local work graph, documentation context/impact, single-agent run, evidence, verification, knowledge compounding/applicable recall và harness ratchet trước.

**Pulse Orchestration v2** bổ sung independent Agent Registry, thread transport, assignment lease, typed mailbox, worktree isolation và reconciliation loop. Thiết kế v2 phải có từ đầu để v1 không khóa sai data model, nhưng không được làm chậm việc chứng minh core.

## Bước tiếp theo

Phase 1 local graph, documentation, evidence, knowledge-store và
shaping/readiness foundations đã được implement qua Slice 1–7. Frontier hiện tại
là **Phase 2 single-agent run**:

1. assignment lease, workspace binding và `PreparedAssignmentV1` đã được
   implement tại
   [`proposals/phase2-slice2-atomic-reservation-workspace-binding.md`](proposals/phase2-slice2-atomic-reservation-workspace-binding.md),
   trên nền preview `WorkPacketV1` đã verified tại
   [`proposals/phase2-slice1-work-packet-dispatch-foundation.md`](proposals/phase2-slice1-work-packet-dispatch-foundation.md), mở gated
   `ready -> active`;
2. thêm runner, cancel/resume và source/workspace recovery;
3. thêm typed handoff, verification và proof-driven close gate để một Ticket đi
   tới `done|rework|blocked`.

Không bắt đầu full Phase 3 QA, Phase 4 compounding hoặc Phase 5 orchestration
trước khi Phase 2 exit proof pass. Chi tiết sequencing và acceptance thuộc
[`08-implementation-roadmap.md`](pulse-reboot/08-implementation-roadmap.md);
Definition of Done hiện hành thuộc
[`09-decisions-and-dod.md`](pulse-reboot/09-decisions-and-dod.md). Mọi thay đổi
lớn phải cập nhật đúng tài liệu sở hữu thay vì làm trang L0 này phình trở lại.
