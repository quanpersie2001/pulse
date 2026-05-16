# Phase 2 — `pulse-work` Engine v1

## 1. Mục tiêu phase

Phase này dựng runtime CLI thật cho workgraph v2. Sau phase này, `pulse-work` không còn là ý tưởng trong spec mà trở thành canonical runtime interface để tạo, đọc, cập nhật, đóng, mở lại, và kiểm tra work items.

Nếu phase 1 tạo public router `/pulse`, thì phase 2 tạo **control plane thực thi phía dưới**.

## 2. Kết quả bắt buộc sau phase này

Sau phase 2, repo phải có:

1. `skills/pulse/scripts/runtime/pulse_work.mjs` làm source entrypoint cho CLI
2. bộ runtime modules cho:
   - store
   - validate
   - ids
   - paths
   - views
   - lock
   - templates
3. schema file ở `.pulse/workgraph/schema.json`
4. canonical metadata file ở `.pulse/workgraph/items.jsonl`
5. generated views dưới `.pulse/workgraph/views/`
6. write lock ở `.pulse/workgraph/write.lock`
7. command set tối thiểu:
   - `create`
   - `show`
   - `list`
   - `ready`
   - `update`
   - `close`
   - `reopen`
   - `dep add`
   - `dep rm`
   - `children`
   - `graph`
   - `doctor`

## 3. Vai trò của `pulse-work`

`pulse-work` là runtime CLI. Nó **không** phải user-facing conversational skill.

### `/pulse`

- chịu trách nhiệm route workflow
- giải thích command intent
- hướng dẫn user/agent chọn mode làm việc phù hợp

### `pulse-work`

- chịu trách nhiệm mutate canonical workgraph state
- validate schema và graph rules
- scaffold content dưới `works/`
- rebuild derived views
- enforce close/verification rules

## 4. Deliverables chi tiết

## 4.1 Runtime source modules

### `pulse_work.mjs`

CLI entrypoint. Chịu trách nhiệm:

- parse argv
- dispatch subcommands
- chuẩn hóa human output vs `--json`
- kết nối vào store/validator/path/template modules

### `workgraph_store.mjs`

Chịu trách nhiệm:

- load `items.jsonl`
- serialize records theo ordering deterministic
- write temp file + atomic rename
- centralize read/write lifecycle

### `workgraph_validate.mjs`

Chịu trách nhiệm enforce:

- schema fields
- enum values
- hierarchy rules
- dependency rules
- status transitions
- close/reopen rules
- verification requirements
- path safety

### `workgraph_ids.mjs`

Chịu trách nhiệm:

- map kind prefix (`E`, `S`, `T`, `B`)
- generate `<KIND>-<TIMESECOND>[-<SEQ>]`
- detect same-kind same-second collisions
- prefix resolution cho CLI lookup

### `workgraph_paths.mjs`

Chịu trách nhiệm:

- slug sanitize
- derive canonical content paths
- reject path traversal / absolute paths / escapes
- implement cascade path updates khi epic/story đổi slug

### `workgraph_views.mjs`

Chịu trách nhiệm:

- build `active.json`
- build `closed.json`
- build `ready.json`
- build `graph.json`
- attach derived fields như readiness, children, reverse deps

### `workgraph_lock.mjs`

Chịu trách nhiệm:

- acquire/release filesystem lock
- read lock JSON metadata
- distinguish active lock vs stale lock
- expose helpers cho `doctor --fix`

### `workgraph_templates.mjs`

Chịu trách nhiệm:

- generate markdown files trong `works/`
- epic/story/task/bug README scaffolds
- `verification.md` scaffold cho `TASK`/`BUG`

## 4.2 Canonical files trong runtime plane

### `.pulse/workgraph/items.jsonl`

- canonical writable metadata source duy nhất
- mỗi line là full snapshot của một item
- không dùng như append-only event log

### `.pulse/workgraph/schema.json`

- machine-readable contract
- validator code phải enforce cùng rules với schema file

### `.pulse/workgraph/views/*`

- derived only
- atomic write
- deterministic rebuild
- gitignored

## 5. Implementation order chi tiết

## 5.1 Workstream A — model foundations

Đây là phần phải làm trước mọi command implementation.

### Việc phải làm

1. định nghĩa shape record thống nhất với `SPEC.md`
2. định nghĩa enums:
   - item kinds
   - statuses
   - risk flags
3. định nghĩa helper để parse JSONL an toàn
4. định nghĩa deterministic ordering của records

### Output mong muốn

- codebase có internal model rõ ràng cho item record
- mọi command sau này dùng chung model này, không tự parse lẻ tẻ

## 5.2 Workstream B — ID, slug, path layer

### Việc phải làm

1. implement ID generator theo Base32 uppercase
2. implement collision suffixing
3. implement unique-prefix resolution
4. implement slug sanitizer
5. implement path derivation cho:
   - epic
   - story
   - task/bug dưới story
   - task/bug dưới epic
6. implement path cascade rules khi parent slug đổi

### Đây là chỗ dễ sai nhất

Nếu path derivation và rename cascade không được thiết kế chuẩn ngay từ đầu, `update --slug` sau này sẽ làm `items.jsonl` và `works/` drift khỏi nhau.

## 5.3 Workstream C — validator layer

### Việc phải làm

1. validate required fields
2. validate nullable fields theo status
3. validate hierarchy:
   - `EPIC.parent_id = null`
   - `STORY.parent_id -> EPIC`
   - `TASK/BUG.parent_id -> STORY | EPIC`
4. validate `epic_id`
5. validate `depends_on`
6. detect cycles
7. validate close rules
8. validate verification heading contract

### Quy tắc quan trọng

- `CLOSED` không được set bằng generic status update
- close phải đi qua `pulse-work close`
- reopen phải đi qua `pulse-work reopen`

## 5.4 Workstream D — store + lock + atomic writes

### Việc phải làm

1. tạo lock acquisition flow
2. viết temp file + rename cho `items.jsonl`
3. viết temp file + rename cho từng view
4. đảm bảo mọi mutate command follow chung mutation pipeline

### Mutation pipeline chuẩn

1. acquire lock
2. load items
3. apply mutation in memory
4. validate toàn graph
5. write `items.jsonl`
6. rebuild views
7. release lock

Không command nào được bypass pipeline này.

## 5.5 Workstream E — CLI command implementation

### `create`

Phải làm được:

- validate inputs
- generate ID + slug
- derive paths
- create markdown scaffolds
- insert item record
- rebuild views

### `show`

Phải trả về:

- canonical record
- derived readiness/dependency context
- resolved content paths

### `list`

Phải support filters cơ bản:

- `--kind`
- `--status`
- `--epic`
- `--parent`
- `--owner`
- `--label`

### `ready`

Phải chỉ trả về items thỏa:

- `status = OPEN`
- `blocked_reason = null`
- tất cả dependencies đã `CLOSED`

### `update`

Phải support:

- title/slug/priority/owner
- add/remove label
- add/remove risk
- blocked reason
- safe path updates

### `close`

Phải enforce:

- child close rules
- verification rules cho `TASK`/`BUG`
- set `closed_at`

### `reopen`

Phải:

- reset về `OPEN`
- clear `closed_at`

### `dep add` / `dep rm`

Phải:

- validate item existence
- block cycles
- rebuild views

### `children`

Phải:

- return direct children
- support `--json`

### `graph`

Phải:

- materialize nodes + hierarchy/dependency edges
- dùng chung logic với `views/graph.json`

### `doctor`

Phải detect:

- schema violations
- duplicate IDs
- broken parent refs
- inconsistent `epic_id`
- missing dependencies
- cycles
- missing/broken paths
- manual move drift
- stale views
- stale lock
- frontmatter metadata leaks

## 5.6 Workstream F — `.pulse/scripts/` delivery surface

Phase 2 nên chuẩn bị thin executable/runtime-facing delivery surface để self-host dogfood hoạt động.

### Việc phải làm

- xác định wrapper name `pulse-work`
- materialize hoặc sync `pulse_work.mjs` vào `.pulse/scripts/`
- đảm bảo human-facing runtime command vẫn là `pulse-work`

## 6. Behavioral contract của workgraph v1

## 6.1 Item lifecycle

- `OPEN` = chưa active, có thể ready hoặc chưa ready do dependencies
- `IN_PROGRESS` = đang được thực thi
- `BLOCKED` = bị external blocker, không phải dependency-derived blocker
- `CLOSED` = hoàn tất và pass close contract

## 6.2 Owner vs reservation

Phase này phải giữ distinction rất rõ:

- `owner` = durable metadata trong item
- reservation = ephemeral execution lease trong runtime file

Không được lén introduce persisted `assignee` field.

## 6.3 Content contract trong `works/`

Mọi item content sống dưới `works/`, không sống trong `.pulse/workgraph/`.

### Epic

- `README.md`

### Story

- `README.md`
- optional planning/validation support docs nếu phase sau dùng

### Task/Bug

- `README.md`
- `verification.md`

## 7. Verification của phase

## 7.1 Unit tests bắt buộc

- ID generation
- collision suffix
- prefix resolution
- slug sanitize
- path derivation
- hierarchy validation
- dependency cycle detection
- close/reopen rules
- verification heading checks
- stale lock detection

## 7.2 Integration tests bắt buộc

- create epic/story/task/bug
- create direct task/bug under epic
- add/remove dependency
- `ready --json`
- close fail nếu thiếu verification
- reopen clears `closed_at`
- story/epic slug rename cascade path updates
- doctor detects manual drift

## 7.3 Golden fixtures

- `items.jsonl`
- `active.json`
- `closed.json`
- `ready.json`
- `graph.json`
- generated markdown templates

## 8. Rủi ro chính

## 8.1 Rủi ro: path rename cascade làm vỡ descendant records

**Xử lý:** path layer phải được build trước `update --slug`; integration tests bắt buộc cover epic/story rename.

## 8.2 Rủi ro: mutation commands viết mỗi nơi một kiểu

**Xử lý:** buộc mọi mutate command đi qua cùng mutation pipeline trong store/lock layer.

## 8.3 Rủi ro: `doctor` trở thành dump checker yếu

**Xử lý:** `doctor` phải được xem như audit contract, không chỉ là tiện ích phụ.

## 8.4 Rủi ro: `.pulse/scripts/` drift khỏi source-of-truth

**Xử lý:** phase 2 chỉ cần dựng thin delivery surface, còn phase 3 sẽ gắn nó vào onboarding sync flow.

## 9. Exit criteria

Phase 2 hoàn tất khi:

- `pulse-work` có thể thao tác canonical workgraph thật
- `items.jsonl` là writable metadata truth duy nhất
- `works/` scaffolding hoạt động theo spec
- validator, lock, và view rebuild đã có thật
- JSON output đủ ổn định để hooks/tests/automation có thể dùng ở phase 5

## 10. Không làm trong phase này

- chưa chuyển toàn bộ onboarding authority sang `/pulse onboard`
- chưa rewrite docs product/hook/eval repo-wide
- chưa xóa legacy skills

Phase này chỉ dựng **runtime engine v1** đủ cứng để các phase migration phía trên có nơi bám vào.