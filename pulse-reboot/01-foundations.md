# Foundations

[Trang vào](../PULSE_REBOOT.md) | [Bản đồ tài liệu](README.md)

**Đọc khi:** cần hiểu vì sao Pulse reboot và phần nào được học từ OpenAI, `harness-experimental`, Maestro, Symphony.
**Sở hữu:** product thesis, goals, non-goals và design principles.

## Vấn đề cần giải

Ý tưởng ban đầu của Pulse đúng: agent cần nhiều hơn một prompt. Nó cần repository context, executable tools, verification, durable work state và feedback loop.

Kiến trúc cũ lại dễ tối ưu cho việc duy trì workflow của chính Pulse: phase cố định, nhiều adapter và artifact bắt buộc dù risk thấp. Khi một bước không tăng capability, observability, safety hoặc recoverability, bước đó là ceremony.

Reboot không có nghĩa vứt bỏ toàn bộ. Reboot nghĩa là đổi trục:

```text
workflow-first  -> capability-first
phase artifacts -> risk-adaptive evidence
agent memory    -> repository-legible context
task list       -> local semantic work graph
success claims  -> executable proof
```

## Bài học từ OpenAI Harness Engineering

Nguồn chính: [Harness engineering: leveraging Codex in an agent-first world](https://openai.com/index/harness-engineering/).

### Repository là môi trường thực thi của agent

Agent chỉ làm tốt khi codebase dễ đọc bằng máy: entrypoint rõ, boundaries rõ, command deterministic, invariant được enforce và failure dễ truy vết. Harness không nằm bên ngoài repo; nó là một phần của product engineering.

### `AGENTS.md` là bản đồ

Root guidance nên ngắn: chỉ ra nơi tìm architecture, tests, domain rules và commands. Kiến thức sâu phải nằm cạnh code hoặc trong docs có ownership rõ. Nhồi mọi thứ vào một instruction file làm giảm khả năng tìm đúng context.

### Guardrail cơ học, judgment để agent xử lý

Formatting, schema validation, forbidden dependency, generated-file freshness và test command nên là script/hook. Trade-off kiến trúc, ticket shaping và semantic priority nên nằm trong agent skill có bằng chứng.

### Làm rõ critical ambiguity trước execution

Một work item không nên trở thành executable khi Agent vẫn phải đoán objective, acceptance, scope, invariant hoặc một lựa chọn khó đảo ngược. Shaping phải pressure-test các nhánh quyết định còn mở theo dependency order: đọc repository và durable docs trước, chỉ hỏi human về intent, preference, authority hoặc trade-off mà evidence không tự trả lời được, hỏi từng câu một và đưa recommended answer khi có strong default.

Yêu cầu này là một **ambiguity gate**, không phải một phase brainstorm bắt buộc. Việc nhỏ, rõ và risk thấp có thể chỉ cần concise contract; work mơ hồ hoặc risk cao cần interview sâu hơn và materialize Story approach, Decision, Discovery/Spike Ticket hoặc artifact tương ứng. Mỗi nhánh planning-critical phải được quyết định, loại bỏ, giao rõ cho implementation freedom, defer có owner/trigger hoặc đánh dấu blocking trước execution.

### Failure phải cải thiện harness

Một lỗi lặp lại không chỉ tạo bugfix. Nó phải được phân loại: thiếu context, thiếu tool, guardrail yếu, verification thiếu, policy mơ hồ hay task shape kém. Kết quả là một thay đổi cụ thể trong harness và một eval chống tái phát.

### Không copy kết quả benchmark mù quáng

Các thực hành của OpenAI được benchmark trên repo và operating model của họ. Pulse học nguyên lý và xây eval trên repository mục tiêu; không coi mọi chi tiết trong blog là universal law.

## Bài học từ `harness-experimental`

- Story packet giữ outcome, context, criteria và proof gần nhau.
- Work item cần dependency và evidence, không chỉ status.
- Critique/review phải có cấu trúc để phát hiện thiếu sót trước khi close.
- Một hierarchy đẹp không đủ; scheduling cần graph và semantic reconciliation.
- Artifact có giá trị khi giúp resume, review hoặc verify. Artifact sinh ra chỉ vì phase là ceremony.

Pulse giữ tinh thần packet nhưng tách rõ Epic, Story và executable Ticket. Mức materialization phụ thuộc risk; xem [`02-work-graph.md`](02-work-graph.md).

## Bài học từ Matt Pocock skills

`references/mattpocock/skills` cung cấp hai lớp primitive đáng giữ.

Từ `grilling`, `grill-me` và `grill-with-docs`, Pulse học cách pressure-test plan/design bằng decision tree: hỏi một câu mỗi lượt, giải quyết parent decision trước child branches, đọc codebase thay vì hỏi fact có thể tự tìm và kèm recommended answer để human phản hồi trên một proposal cụ thể. `grill-me` dùng tree như reasoning model nhưng không persist nó; `grill-with-docs` giữ glossary và ADR chọn lọc, không lưu toàn bộ branch graph.

Từ `wayfinder`, Pulse học cách vận hành khi decision space lớn hơn một session và chưa thể nhìn thấy đầy đủ:

- khóa **destination** và shaping exit condition trước khi map work;
- materialize câu hỏi đã đủ sắc nét thành decision work có dependency;
- giữ **decision frontier** là tập câu hỏi có thể xử lý ngay;
- giữ **not yet specified** cho fog-of-war chưa thể phát biểu thành actionable question, thay vì tạo speculative Tickets;
- resolve một decision rồi reconcile map, graduate fog vừa rõ, supersede nhánh bị invalid và recompute execution readiness;
- dùng map như index trỏ tới canonical resolutions, không copy cùng một quyết định vào nhiều artifact.

Pulse không copy nguyên skill chain `grill-with-docs -> to-spec -> to-tickets`, issue-tracker storage hoặc mặc định ghi `CONTEXT.md`/ADR trong mọi session. Pulse tích hợp grilling và progressive wayfinding vào `pulse-shape`; local work graph vẫn canonical, external tracker chỉ là adapter. Độ sâu interview/map và artifact theo risk, nhưng critical ambiguity không được âm thầm chuyển sang Worker.

## Bài học từ Maestro

Maestro cung cấp nhiều primitive đáng giữ:

- Card/Task làm đơn vị công việc local.
- Binding giữa work item, session và worktree.
- Claim, presence, messaging, related edge và conflict advisory.
- QA facet và các loop recipe có ownership tương đối rõ.
- Conductor giữ một số gate thay vì để worker tự kết luận mọi thứ.

Pulse học typed Card envelope, typed relations, sidecar prose và CLI graph projection của Maestro. Pulse không copy DB-backed Card store hiện tại: tracked SQLite có diff/merge kém và đi ngược mục tiêu Git-native của Pulse.

Nhưng Maestro hiện là coordination substrate nhiều hơn là lifecycle orchestration hoàn chỉnh. Nó chưa cung cấp trọn vòng create independent agent, assign, wake, wait, collect, reconcile và retire. Pulse không nên copy UI hoặc recipe nguyên trạng; cần chuẩn hóa primitive thành contracts trong [`05-cross-agent-coordination.md`](05-cross-agent-coordination.md).

## Symphony nằm ở đâu

Nguồn chính: [OpenAI công bố Symphony](https://openai.com/index/open-source-codex-orchestration-symphony/) và [repository `openai/symphony`](https://github.com/openai/symphony).

Symphony minh họa một orchestration service đọc tracker, normalize issue, chọn work, tạo isolated workspace và chạy coding agents theo vòng lặp. Reference implementation để Linear giữ Ticket/relations; scheduler chỉ giữ running/claimed/retry state trong memory và recover bằng tracker + filesystem thay vì persistent database. Giá trị chính cho Pulse là ranh giới:

- **Pulse Core** làm repository harness và local JSON work graph.
- **Pulse CLI** đóng vai tracker query/normalization layer mà Linear adapter cung cấp cho Symphony.
- **Pulse Orchestration** dùng normalized packet từ CLI để điều phối Agent độc lập.
- External tracker chỉ là adapter, không phải canonical truth mặc định.

Symphony là reference cho orchestration shape, không phải dependency bắt buộc của Pulse.

## Paseo nằm ở đâu

Nguồn chính trong repository:
[`references/paseo/docs/architecture.md`](../references/paseo/docs/architecture.md),
[`references/paseo/public-docs/workspaces.md`](../references/paseo/public-docs/workspaces.md),
[`references/paseo/docs/agent-lifecycle.md`](../references/paseo/docs/agent-lifecycle.md)
và
[`references/paseo/docs/timeline-sync.md`](../references/paseo/docs/timeline-sync.md).

Paseo chứng minh một local-first daemon có thể làm runtime authority cho nhiều
clients và nhiều coding-agent providers. Các pattern Pulse áp dụng:

- daemon quản lý Project, Workspace, Session và Provider lifecycle;
- Workspace là stable container, worktree chỉ là isolation mode;
- một Workspace chứa nhiều Agent sessions, terminals và services;
- stable product session identity tách provider-native persistence handle;
- native provider adapters và ACP generic cùng nằm sau capability contract;
- transport-neutral tool catalog được expose qua native tools hoặc MCP;
- live WebSocket stream phục vụ immediacy, authoritative timeline fetch/cursor
  phục vụ correctness;
- cancellation chỉ commit sau provider acknowledgement/terminal event;
- runtime process/helper ownership có ledger và startup reconciliation.

Pulse không copy Paseo như product hoặc code dependency. Paseo hiện dùng Node.js
daemon; Pulse implement cùng architectural shape bằng Rust để giữ một toolchain,
chia sẻ Core contracts và phát hành một executable. Pulse cũng không dùng
Paseo AgentManager làm work truth: daemon chỉ sở hữu runtime, còn repository
Core sở hữu Ticket, contracts, evidence và gates.

Một số chi tiết không được copy:

- không dùng `cwd` hoặc provider thread ID làm Pulse workspace/session identity;
- không để runtime parentage truyền business authority;
- không để client replica, timeline hoặc daemon registry thay canonical graph;
- không thêm relay/mobile/browser breadth trước single-Agent vertical slice.

## Product thesis

> Pulse là local-first harness engineering system giúp coding agent chọn đúng việc, lấy đúng context, dùng đúng capability, tạo proof đáng tin và làm repository dễ vận hành hơn sau mỗi run.

## Goals

- Bootstrap một repository thành agent-legible environment.
- Quản lý durable repository knowledge có owner, applicability, authority và freshness.
- Quản lý công việc lớn và executable work bằng local work graph.
- Pressure-test critical ambiguity trước execution bằng shaping repo-grounded và risk-adaptive.
- Chạy một Ticket bằng agent với bounded source/docs context và policy có giới hạn.
- Quản lý Project, Workspace, Session và Provider qua một local Rust daemon có
  timeline/recovery rõ ràng.
- Thu thập typed evidence từ code, test, browser, API và review.
- Reconcile priority theo dependency, foundation value và supersession.
- Chuyển failure lặp lại thành harness improvement và eval.
- Sau v1, điều phối các Agent độc lập mà human vẫn quan sát và takeover được.

## Non-goals

- Thay thế toàn bộ project management suite.
- Ép Scrum taxonomy hoặc mọi Ticket phải thuộc Story/Epic.
- Tạo một universal workflow engine.
- Che giấu agent runtime sau abstraction quá sớm.
- Duy trì cả hidden per-run supervisor và daemon như hai runtime authorities.
- Tối đa concurrency trước khi single-agent flow tin cậy.
- Cho Orchestration Agent toàn bộ quyền của human chỉ vì nó gửi prompt như user.
- Dùng conversation history làm durable source of truth.

## Design principles

1. **Capability before ceremony.** Mỗi thành phần phải tăng khả năng, độ tin cậy hoặc khả năng phục hồi.
2. **Progressive disclosure.** Root là map; chi tiết và applicable docs chỉ được nạp khi cần.
3. **One writable source per truth.** Durable docs, work state, runtime state và evidence không tranh quyền ghi.
4. **Local-first and inspectable.** Offline vẫn đọc, sửa, diff, audit và recover được.
5. **Deterministic mechanism, agent judgment.** Kernel không giả làm semantic planner.
6. **Evidence over assertion.** `done` là kết quả của gate, không phải một câu báo cáo.
7. **Risk-adaptive materialization.** Risk cao cần artifact và proof sâu hơn.
8. **Human authority remains explicit.** Automation có quyền theo capability, không theo vai trò mơ hồ.
9. **Durable knowledge over work leakage.** Invariant/product/operations knowledge không được mắc kẹt trong closed work artifacts.
10. **Reliability before concurrency.** Chỉ scale sau khi recovery và verification đủ mạnh.
11. **Failure feeds the ratchet.** Mỗi failure class phải có đường nâng cấp harness.
12. **Learning must be retrievable, not merely stored.** Reusable insight cần provenance, typed applicability, lifecycle và bounded recall; current truth vẫn thuộc docs/Decision, stable prevention thuộc checks/evals/policy.
13. **Resolve critical ambiguity before execution.** Đọc evidence trước khi hỏi, đi qua decision branches theo dependency order và chỉ dispatch khi mỗi nhánh quan trọng đã resolved, delegated, deferred có kiểm soát hoặc blocking; độ sâu và artifact phải theo risk.
14. **Map the frontier, not the fog.** Khóa destination, materialize chỉ những câu hỏi đã đủ sắc nét, giữ uncertainty chưa thể phát biểu trong `not_yet_specified`, và mở rộng decision graph dần sau mỗi resolution thay vì giả vờ biết toàn bộ upfront.
