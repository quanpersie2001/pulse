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
|       `-- E-0V9K4F-authentication/
|           # Một capability stream / domain lớn.
|           |
|           |-- README.md
|           |   # Epic overview: mục tiêu, boundary, stories liên quan.
|           |
|           `-- S-0V9K4G-oauth-login/
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
|                   |-- T-0V9K4H-session-store/
|                   |   |
|                   |   |-- README.md
|                   |   |   # Canonical entry file cho task.
|                   |   |
|                   |   `-- verification.md
|                   |       # Narrative proof / commands / attempts / gaps cho task.
|                   |
|                   `-- B-0V9K4J-mobile-oauth-bounce/
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

- `S-0V9K4G` là child của `E-0V9K4F`
- `T-0V9K4H` là child của `S-0V9K4G`
- `T-0V9K4K` phụ thuộc `T-0V9K4H`

Trong trường hợp này:

- parent-child mô tả cấu trúc work
- `depends_on` mô tả execution order / blocking graph

### CLI ergonomics

Vì canonical ID không còn là số tăng dần, CLI nên hỗ trợ nhập **unique prefix** của ID khi không ambiguous.

Ví dụ:

- lưu canonical `T-0V9K4H`
- cho phép `pulse-work show T-0V9K4H`
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
- `blocked_by_dependencies = [T-0V9K4H, T-0V9K4K]`

Tức là:

- graph blocking là derived state
- external blocking là explicit status + reason

---

## ID strategy được chốt

Canonical IDs sẽ là **short timestamp-derived IDs theo kind**, không encode full hierarchy và không dựa vào global counter.

### Format canonical ID

```text
<KIND>-<TIMESECOND>[-<SEQ>]
```

Ví dụ:

- `E-0V9K4F`
- `S-0V9K4G`
- `T-0V9K4H`
- `T-0V9K4H-1` nếu có collision cùng kind/cùng second
- `B-0V9K4J`

Trong đó:

- `<KIND>` là prefix theo loại item: `E`, `S`, `T`, `B`
- `<TIMESECOND>` là UTC Unix timestamp ở second granularity, encode compact bằng Base32 alphabet tránh ký tự dễ nhầm
- `<SEQ>` là sequence suffix chỉ xuất hiện khi collision trong cùng kind/cùng second

Second-level timebucket đủ ngắn và dễ sort gần đúng theo thời gian, đồng thời giảm nhu cầu random suffix trong v1.

### Quy tắc chốt

- chỉ có **một canonical ID duy nhất**
- không có `display_id` tuần tự kiểu `T233`
- không có shared counter file như `_id.json` hay `ids.json`
- ID phải immutable sau khi tạo
- timebucket dùng compact UTC second encode bằng Base32 tránh ký tự dễ nhầm
- không dùng random suffix trong v1
- `pulse-work create` phải check collision trong `items.jsonl` trước khi ghi
- nếu collision cùng kind/cùng second xảy ra, CLI append sequence suffix `-1`, `-2`, ... và chọn sequence đầu tiên chưa tồn tại
- sequence suffix chỉ là collision resolver, không phải display counter hoặc global counter
- input ID từ CLI có thể normalize case khi resolve, nhưng persisted canonical ID vẫn uppercase

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

- tạo item offline hoặc trên branch riêng vẫn an toàn hơn counter dùng chung
- merge nhiều luồng công việc song song không cần coordinator trung tâm chỉ để cấp ID
- user vẫn có ID ngắn, có kind prefix, và đủ dễ đọc để dùng trong chat/CLI/review
- runtime ownership không phải overload identity; nếu cần tránh double-work thì dùng reservation trong `.pulse/runtime/`

### Collision note

Trong cùng một checkout, `pulse-work create` đi qua runtime queue + lock file nên có thể check collision và chọn sequence suffix an toàn trước khi ghi. Across branches vẫn có khả năng cực thấp hai nhánh tạo cùng kind đúng cùng second; khi merge, `pulse-work doctor` phải detect duplicate ID và yêu cầu regenerate một bên trước khi graph được coi là healthy.

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
- generated views trong `.pulse/workgraph/views/` là local generated data và phải được gitignored
- Pulse onboarding/doctor safe-fix phải auto-ensure `.pulse/workgraph/views/` có trong `.gitignore`
- không dùng hai file writable như `active.jsonl` và `closed.jsonl` làm dual source of truth
- không tạo event-sourced log riêng ngoài `items.jsonl` trong v1
- mọi mutation phải đi qua `pulse-work` runtime queue để serialize write vào `items.jsonl`
- runtime queue là process-local/in-memory queue được bảo vệ bằng lock file; không persist pending queue như canonical state
- write vào `items.jsonl` phải dùng atomic write; không partial-write trực tiếp vào canonical file

### Lý do

Nếu có hai canonical writable files (`active` + `closed`), sẽ phát sinh các vấn đề:

- move item giữa 2 file
- reopen complexity
- partial write / duplicate records
- race conditions
- audit khó sạch

Runtime queue giải quyết coordination khi nhiều agent/CLI process muốn mutate graph trên cùng checkout, nhưng không biến mutation history thành source of truth mới. Queue không được persist như durable pending log; nếu process chết giữa chừng, mutation chưa apply phải được retry từ command/user intent thay vì replay từ canonical log. Sau khi transaction được apply, `items.jsonl` vẫn là snapshot canonical duy nhất.

---

## Schema v1 được chốt cho `items.jsonl`

Ví dụ record:

```json
{
  "id": "T-0V9K4H",
  "kind": "TASK",
  "title": "Implement session store",
  "slug": "session-store",
  "status": "OPEN",
  "parent_id": "S-0V9K4G",
  "epic_id": "E-0V9K4F",
  "depends_on": [],
  "priority": 2,
  "owner": null,
  "labels": ["auth", "session"],
  "risk_flags": ["AUTH", "EXISTING_BEHAVIOR"],
  "blocked_reason": null,
  "content_path": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/tasks/T-0V9K4H-session-store/README.md",
  "verification_path": "works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/tasks/T-0V9K4H-session-store/verification.md",
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
- `labels`
- `risk_flags`
- `verification_path`
- `blocked_reason`
- `closed_at`

## Validation rules v1

Schema v1 phải strict ngay từ đầu.

### Enum / shape

- `kind` enum: `EPIC`, `STORY`, `TASK`, `BUG`
- `status` enum: `OPEN`, `IN_PROGRESS`, `BLOCKED`, `CLOSED`
- `priority` dùng convention P0 highest: số nhỏ hơn là ưu tiên cao hơn; `0` là cao nhất
- `labels` là free-form string array
- `risk_flags` là enum strict để phục vụ validation/review routing
- `risk_flags` enum v1: `AUTH`, `DATA`, `SECURITY`, `MIGRATION`, `EXISTING_BEHAVIOR`, `EXTERNAL_API`, `PERFORMANCE`, `UX`, `CI`, `UNKNOWN`
- timestamp fields dùng ISO 8601 UTC string
- `depends_on` là array ID canonical, không cho duplicate

### Nullability / conditional fields

- `parent_id = null` chỉ hợp lệ với `EPIC`
- `epic_id` bắt buộc cho mọi item; với `EPIC`, `epic_id` phải bằng chính `id`
- `blocked_reason` bắt buộc khi `status = BLOCKED`, và phải null khi không blocked
- `closed_at` bắt buộc khi `status = CLOSED`, và phải null khi chưa closed
- `verification_path` bắt buộc để close `TASK` hoặc `BUG`
- `owner` có thể null

### Hierarchy consistency

- `STORY.parent_id` phải trỏ tới `EPIC`
- `TASK.parent_id` có thể trỏ tới `STORY` hoặc `EPIC`
- `BUG.parent_id` có thể trỏ tới `STORY` hoặc `EPIC`
- `epic_id` phải consistent với ancestor epic của item

### Dependency consistency

- dependency có thể cross-epic
- dependency ID phải tồn tại trong `items.jsonl`
- item không được depend vào chính nó
- dependency graph không được có cycle
- `pulse-work` phải chặn mutation tạo cycle ngay khi write
- `pulse-work doctor` vẫn phải detect cycle nếu file bị sửa tay

### Status transitions

State machine v1:

```text
OPEN        -> IN_PROGRESS | BLOCKED | CLOSED
IN_PROGRESS -> BLOCKED | CLOSED | OPEN
BLOCKED     -> OPEN | IN_PROGRESS | CLOSED
CLOSED      -> OPEN   # chỉ qua reopen
```

`CLOSED` không được chuyển trực tiếp bằng `update status`; phải dùng `pulse-work reopen` để reset `closed_at` và đưa item về `OPEN`.

### Close rules

- không cho close parent nếu còn child chưa `CLOSED`
- `TASK` và `BUG` chỉ được close khi `verification_path` tồn tại và có verification evidence tối thiểu
- close phải set `closed_at`
- reopen phải clear `closed_at`

### Semantics cho ownership / reservation

- `owner` = người hoặc role chịu trách nhiệm tổng thể cho item; có thể null
- Pulse v2 không có `assignee` field trong `items.jsonl` v1
- actor/agent đang thực hiện item ở thời điểm hiện tại được track bằng runtime reservation trong `.pulse/runtime/`, không phải metadata bền vững
- reservation là lease ngắn hạn để tránh double-work giữa nhiều agent/process

Ví dụ:

- `owner = "quan"`
- runtime reservation có thể ghi nhận `reserved_by = "agent-executor-1"` cho `T-0V9K4H` trong một TTL ngắn

Nếu repo không cần owner bền vững, có thể để `owner = null` và chỉ dùng reservation cho runtime coordination.

---

## Human-facing content routing được chốt

### General rule

Graph metadata ở `.pulse/workgraph/`.

Human-facing content ở `works/`.

Markdown content là pure human-facing nội dung, không phải metadata source. Mọi metadata liên quan đến item như status, deps, owner, priority, blocked reason, timestamps chỉ có `items.jsonl` là source of truth.

`pulse-work create` và `pulse-work update` được phép tạo và cập nhật markdown content trong `works/` khi cần, nhưng không được biến markdown thành mirror metadata thứ hai.

### Path strategy

#### Không route theo slug-only

Không dùng path kiểu:

- `<epic-name>/<story-name>/<task-name>`

#### Dùng ID + slug

Ví dụ:

```text
works/
`-- epics/
    `-- E-0V9K4F-authentication/
        `-- S-0V9K4G-oauth-login/
            `-- tasks/
                `-- T-0V9K4H-session-store/
```

### Lý do

- ID là identity ổn định
- slug chỉ giúp readability
- rename slug không phá identity

### Path safety

- slug do CLI sinh phải là lowercase kebab-case ASCII
- CLI phải strip unsafe characters hoặc reject title/slug không thể sanitize an toàn
- reject path traversal (`..`, absolute path, encoded traversal)
- reject ghi ra ngoài `works/`
- không follow symlink để overwrite file ngoài vùng cho phép
- không overwrite file/folder đã tồn tại trừ khi đó là update hợp lệ cho đúng item ID
- move/rename content path nên đi qua `pulse-work update --slug` hoặc command tương đương; manual move khiến `content_path` broken phải được `doctor` báo lỗi

---

## Canonical markdown naming

Mỗi entity directory sẽ có **`README.md` làm canonical living file**.

Markdown entry files có thể có frontmatter tối thiểu chỉ để trace identity:

```yaml
---
id: T-0V9K4H
---
```

Frontmatter không được chứa status, deps, owner, priority, blocked reason, hoặc lifecycle timestamps. Các field đó chỉ thuộc `items.jsonl`.

### Ví dụ

- `works/epics/E-0V9K4F-authentication/README.md`
- `works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/README.md`
- `works/epics/E-0V9K4F-authentication/S-0V9K4G-oauth-login/tasks/T-0V9K4H-session-store/README.md`

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

Một backlog entry nên được promote thành work item trong `items.jsonl` khi nó đã actionable: có scope/action đủ rõ và cần tracking status, dependency, hoặc owner.

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
    `-- E-0V9K4F-authentication/
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
    `-- E-0V9K4F-authentication/
        `-- S-0V9K4G-oauth-login/
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
- short request summary
- scope / non-goals
- acceptance criteria
- related product docs
- related decisions
- link tới `lifecycle-summary.md` nếu có
- link tới task/bug verification evidence quan trọng

Story `README.md` không chứa status/deps/owner/priority như metadata; các field đó derive từ `items.jsonl`.

---

## Task / Bug structure

Ví dụ:

```text
works/
`-- epics/
    `-- E-0V9K4F-authentication/
        `-- S-0V9K4G-oauth-login/
            `-- tasks/
                `-- T-0V9K4H-session-store/
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

`TASK` và `BUG` muốn close phải có `verification.md` tồn tại và có evidence tối thiểu. Nếu verification còn gap, gap phải được ghi rõ thay vì đóng im lặng.

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
S-0V9K4G-oauth-login/
|-- lifecycle-summary.md      # optional durable closeout summary ở story level
`-- tasks/
    `-- T-0V9K4H-session-store/
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

### Output format

- mặc định output human-readable
- mọi command cần automation phải hỗ trợ `--json`
- JSON output phải ổn định hơn human output để agent/tooling parse

### `create` / `update`

- `pulse-work create` tạo record trong `items.jsonl` và có thể tạo folder/file content tương ứng trong `works/`
- `pulse-work update` có thể cập nhật metadata và content path/slug/content liên quan khi cần
- mọi metadata item phải ghi vào `items.jsonl`, không ghi metadata vào markdown ngoài frontmatter `id`
- input ID nên hỗ trợ unique prefix; nếu prefix ambiguous thì CLI bắt buộc user gõ dài hơn

### `ready`
Phải trả ra các item:

- `status = OPEN`
- không bị dependency blocking
- không có external blocker

Dependency chỉ được coi là satisfied khi dependency item đã `CLOSED`.

Ready list sort theo:

1. `priority` tăng dần, vì P0 là cao nhất
2. `created_at` tăng dần
3. `id` tăng dần để ổn định output

Đây là phần quan trọng nhất cần giữ lại từ beads/Flywheel.

### `doctor`

`pulse-work doctor` v1 phải detect:

- schema violation
- duplicate item ID
- missing dependency ID
- dependency cycle
- invalid hierarchy / inconsistent `epic_id`
- broken `content_path` hoặc `verification_path`
- stale hoặc missing generated views
- `.pulse/workgraph/views/` chưa được gitignore
- manual move/rename làm content path lệch khỏi `items.jsonl`

`doctor` được phép safe-fix:

- rebuild generated views
- auto-ensure `.pulse/workgraph/views/` trong `.gitignore`
- normalize deterministic ordering/output formatting
- tạo missing directories/templates nếu không overwrite file hiện có

`doctor` không được tự ý đổi metadata lifecycle, close/reopen item, hoặc overwrite human-authored markdown content.

---

## Quyết định v1 đã chốt thêm

Các nội dung từng deferred nay được chốt cho v1 như sau:

- giữ `docs/GLOSSARY.md` mặc định
- tạo repo-level tactical `works/test-matrix.md` ngay từ đầu
- tạo `.pulse/workgraph/schema.json` ngay từ đầu
- schema v1 strict ngay từ đầu
- ID dùng compact UTC second + optional sequence suffix khi collision
- generated views là local generated data và phải gitignored
- onboarding/doctor safe-fix auto-ensure `.pulse/workgraph/views/` trong `.gitignore`
- mutations đi qua process-local runtime queue + lock file nhưng queue không phải canonical audit log
- CLI output mặc định human-readable, có `--json` cho automation
- `priority` dùng P0 highest convention
- `TASK`/`BUG` close bắt buộc có verification evidence
- markdown content chỉ chứa nội dung human-facing; chỉ cho frontmatter `id`, không chứa metadata item
- không có `assignee` field trong `items.jsonl` v1; actor/agent đang làm item chỉ được track bằng runtime reservation ngắn hạn
- dependency cross-epic được phép, nhưng cycle bị chặn ngay khi write
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

### Phase 1 — thay engine và route gate artifacts sớm

- build `pulse-work`
- build `.pulse/workgraph/`
- build strict `.pulse/workgraph/schema.json`
- build process-local runtime queue + lock file để serialize mutations vào `items.jsonl`
- build generated views và auto-ensure `.pulse/workgraph/views/` được gitignore
- thay logic phụ thuộc `br` / `bv`
- giữ gate semantics cũ về mặt human approval, nhưng route artifacts sớm sang `works/` thay vì tiếp tục coi `history/` là surface chính

### Phase 2 — hoàn tất folder layout và skill routing

- introduce đầy đủ `works/`
- deprecate `history/` hiện tại
- route work content sang `works/`
- cập nhật skill contracts để đọc/write `works/` và `.pulse/workgraph/` theo source hierarchy mới

### Phase 3 — giảm artifact complexity

- xác lập `README.md` làm canonical entry file
- chỉ giữ optional satellite files khi cần
- simplify skill read/write assumptions

### Migration blueprint

Migration từ beads/history sang Pulse v2 nên bắt đầu bằng manual blueprint, không one-shot script tự động ngay từ đầu.

Blueprint cần mô tả:

- cách map `.beads/` item cũ sang `.pulse/workgraph/items.jsonl`
- cách map `history/<feature>/...` sang `works/epics/**`
- cách preserve verification evidence vào task/bug `verification.md`
- cách preserve story closeout vào `lifecycle-summary.md` khi cần
- cách backup brownfield docs vào `.pulse/migrations/docs-backups/<timestamp-or-migration-id>/`
- tiêu chí khi nào có thể bỏ hẳn `br` / `bv` khỏi workflow contracts

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
17. **Schema v1 strict ngay từ đầu: enum/status transitions/hierarchy/dependency/close rules đều được validate**
18. **ID dùng compact UTC second + optional sequence suffix khi collision, collision check trước khi ghi**
19. **Generated views trong `.pulse/workgraph/views/` là local generated data và phải gitignored; onboarding/doctor safe-fix auto-ensure `.gitignore`**
20. **Mọi mutation đi qua process-local runtime queue + lock file, nhưng queue không phải event-sourced canonical log**
21. **Không tạo event-sourced log riêng ngoài `items.jsonl` trong v1**
22. **Defer `.pulse/cache/` sau v1**
23. **`pulse-work` mặc định output human-readable và hỗ trợ `--json` cho automation**
24. **Ready list sort theo priority P0-highest, rồi `created_at`, rồi `id`**
25. **TASK/BUG close bắt buộc có verification evidence**
26. **Markdown trong `works/` là pure human-facing content, chỉ cho frontmatter `id`; metadata item chỉ thuộc `items.jsonl`**
27. **Không có `assignee` field trong `items.jsonl` v1; `.pulse/runtime` reservation là runtime lease ngắn hạn cho actor/agent đang giữ item**
28. **Dependency cross-epic được phép, nhưng cycle bị chặn khi write**
29. **Hấp thụ `HARNESS.md` thành `.pulse/harness/HARNESS.md` làm durable operating harness contract**
30. **Không giữ `docs/HARNESS_BACKLOG.md` hoặc trộn harness backlog vào `works/backlog.md`; harness improvement proposals đi vào `.pulse/harness/backlog.md`**

---

## Immediate next steps

Thứ tự nên làm tiếp:

1. viết `.pulse/workgraph/schema.json` theo strict validation rules đã chốt
2. viết `pulse-work CLI command spec` chi tiết cho create/update/list/ready/close/reopen/dep/doctor
3. thiết kế process-local runtime queue + lock file để serialize mutations nhưng không tạo canonical event log
4. chốt `.pulse/harness/HARNESS.md` và `.pulse/harness/backlog.md` contract, gồm cách migrate từ harness-experimental
5. chốt template markdown cho `works/` với frontmatter `id` only
6. lập manual migration blueprint từ Pulse hiện tại sang Pulse v2
7. sửa dần skill contracts theo phase, bắt đầu bằng route gate artifacts sớm sang `works/`

---

## File purpose

Tài liệu này là bản chốt định hướng refactor kiến trúc cho Pulse v2, dựa trên các thảo luận về:

- bỏ beads/beads_viewer
- hấp thụ một phần tư duy harness-experimental
- giữ `.pulse/` cho runtime/control plane
- đưa work content sang `works/`
- làm rõ machine graph vs human content
