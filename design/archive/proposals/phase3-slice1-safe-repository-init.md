# Phase 3 Slice 1 — Safe Repository Initialization

## Trạng thái

Đã implement trên `features/harness-experimental` sau real-browser Story QA
slice. Proposal này ghi lại ownership và failure contract đã ship; nó không tạo
một planning system thứ hai và không enroll chính Pulse development repository.

## Vấn đề

Graph, evidence, documentation và knowledge đã có bootstrap primitive riêng,
nhưng maintainer phải gọi từng primitive và tự giữ đúng identity order. Chưa có
public `pulse init`, nên target repository mới có thể chỉ được enroll một phần,
hoặc phát hiện conflict ở domain sau khi domain trước đã ghi canonical state.

Public initializer cũng phải phân biệt Pulse-owned defaults với file do
maintainer sở hữu. Đặc biệt, command không được âm thầm rewrite `.gitignore`,
documentation, work prose hoặc authority grants hiện có.

## Phạm vi

Slice thêm một local-first Core operation:

```text
pulse --repo-root <target> init [--json]
```

Command compose:

1. graph/workgraph bootstrap;
2. evidence schemas, repository identity và immutable stores;
3. documentation registry dùng repository identity đó;
4. knowledge manifest dùng cùng repository identity;
5. canonical empty default-deny authority policy khi policy chưa tồn tại;
6. durable source roots cho `docs/`, `works/`, `knowledge/learnings/` và
   `.pulse/events/`.

Command report `.pulse/runtime/` và `.pulse/cache/` thành proposed ignore
entries, nhưng không tự sửa `.gitignore`.

## Ownership

- `kernel::init` chỉ own cross-domain ordering, repository-wide lock,
  managed-root safety và aggregate report.
- Graph, evidence, docs, knowledge và policy tiếp tục own schema, validation và
  bootstrap semantics của mình.
- CLI chỉ parse `init`, gọi Core và render human/JSON output.
- Daemon không tham gia; repository enrollment phải chạy được offline.

Slice không thêm generic bootstrap framework, DI layer, migration engine hoặc
repository abstraction thứ hai.

## Safety contract

### Preflight trước canonical writes

Sau khi kiểm tra managed paths, command acquire repository write lock, recover
prepared Core transactions rồi validate lại toàn bộ domain trước canonical
enrollment write đầu tiên.

- Workgraph chấp nhận empty, current hoặc recognized safe-current partial state.
- Evidence chấp nhận current manifest, hoặc current partial schemas khi chưa có
  receipt/artifact thiếu repository identity.
- Docs chấp nhận current registry, hoặc chỉ current partial document schema.
- Knowledge chấp nhận current manifest, hoặc current partial schemas khi chưa có
  learning/relation records.
- Authority chấp nhận missing policy hoặc existing canonical valid policy.
- Managed roots và files phải là real path đúng loại; file conflict và symlink
  đều fail closed trước khi Pulse có thể ghi ra ngoài repository.

Preflight failure có thể giữ local runtime lock path đã tồn tại, nhưng không tạo
graph, evidence, docs, knowledge, policy, events hoặc work-content truth mới.

### Apply order và retry

Apply order là graph → evidence → docs → knowledge → authority. Evidence tạo
stable `repository_id`; docs và knowledge phải match identity đó.

Bootstrap dùng create-new/preserve behavior. Recognized current partial layouts
có thể được hoàn tất sau interruption, vì vậy retry an toàn mà không cần broad
cross-domain transaction hoặc destructive rollback.

### User-owned state

- Existing files không bị overwrite chỉ vì `init` được gọi.
- Existing valid authority principals/grants được preserve byte-for-byte.
- Existing `docs/`, `works/`, `knowledge/` và `.gitignore` content được giữ lại.
- Schema hoặc identity drift trả typed error và yêu cầu explicit repair/migration
  ngoài command này.

## Output contract

JSON output là versioned `RepositoryInitReport` gồm:

- `code = repository_initialized`;
- `status = initialized|unchanged`;
- shared `repository_id`;
- current authority policy revision;
- sorted repository-relative `created` và `preserved` paths;
- proposed ignore entries.

Absolute temporary path không thuộc stable output.

## Acceptance evidence

`tests/target_repo/repository_init.rs` chỉ chạy trên
`TestRepo::from_fixture("minimal-service")`, tức tracked fixture được copy ra
external temporary Git repository.

Coverage chứng minh:

- fresh public initialization xuyên mọi Core domain;
- một shared evidence/docs/knowledge repository identity;
- canonical empty default-deny policy;
- preserve user docs, README và `.gitignore`;
- lần init thứ hai idempotent, không có created path;
- hoàn tất safe partial graph/evidence/docs/knowledge layouts;
- preserve existing valid authority policy;
- late docs drift bị reject trước mọi earlier canonical domain write;
- managed-path file/symlink collision bị reject mà không overwrite hoặc ghi ra
  ngoài repository;
- tracked fixture source không đổi.

## Non-goals

- `pulse doctor` hoặc generated-doc freshness validation;
- authority grant/revoke UX;
- tự tạo `.pulse/config.yaml` trước khi operational schema có owner;
- tự động edit `.gitignore`;
- brownfield semantic documentation migration;
- daemon/project/workspace/provider enrollment;
- enroll Pulse development repository thành Pulse target.
