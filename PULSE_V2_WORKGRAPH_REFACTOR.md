# Pulse v2 Workgraph Refactor Proposal

## Mục tiêu

Refactor Pulse theo hướng tự chủ hơn, giảm phụ thuộc runtime ngoài, và làm rõ ranh giới giữa 3 mặt phẳng:

1. **Application/product repo truth** — các tài liệu bền vững về app/product, dành cho con người và agent cùng đọc.
2. **Application/product work surface** — nơi mô tả các epic/story/task/bug của app/product đang hoặc sẽ được thực hiện.
3. **Runtime / machine / harness control plane** — trạng thái chạy, handoff, memory, harness docs/backlog, và graph metadata.

Mục tiêu chính của Pulse v2:

- bỏ phụ thuộc `br` (`beads_rust`) và `bv` (`beads_viewer`)
- thay bằng một **minimal built-in workgraph CLI** do Pulse quản lý
- giảm file explosion trong workflow hiện tại
- tách bạch `docs/`, `works/`, và `.pulse/`
- bỏ tư duy `history/` như source of truth cho work-in-progress
- giữ lại những ý tưởng tốt từ beads/Flywheel: dependency graph, ready work, task ownership, verification linkage

---

## Kết luận chốt

### 1. Bỏ `br` / `bv`

Pulse v2 sẽ **không còn phụ thuộc** vào:

- `br`
- `bv`

Thay vào đó, Pulse sẽ có một **minimal workgraph system** riêng.

### 2. Naming convention được chốt

Dù mental model gần với Jira hơn beads, Pulse v2 sẽ **không dùng `jira`** làm tên nội bộ để tránh kéo sai kỳ vọng sản phẩm.

Naming được chốt theo 4 lớp:

- **generic noun trong docs/UI**: `work item`
- **internal short noun trong schema/code**: `item`
- **storage/system name**: `workgraph`
- **CLI name**: `pulse-work`

Tức là:

- `EPIC`, `STORY`, `TASK`, `BUG` đều là **work items**
- các item được lưu trong `.pulse/workgraph/`
- người dùng thao tác với graph qua CLI `pulse-work`

### Không dùng các tên sau làm generic noun

- `jira`
- `bead`
- `pulse-item`
- `pulse-work`

Lý do: `pulse-work` hợp làm tên CLI hơn là tên object; còn `jira`/`bead` kéo theo mental model hoặc legacy không còn phù hợp.

### 3. Chia repo thành 3 plane chính

- `docs/` = durable application/product repo truth
- `works/` = canonical application/product work/content surface
- `.pulse/` = runtime + harness + machine memory + workgraph metadata

### 4. Không giữ top-level `history/` hoặc `runs/` như source of truth

Audit trail không cần một folder `runs/` riêng. Execution record đi theo task/bug qua `verification.md`; story-level closeout đi qua `lifecycle-summary.md` khi cần tổng kết durable.

### 5. Một canonical metadata source

Work graph metadata sẽ có **một source of truth duy nhất**:

- `.pulse/workgraph/items.jsonl`

Các view như `active`, `closed`, `ready`, `graph` chỉ là **generated views**, không phải canonical writable sources.

---

## Folder structure được chốt

> Ghi chú: dùng ASCII tree làm cấu trúc chuẩn để dễ đọc. Các comment ở cạnh folder/file mô tả trách nhiệm của chúng.

```text
project/
|-- AGENTS.md
|   # Agent operating contract ở mức repo.
|
|-- README.md
|   # Entry document cho con người; giải thích repo là gì và dùng ra sao.
|
|-- docs/
|   # Application/product repo truth bền vững. Không chứa harness/runtime state hoặc tactical run logs.
|   |
|   |-- ARCHITECTURE.md
|   |   # Kiến trúc hiện hành của application/product, boundaries, layering, constraints dài hạn.
|   |
|   |-- decisions/
|   |   # Durable rationale cho các quyết định quan trọng.
|   |   |
|   |   `-- 0001-...
|   |
|   |-- product/
|   |   # Product contract hiện hành của repo/app.
|   |   |
|   |   |-- overview.md
|   |   `-- ...
|   |
|   |-- GLOSSARY.md
|   |   # Default repo glossary để cố định thuật ngữ application/domain dễ drift hoặc bị overload.
|   |
|
|-- works/
|   # Human-facing application/product work surface. Đây là nơi đọc/ghi nội dung công việc app/product.
|   |
|   |-- test-matrix.md
|   |   # Default repo-level tactical verification matrix có status/evidence theo work hiện hành.
|   |
|   |
|   |-- backlog.md
|   |   # Application/product backlog entry đơn giản cho work chưa slice sâu.
|   |
|   `-- epics/
|       |
|       `-- E-25F-H2X9-authentication/
|           # Một capability stream / domain lớn.
|           |
|           |-- README.md
|           |   # Epic overview: mục tiêu, boundary, stories liên quan.
|           |
|           `-- S-25F-K3W8-oauth-login/
|               # Một story / delivery slice cụ thể.
|               |
|               |-- README.md
|               |   # Canonical entry file cho story.
|               |
|               |-- approach.md
|               |   # Optional. Chỉ tạo khi reasoning / design cần tách riêng.
|               |
|               |-- execplan.md
|               |   # Optional. Chỉ tạo khi sequencing/phases đủ phức tạp.
|               |
|               |-- validation.md
|               |   # Optional. Chỉ tạo khi proof contract/risk cần file riêng.
|               |
|               |-- references/
|               |   # Nơi chứa research/input docs gắn riêng với story này.
|               |   |
|               |   |-- research.md
|               |   `-- design-system-research.md
|               |
|               |-- lifecycle-summary.md
|               |   # Optional closeout/audit summary cho story sau review hoặc khi cần resume onboarding lâu dài.
|               |
|               `-- tasks/
|                   # Nội dung human-facing cho TASK/BUG thuộc story.
|                   |
|                   |-- T-25F-7K9M-session-store/
|                   |   |
|                   |   |-- README.md
|                   |   |   # Canonical entry file cho task.
|                   |   |
|                   |   `-- verification.md
|                   |       # Narrative proof / commands / attempts / gaps cho task.
|                   |
|                   `-- B-25F-M8R2-mobile-oauth-bounce/
|                       |
|                       |-- README.md
|                       `-- verification.md
|
`-- .pulse/
    # Internal machine/runtime plane. Không phải nơi viết product truth.
    |
    |-- project-docs.json
    |   # Mapping cho application/product docs nếu cần scout / bootstrap.
    |
    |-- harness/
    |   # Durable harness/agent operating surface. Không phải application docs/work.
    |   |
    |   |-- HARNESS.md
    |   |   # Operating harness contract: app là product surface, harness là agent/workflow surface.
    |   |
    |   `-- backlog.md
    |       # Harness improvement proposals phát hiện từ process/tooling friction.
    |
    |-- workgraph/
    |   # Canonical machine work graph.
    |   |
    |   |-- items.jsonl
    |   |   # Source of truth duy nhất cho graph metadata.
    |   |
    |   |-- schema.json
    |   |   # Required v1 schema để validate records trong items.jsonl.
    |   |
    |   `-- views/
    |       # Generated materialized views. Không phải canonical writable sources.
    |       |
    |       |-- active.json
    |       |-- closed.json
    |       |-- ready.json
    |       `-- graph.json
    |
    |-- runtime/
    |   # Live control plane của session/workflow.
    |   |
    |   |-- tooling-status.json
    |   |   # Preflight / readiness result.
    |   |
    |   |-- state.json
    |   |   # Machine-readable session/workflow state.
    |   |
    |   |-- STATE.md
    |   |   # Human-readable runtime snapshot.
    |   |
    |   |-- handoffs/
    |   |   # Pause/resume contracts.
    |   |
    |   |-- checkpoints/
    |   |   # Advisory snapshots / recoverability helpers.
    |   |
    |   `-- reservations.json
    |       # Shared reservation state nếu cần file coordination.
    |
    |-- migrations/
    |   # Brownfield-safe migration artifacts. Không phải live docs.
    |   |
    |   `-- docs-backups/
    |       # Snapshot docs cũ trước khi onboard/restructure bằng Pulse.
    |       |
    |       `-- 2026-05-14-pre-pulse-onboard/
    |           # Timestamped snapshot; never treat as current truth.
    |
    |-- memory/
    |   # Cross-feature durable machine memory.
    |   |
    |   |-- critical-patterns.md
    |   |-- learnings/
    |   |-- corrections/
    |   `-- ratchet/
    |
    `-- scripts/
        # Repo-local helpers: onboarding, pulse-work, migrations, utilities.
```

> Deferred after v1: `.pulse/cache/` cho generated cache/indexes; không phải source of truth.

---

## Trách nhiệm của từng khu vực

## `docs/`

Chỉ chứa application/product docs có tuổi thọ dài hơn từng request/work item.

### Nên giữ

- `docs/ARCHITECTURE.md`
- `docs/product/*`
- `docs/decisions/*`
- `docs/GLOSSARY.md`

### Optional nhưng hợp lý

- `docs/TESTING.md` hoặc tài liệu tương đương nếu cần ghi lại testing policy / coverage philosophy ở mức durable

### Không nên có

- `docs/backups/` như một default permanent part của live docs surface
- harness operating docs/backlogs như `docs/HARNESS.md` hoặc `docs/HARNESS_BACKLOG.md`
- các tactical story packets đang active
- các validation snapshots theo run
- test matrices có `Status`, `Evidence`, hoặc progress state thay đổi theo execution
- runtime logs / handoff / state mirrors

### Lý do

`docs/` phải là **application/product repo truth surface**, không phải:

- backup area mặc định
- execution log area
- runtime state area

### Brownfield note

Với **brownfield repos**, backup docs cũ trước khi Pulse onboard hoặc restructure là **được khuyến nghị mạnh**. Tuy nhiên, backup đó nên nằm ở:

- `.pulse/migrations/docs-backups/...`

thay vì sống lâu dài trong `docs/` canonical.

---

## `works/`

Là **canonical human-facing application/product work surface**.

Nơi này chứa:

- epic hierarchy
- story/task/bug content
- planning/approach/validation notes cho từng work item
- repo-level tactical `test-matrix.md` có status/evidence sống cùng application/product work hiện hành
- application/product backlog chưa cần slice thành item riêng
- task/bug verification evidence và story-level lifecycle summary khi cần audit closeout

### Quy tắc chính

- `works/` là nơi con người và agent đọc để hiểu **application/product work thực tế**
- `works/` không phải machine source of truth cho graph metadata
- graph metadata canonical nằm ở `.pulse/workgraph/`
- mỗi entity directory có `README.md` làm entry file canonical

---

## `.pulse/`

Là **runtime + machine plane**.

Chứa:

- harness/agent operating contract và harness improvement backlog
- workgraph metadata
- runtime state
- handoff/checkpoint/reservation
- reusable machine memory
- repo-local helper scripts

`.pulse/cache/` được defer sau v1; nếu thêm sau, nó chỉ chứa generated cache/indexes và không phải source of truth.

### Quy tắc chính

- `.pulse/` là internal control plane
- `.pulse/harness/` là harness-facing durable surface, không phải application docs/work
- không dùng `.pulse/` làm nơi viết canonical product docs
- không dùng `.pulse/` làm nơi viết human application/product work content chính
- generated views/cache ở đây phải được coi là derived data, không phải truth

---

## Harness operating model được hấp thụ

Pulse v2 sẽ hấp thụ các ý tưởng chính từ `references/harness-experimental/docs/HARNESS.md`, nhưng route lại vào `.pulse/harness/` để không lẫn với application docs/work.

### Mental model

```text
Human intent
  -> feature intake
  -> work item / story packet
  -> agent work loop
  -> product delta
  -> validation proof
  -> harness delta
  -> next intent
```

### App vs harness

- app là surface người dùng chạm vào
- harness là surface agent/workflow chạm vào
- Pulse v2 không scaffold app stack, CI, DB, infra, hoặc package scripts nếu work item hiện tại chưa cần

### Hai output hợp lệ của mỗi task

Mỗi task có thể tạo một hoặc cả hai loại output:

1. **Product delta**: code, tests, API shape, data model, product docs, hoặc behavior contract.
2. **Harness delta**: docs, templates, validation expectations, backlog entries, decisions, workgraph/schema/CLI improvements.

### Source hierarchy mới

Harness-experimental dùng `docs/stories/*` và `docs/TEST_MATRIX.md`. Pulse v2 đổi routing như sau:

```text
User-provided spec or prompt
  input material cho first buildout hoặc future changes

docs/product/*
  current product contract derived from accepted input

works/epics/**
  story/task/bug-sized work content, evidence, and closeout summaries

works/test-matrix.md
  behavior-to-proof tactical control panel

docs/decisions/*
  durable rationale cho application/product contract changes

.pulse/harness/HARNESS.md
  durable operating harness contract

.pulse/harness/backlog.md
  harness improvement backlog phát sinh từ process/tooling friction
```

### Spec lifecycle

Spec do user cung cấp là input material, không phải permanent operating manual. Sau khi spec được phân rã, ongoing work phải update các surface nhỏ hơn:

- `docs/product/*`
- `works/epics/**`
- `works/test-matrix.md`
- `docs/decisions/*`
- `.pulse/workgraph/items.jsonl`
- `.pulse/harness/HARNESS.md` khi operating contract thay đổi
- `.pulse/harness/backlog.md` khi phát hiện harness friction chưa nên implement ngay

### Input types

Pulse v2 nên phân loại request đầu vào theo các nhóm sau trước khi tạo hoặc update work items:

- new spec
- spec slice
- change request
- new initiative
- maintenance request
- harness improvement

### Growth rule

Harness grows from friction.

Khi agent bị confuse, phải lặp reasoning thủ công, cần validation command mới, phát hiện missing rule, hoặc thấy recurring failure pattern, agent phải:

- update `.pulse/harness/HARNESS.md`, templates, schema, hoặc skill contracts nếu thay đổi nhỏ và an toàn; hoặc
- thêm proposal vào `.pulse/harness/backlog.md` nếu chưa nên đổi operating model ngay.

Không dùng `docs/HARNESS_BACKLOG.md` hoặc `works/backlog.md` làm harness backlog trong Pulse v2. `docs/` là application/product durable docs; `works/` là application/product work surface; harness backlog thuộc `.pulse/harness/`.

### Harness backlog shape

`.pulse/harness/backlog.md` nên giữ shape tối thiểu từ `HARNESS_BACKLOG.md` của harness-experimental:

- title
- discovered while
- current pain
- suggested improvement
- risk
- status

### Validation ladder

Pulse v2 giữ tinh thần validation ladder của harness-experimental nhưng không giả định command tồn tại trước khi repo/app cần:

```text
validate:quick
  format, lint, typecheck, unit tests, architecture check

test:integration
  backend, database, provider, or service checks as the stack requires

test:e2e
  user-visible end-to-end flows

test:platform
  shell, mobile, desktop, or deployment smoke checks as the stack requires

test:release
  full suite, log checks, and performance smoke
```

Agent không được claim command pass nếu command chưa tồn tại hoặc chưa được chạy.

---

## Workgraph model được chốt

### Thuật ngữ chuẩn trong tài liệu này

- Khi nói chung về `EPIC`, `STORY`, `TASK`, `BUG`, tài liệu này sẽ dùng cụm **work item**.
- Khi nói trong schema, JSON, code, hoặc CLI output ngắn gọn, tài liệu này sẽ dùng từ **item**.
- `pulse-work` chỉ là **tên CLI**, không phải tên của object.
- `workgraph` là **tên của hệ metadata/dependency graph**, không phải tên từng entity.

## Entity kinds v1

Pulse v2 workgraph sẽ hỗ trợ 4 loại entity chính:

- `EPIC`
- `STORY`
- `TASK`
- `BUG`

### Ý nghĩa

- `EPIC`: capability stream / domain lớn
- `STORY`: delivery slice chính
- `TASK`: đơn vị thực hiện nhỏ hơn trong một story hoặc epic
- `BUG`: defect/fix work item, có thể gắn vào story hoặc epic

---

## Hierarchy rules

Hierarchy và dependency là **hai khái niệm khác nhau**.

### Hierarchy (parent-child)

- `EPIC` -> không có parent
- `STORY` -> parent phải là `EPIC`
- `TASK` -> parent có thể là `STORY` hoặc `EPIC`
- `BUG` -> parent có thể là `STORY` hoặc `EPIC`

### Vì sao `TASK` / `BUG` được phép ở dưới `EPIC`

Để tránh tạo story giả cho các work như:

- investigation task
- epic setup task
- cross-cutting fix
- cleanup follow-up

---

## Dependency rules

Dependency graph được giữ độc lập với hierarchy.

Ví dụ:

- `S-25F-K3W8` là child của `E-25F-H2X9`
- `T-25F-7K9M` là child của `S-25F-K3W8`
- `T002` phụ thuộc `T-25F-7K9M`

Trong trường hợp này:

- parent-child mô tả cấu trúc work
- `depends_on` mô tả execution order / blocking graph

### CLI ergonomics

Vì canonical ID không còn là số tăng dần, CLI nên hỗ trợ nhập **unique prefix** của ID khi không ambiguous.

Ví dụ:

- lưu canonical `T-25F-7K9M`
- cho phép `pulse-work show T-25F`
- nếu prefix match nhiều item thì CLI bắt buộc user gõ dài hơn

---

## Status model được chốt

Canonical statuses:

- `OPEN`
- `IN_PROGRESS`
- `BLOCKED`
- `CLOSED`

### Quy tắc dùng status

#### `OPEN`
Item tồn tại, chưa bắt đầu hoặc sẵn sàng chờ được kéo.

#### `IN_PROGRESS`
Đang được active work.

#### `BLOCKED`
Chỉ dùng khi có **blocker thực ngoài graph**, ví dụ:

- chờ user decision
- chờ credentials
- chờ environment
- chờ reproduction
- chờ design clarification

#### `CLOSED`
Hoàn thành và đã đóng.

### Quy tắc quan trọng

**Dependency blocking không nhất thiết đổi status thành `BLOCKED`.**

Một item vẫn có thể là `OPEN` nhưng:

- `ready = false`
- `blocked_by_dependencies = [T-25F-7K9M, T-25F-Q2N4]`

Tức là:

- graph blocking là derived state
- external blocking là explicit status + reason

---

## ID strategy được chốt

Canonical IDs sẽ là **short distributed-safe IDs theo kind**, không encode full hierarchy và không dựa vào global counter.

### Format canonical ID

```text
<KIND>-<TIMEBUCKET>-<RANDOM>
```

Ví dụ:

- `E-25F-H2X9`
- `S-25F-K3W8`
- `T-25F-7K9M`
- `B-25F-M8R2`

Trong đó:

- `<KIND>` là prefix theo loại item: `E`, `S`, `T`, `B`
- `<TIMEBUCKET>` là short creation bucket để tăng readability và hỗ trợ eyeballing order gần đúng
- `<RANDOM>` là suffix random ngắn, dùng alphabet tránh ký tự mơ hồ để giảm conflict khi nhiều người tạo item trên các branch khác nhau

### Quy tắc chốt

- chỉ có **một canonical ID duy nhất**
- không có `display_id` tuần tự kiểu `T233`
- không có shared counter file như `_id.json` hay `ids.json`
- ID phải immutable sau khi tạo

### Không dùng các dạng sau

- `S014`
- `T233`
- `B019`
- `E01-S01-T01`

### Lý do

- counter theo repo/branch sẽ conflict khi nhiều người tạo item song song rồi merge
- `display_id` tuần tự nhìn thân thiện nhưng sẽ trở thành identity thứ hai trong đầu người dùng và gây ambiguity khi bị trùng
- composite hierarchical ID làm move task sang story khác rất đau
- hierarchy change sẽ kéo theo ID churn
- rename/restructure khó hơn

### Hệ quả tích cực

- tạo item offline hoặc trên branch riêng vẫn an toàn hơn
- merge nhiều luồng công việc song song không cần coordinator trung tâm chỉ để cấp ID
- user vẫn có ID ngắn, có kind prefix, và đủ dễ đọc để dùng trong chat/CLI/review
- schema vẫn đủ chỗ để gắn runtime assignment qua `assignee` mà không phải overload identity

---

## Canonical metadata storage

### Source of truth duy nhất

```text
.pulse/workgraph/items.jsonl
```

Mỗi dòng là một entity record.

### Generated views

```text
.pulse/workgraph/views/
|-- active.json   # items đang open/in_progress/blocked
|-- closed.json   # items đã closed
|-- ready.json    # items OPEN và không bị deps block
`-- graph.json    # materialized graph view
```

### Quy tắc chốt

- `items.jsonl` là canonical writable source
- `.pulse/workgraph/schema.json` là required ngay từ v1 để validate records
- `active/closed/ready/graph` chỉ là generated materialized views
- không dùng hai file writable như `active.jsonl` và `closed.jsonl` làm dual source of truth
- không tạo event-sourced log riêng ngoài `items.jsonl` trong v1

### Lý do

Nếu có hai canonical writable files (`active` + `closed`), sẽ phát sinh các vấn đề:

- move item giữa 2 file
- reopen complexity
- partial write / duplicate records
- race conditions
- audit khó sạch

---

## Schema v1 được chốt cho `items.jsonl`

Ví dụ record:

```json
{
  "id": "T-25F-7K9M",
  "kind": "TASK",
  "title": "Implement session store",
  "slug": "session-store",
  "status": "OPEN",
  "parent_id": "S-25F-K3W8",
  "epic_id": "E-25F-H2X9",
  "depends_on": [],
  "priority": 2,
  "owner": null,
  "assignee": "agent-executor-1",
  "labels": ["auth", "session"],
  "risk_flags": ["AUTH", "EXISTING_BEHAVIOR"],
  "blocked_reason": null,
  "content_path": "works/epics/E-25F-H2X9-authentication/S-25F-K3W8-oauth-login/tasks/T-25F-7K9M-session-store/README.md",
  "verification_path": "works/epics/E-25F-H2X9-authentication/S-25F-K3W8-oauth-login/tasks/T-25F-7K9M-session-store/verification.md",
  "created_at": "2026-05-14T10:00:00Z",
  "updated_at": "2026-05-14T10:00:00Z",
  "closed_at": null
}
```

## Fields bắt buộc

- `id`
- `kind`
- `title`
- `slug`
- `status`
- `parent_id`
- `epic_id`
- `depends_on`
- `content_path`
- `created_at`
- `updated_at`

## Fields nên có sớm

- `priority`
- `owner`
- `assignee`
- `labels`
- `risk_flags`
- `verification_path`
- `blocked_reason`
- `closed_at`

### Semantics cho assignment

- `assignee` = agent name hoặc actor name đang được assign trực tiếp để thực hiện item
- `owner` = người hoặc role chịu trách nhiệm tổng thể cho item, có thể khác với assignee

Ví dụ:

- `owner = "quan"`
- `assignee = "agent-executor-1"`

Nếu repo không cần tách hai khái niệm này ở v1, có thể tạm để `owner = null` và chỉ dùng `assignee` cho runtime work assignment.

---

## Human-facing content routing được chốt

### General rule

Graph metadata ở `.pulse/workgraph/`.

Human-facing content ở `works/`.

### Path strategy

#### Không route theo slug-only

Không dùng path kiểu:

- `<epic-name>/<story-name>/<task-name>`

#### Dùng ID + slug

Ví dụ:

```text
works/
`-- epics/
    `-- E-25F-H2X9-authentication/
        `-- S-25F-K3W8-oauth-login/
            `-- tasks/
                `-- T-25F-7K9M-session-store/
```

### Lý do

- ID là identity ổn định
- slug chỉ giúp readability
- rename slug không phá identity

---

## Canonical markdown naming

Mỗi entity directory sẽ có **`README.md` làm canonical living file**.

### Ví dụ

- `works/epics/E-25F-H2X9-authentication/README.md`
- `works/epics/E-25F-H2X9-authentication/S-25F-K3W8-oauth-login/README.md`
- `works/epics/E-25F-H2X9-authentication/S-25F-K3W8-oauth-login/tasks/T-25F-7K9M-session-store/README.md`

### Không dùng các tên entry file khác nhau kiểu

- `epic.md`
- `story.md`
- `task.md`

### Lý do

- đồng nhất tối đa
- dễ tooling
- dễ route
- dễ preview
- mở folder là thấy entry file mặc định

---

## Optional satellite files cho work items

### Story-level

Có thể có nhưng **không bắt buộc**:

- `approach.md`
- `execplan.md`
- `validation.md`
- `references/*`

### Task/Bug-level

Có thể có:

- `verification.md`

### Quy tắc

Không ép mọi work item phải có đầy đủ 4-5 file.

Nếu bắt buộc quá nhiều file cho mọi item, Pulse v2 sẽ tái tạo file explosion của Pulse hiện tại.

---

## `works/` details được chốt

### `backlog.md`

Dùng làm entry backlog đơn giản cho application/product work chưa được slice thành EPIC/STORY/TASK/BUG.

### Không ưu tiên

- `works/backlogs/` với nhiều tầng trừ khi thật sự phát sinh nhu cầu
- harness improvement proposals; phần này thuộc `.pulse/harness/backlog.md`

### Lý do

Giữ v1 đơn giản hơn, tránh biến application/product backlog thành nơi dump ý tưởng thiếu cấu trúc hoặc trộn lẫn process/tooling work của harness.

---

## Epic structure

Ví dụ:

```text
works/
`-- epics/
    `-- E-25F-H2X9-authentication/
        `-- README.md
```

Epic `README.md` nên chứa:

- mục tiêu capability
- boundary của epic
- danh sách stories liên quan
- open questions ở level epic

---

## Story structure

Ví dụ:

```text
works/
`-- epics/
    `-- E-25F-H2X9-authentication/
        `-- S-25F-K3W8-oauth-login/
            |-- README.md
            |-- approach.md      # optional
            |-- execplan.md      # optional
            |-- validation.md    # optional
            |-- references/
            |-- lifecycle-summary.md # optional
            `-- tasks/
```

### Story `README.md` nên chứa

- title
- status
- short request summary
- scope / non-goals
- acceptance criteria
- related product docs
- related decisions
- link tới `lifecycle-summary.md` nếu có
- link tới task/bug verification evidence quan trọng

---

## Task / Bug structure

Ví dụ:

```text
works/
`-- epics/
    `-- E-25F-H2X9-authentication/
        `-- S-25F-K3W8-oauth-login/
            `-- tasks/
                `-- T-25F-7K9M-session-store/
                    |-- README.md
                    `-- verification.md
```

### Task/Bug `README.md` nên chứa

- human-readable scope
- local implementation notes
- related files
- important caveats

### `verification.md`

- evidence summary
- commands run
- observed outputs
- execution attempts nếu có retry hoặc failure trước khi pass
- links tới generated proof artifacts, screenshots, logs, hoặc findings files
- unresolved gaps

### Quy tắc

- task metadata canonical vẫn ở `items.jsonl`
- không tạo `tasks.jsonl` local mirror mặc định; story task list phải derive từ `.pulse/workgraph/items.jsonl`
- `README.md` và `verification.md` không được tự trở thành source of truth cho status/deps/owner
- không tạo `runs/` chỉ để lưu từng execution attempt; attempt đó thuộc task/bug `verification.md`

---

## Brownfield onboarding và docs backup policy

Khi Pulse được dùng trong **brownfield repo** đã có sẵn docs riêng, onboarding/restructure không được giả định là an toàn nếu không có snapshot trước.

### Quy tắc chốt

- Nếu `using-pulse` hoặc onboarding phát hiện docs cũ và cần tạo layout mới, split docs, hoặc remap docs hiện có, thì **phải tạo backup trước**.
- Backup này là **migration artifact**, không phải canonical live docs.
- Vị trí backup được chốt là:

```text
.pulse/migrations/docs-backups/<timestamp-or-migration-id>/
```

### Nội dung backup nên giữ

- toàn bộ docs cũ nếu phạm vi đổi lớn
- hoặc subset docs bị Pulse sắp rewrite/remap nếu migration đủ targeted
- manifest ngắn mô tả snapshot được tạo khi nào và vì sao

### Quy tắc đọc

- Agent không được coi backup trong `.pulse/migrations/docs-backups/` là current truth nếu đã có docs mới canonical ở `docs/` hoặc work content mới ở `works/`
- Backup chỉ dùng cho rollback, audit migration, hoặc manual recovery

---

## Audit trail thay cho top-level `history/` và `runs/`

Top-level `history/` sẽ **không còn là source of truth chính**. Đồng thời, `runs/` cũng không nên trở thành một audit surface mặc định mới.

Thay vào đó:

```text
S-25F-K3W8-oauth-login/
|-- lifecycle-summary.md      # optional durable closeout summary ở story level
`-- tasks/
    `-- T-25F-7K9M-session-store/
        `-- verification.md   # execution attempts, commands, outputs, evidence, gaps
```

### Ý nghĩa

- audit trail nằm gần task/bug thực sự được execute
- story chỉ giữ lifecycle summary khi cần tổng kết closeout hoặc onboarding sau này
- không tạo archive tree song song với task graph
- không trộn durable work evidence với repo-wide docs hay runtime state

---

## CLI direction được chốt

CLI v1 sẽ là:

```text
pulse-work
```

## Commands tối thiểu đề xuất

- `pulse-work create`
- `pulse-work show <id>`
- `pulse-work list`
- `pulse-work ready`
- `pulse-work update <id>`
- `pulse-work close <id>`
- `pulse-work reopen <id>`
- `pulse-work dep add <id> <depends-on>`
- `pulse-work dep rm <id> <depends-on>`
- `pulse-work children <id>`
- `pulse-work graph`
- `pulse-work doctor`

## Hành vi quan trọng nhất

### `ready`
Phải trả ra các item:

- `status = OPEN`
- không bị dependency blocking
- không có external blocker

Đây là phần quan trọng nhất cần giữ lại từ beads/Flywheel.

---

## Quyết định v1 đã chốt thêm

Các nội dung từng deferred nay được chốt cho v1 như sau:

- giữ `docs/GLOSSARY.md` mặc định
- tạo repo-level tactical `works/test-matrix.md` ngay từ đầu
- tạo `.pulse/workgraph/schema.json` ngay từ đầu
- không tạo event-sourced log riêng ngoài `items.jsonl` trong v1
- defer `.pulse/cache/` sau v1

## Những gì không chốt / deferred

Các nội dung sau **chưa bắt buộc chốt ngay trong v1**:

- có cần một durable testing policy doc trong `docs/` hay không

### Tạm kết luận

- durable testing policy doc trong `docs/`: optional, chỉ thêm khi cần ghi lại repo-wide testing policy / coverage philosophy
- `.pulse/cache/`: defer sau v1; nếu thêm sau thì chỉ dùng cho generated cache/indexes, không phải source of truth

---

## Những gì bị loại bỏ khỏi phương án

### Không dùng tên `jira`

Lý do:

- kéo theo mental model issue tracker truyền thống
- không phản ánh đúng nature của work dependency graph runtime

### Không dùng 2 writable files `active.jsonl` / `closed.jsonl`

Lý do:

- dual source of truth
- reopen/migrate phức tạp
- partial write / duplicate risk

### Không commit `docs/backups/`

Lý do:

- backup không phải durable application/product repo truth
- git/runtime checkpoints xử lý tốt hơn

### Không đặt harness contract/backlog trong `docs/` hoặc `works/`

Lý do:

- `docs/` là application/product durable docs, không phải harness operating surface
- `works/` là application/product work surface, không phải process/tooling backlog
- harness contract và harness improvement backlog thuộc `.pulse/harness/`

### Không ép mọi work item phải có đầy đủ

- `approach.md`
- `execplan.md`
- `validation.md`
- nhiều files khác

Lý do:

- tái tạo file explosion của Pulse cũ

### Không dùng composite hierarchical IDs làm canonical IDs

Lý do:

- hierarchy change / move khó khăn
- rename pain

---

## Migration strategy được khuyến nghị

Không làm cùng lúc cả 3 migration lớn.

### Phase 1 — thay engine

- build `pulse-work`
- build `.pulse/workgraph/`
- giữ gate semantics cũ tạm thời
- thay logic phụ thuộc `br` / `bv`

### Phase 2 — thay folder layout

- introduce `works/`
- deprecate `history/` hiện tại
- route work content sang `works/`

### Phase 3 — giảm artifact complexity

- xác lập `README.md` làm canonical entry file
- chỉ giữ optional satellite files khi cần
- simplify skill read/write assumptions

### Lý do cần chia phase

Nếu cùng lúc đổi:

1. runtime engine
2. folder structure
3. skill semantics

thì nguy cơ vỡ behavior sẽ rất cao.

---

## Final decision summary

Pulse v2 sẽ đi theo hướng:

1. **Bỏ `br` / `bv`**
2. **Dùng built-in workgraph runtime**
3. **Dùng `work item` làm generic noun cho `EPIC` / `STORY` / `TASK` / `BUG`**
4. **Dùng `item` làm short noun trong schema/code**
5. **Dùng `pulse-work` làm tên CLI, không dùng nó làm object name**
6. **Đặt machine metadata trong `.pulse/workgraph/`**
7. **Đặt human work content trong `works/`**
8. **Giữ `docs/` rất sạch, chỉ cho durable application/product repo truth**
9. **Bỏ top-level `history/` như source of truth chính**
10. **Không dùng `runs/` làm audit surface mặc định; execution evidence đi theo task/bug `verification.md`, story closeout đi qua `lifecycle-summary.md` khi cần**
11. **Một canonical metadata source duy nhất: `items.jsonl`**
12. **Canonical entry markdown file cho mọi entity là `README.md`**
13. **ID strategy dùng short distributed-safe IDs theo kind, không dùng counter hay display ID tuần tự**
14. **Giữ `docs/GLOSSARY.md` mặc định**
15. **Tạo `works/test-matrix.md` ngay từ đầu cho tactical verification matrix**
16. **Tạo `.pulse/workgraph/schema.json` ngay từ đầu**
17. **Không tạo event-sourced log riêng ngoài `items.jsonl` trong v1**
18. **Defer `.pulse/cache/` sau v1**
19. **Hấp thụ `HARNESS.md` thành `.pulse/harness/HARNESS.md` làm durable operating harness contract**
20. **Không giữ `docs/HARNESS_BACKLOG.md` hoặc trộn harness backlog vào `works/backlog.md`; harness improvement proposals đi vào `.pulse/harness/backlog.md`**

---

## Immediate next steps

Thứ tự nên làm tiếp:

1. viết chi tiết `workgraph schema v1` và `.pulse/workgraph/schema.json`
2. chốt `pulse-work CLI command spec`
3. chốt `.pulse/harness/HARNESS.md` và `.pulse/harness/backlog.md` contract, gồm cách migrate từ harness-experimental
4. chốt `works/` markdown contract (`README.md`, `approach.md`, `execplan.md`, `validation.md`, `verification.md`, `test-matrix.md`, `backlog.md`)
5. lập migration blueprint từ Pulse hiện tại sang Pulse v2
6. sửa dần skill contracts theo phase

---

## File purpose

Tài liệu này là bản chốt định hướng refactor kiến trúc cho Pulse v2, dựa trên các thảo luận về:

- bỏ beads/beads_viewer
- hấp thụ một phần tư duy harness-experimental
- giữ `.pulse/` cho runtime/control plane
- đưa work content sang `works/`
- làm rõ machine graph vs human content
