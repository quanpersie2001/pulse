# Phase 1 — Slice 1: Transactional Storage Primitive + Minimal Work Graph

> Trạng thái: **proposal để review**, chưa phải work contract hay compatibility contract.
> Sở hữu: implementation strategy cho lát cắt đầu tiên của Phase 1: storage primitive và work-graph mechanics tối thiểu.
> Tham chiếu normative: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`02-work-graph.md`](../pulse-reboot/02-work-graph.md), [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md), [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md), [`12-knowledge-compounding.md`](../pulse-reboot/12-knowledge-compounding.md).

## Vị trí của slice trong Pulse Reboot

Phase 1 hiện không chỉ có graph store. Nó còn bao gồm documentation foundations, receipt identity, shaping/readiness projections, docs retrieval và knowledge-store foundations. Vì vậy slice này **không tuyên bố hoàn thành Phase 1**.

Slice 1 chỉ tạo nền cơ học dùng lại được cho các phần sau:

1. canonical JSON read/write có schema và deterministic bytes;
2. repository-scoped write fence, expected-revision CAS và crash recovery;
3. sharded work-graph node/edge store;
4. deterministic graph projection/fingerprint/cache;
5. immutable semantic event emission với recovery rõ ràng.

Các slice Phase 1 tiếp theo sẽ dùng primitive này cho lifecycle/readiness, receipts, documentation registry/retrieval, shaping projections và knowledge records. Storage primitive phải đủ generic để reuse, nhưng implementation trong slice này chỉ expose **work graph adapter**; không tạo một generic database framework trước khi có usage thật.

## Nguyên tắc

- Pulse core là implementation mới bằng Rust stable theo D-22. `references/**` chỉ cung cấp bài học; không port code và không kế thừa contract.
- Repository là system of record. Canonical graph là independent files dưới `.pulse/workgraph/`; top-level `works/` giữ human-facing work prose.
- Deterministic mechanism thuộc kernel/library; CLI chỉ parse input, gọi library và render stable output.
- Normal Agent workflow đọc/mutate graph qua CLI/API. Raw files tồn tại để Git và human inspect/recover, không phải query surface cho Agent.
- Runtime lock, transaction intent và temp state không được trộn vào tracked canonical graph plane.
- Cache là disposable; event là audit evidence; node/edge files mới là canonical work-state truth.
- Slice phải chứng minh failure semantics, không chỉ happy path. Không được gọi một write protocol là “atomic transaction” nếu crash giữa canonical write và event có thể để lại trạng thái không reconcile được.
- Mọi acceptance của slice phải trace về decision, roadmap scenario hoặc prototype question hiện hành. Các chi tiết chưa được owner document khóa phải được ghi là proposal/open question, không giả thành compatibility contract.

## Mục tiêu

Triển khai storage layer và graph store tối thiểu để có thể:

- bootstrap fixture/layout qua library primitive mà không phá file user;
- tạo, xem, liệt kê và CAS-edit Epic/Story/Ticket/Decision;
- tạo typed edge với deterministic identity;
- validate schema, references và cycle rules;
- derive inverse relations và export graph deterministic;
- recover sau process crash ở mọi fail point đã định nghĩa;
- ghi immutable semantic event cho mutation đã commit;
- xóa cache rồi rebuild mà không đổi canonical graph fingerprint hoặc projection semantics.

Slice này ưu tiên trực tiếp prototype question Q1 trong `09-decisions-and-dod.md`: repository-scoped lock, atomic replace và crash recovery trên các platform mục tiêu.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

- **#2:** standalone Ticket hợp lệ, không cần Story/Epic.
- **#3, graph subset:** Epic → Story → Tickets là independent node/edge files; inverse parent/children derive đúng.
- **#5:** hai writer cùng expected revision chỉ một mutation thắng; hai writer trên hai nodes khác nhau đều thành công và không sửa chung canonical file.
- **#6:** retry cùng edge không tạo duplicate; dangling/cyclic edge bị reject.
- **#7:** xóa graph cache rồi export lại cho cùng fingerprint và equivalent projection.

### Decisions liên quan

- D-02, D-06, D-07.
- D-18 đến D-22.
- D-25 cho boundary giữa `.pulse/workgraph/` và top-level `works/`.

### Slice exit

Slice hoàn thành khi storage/graph mechanics deterministic và recoverable. Nó **không** thỏa Phase 1 exit cho tới khi các phần còn lại của roadmap Phase 1 — docs, receipts, shaping/readiness, retrieval và knowledge foundations — cũng hoàn thành.

## Non-goals

- Full lifecycle transition/gate: `shaped`, `ready`, `active`, `verifying`, `done`, `rework`, `blocked`, `superseded`.
- Semantic ready gate, branch disposition, shaping receipt, decision/execution frontier.
- `pulse work packet`, applicable docs, QA impact, documentation impact và close gate.
- Documentation registry, `_index.md`, comrak section extraction hoặc Tantivy BM25 index.
- Receipt/evidence artifact store ngoài mutation transaction intent tối thiểu.
- Knowledge entry/relation store và applicability-aware recall.
- Assignment lease, run state, handoff, worktree hoặc peer-agent orchestration.
- Public `pulse init` và `pulse doctor`; roadmap hiện đặt repository bootstrap/doctor capability hoàn chỉnh ở Phase 3. Slice chỉ cung cấp library bootstrap primitive và fixture helper để Phase 3 gọi lại.
- Skills/scripts/hooks/capability packs.
- Edge removal/tombstone semantics; slice đầu chỉ add/query/validate edge, còn remove được thêm khi delete recovery contract đã khóa.
- Benchmark-driven cache optimization hoặc daemon.

Schema của slice có thể nhận diện lifecycle values đã được reboot định nghĩa để tránh tạo enum tạm mâu thuẫn, nhưng slice không expose transition command hay tuyên bố enforce lifecycle semantics đầy đủ.

## Repository layout

```text
works/                              # tracked human-facing work content root
  EP-001/                           # chỉ tồn tại sau khi content được materialize
  ST-014/
  TK-031/
  DEC-006/

.pulse/
  workgraph/                        # tracked canonical graph truth
    manifest.json
    schemas/
      node.schema.json
      edge.schema.json
    nodes/
      EP-001.json
      ST-014.json
      TK-031.json
      DEC-006.json
    edges/
      parent--ST-014--EP-001.json
      blocked-by--TK-031--TK-029.json

  events/                           # tracked immutable semantic audit files
    2026-07-20/
      evt_01J....json

  runtime/                          # local coordination, gitignored
    locks/
      workgraph.lock
    transactions/
      txn_01J....json

  cache/                            # gitignored, disposable
    workgraph.snapshot.json
```

Ownership:

- `manifest.json`, `schemas/`, `nodes/`, `edges/`, `events/` và materialized files dưới `works/` là tracked. Git không track empty directories.
- `.pulse/runtime/` và `.pulse/cache/` là gitignored.
- Lock không nằm trong `.pulse/workgraph/`, vì runtime coordination không được giả thành canonical work state.
- Schema files dưới `.pulse/workgraph/schemas/` là canonical schema được manifest reference. Binary có thể embed bootstrap templates cùng version, nhưng embedded copy không được silently thắng schema đã được repository quản lý.
- Bootstrap primitive trả proposed ignore entries hoặc áp dụng qua explicit caller policy; public `pulse init` ở Phase 3 mới sở hữu UX/config integration hoàn chỉnh và không rewrite `.gitignore` tùy tiện.

## Manifest contract

```json
{
  "schema_version": 1,
  "graph_id": "pulse-main",
  "node_schema": "schemas/node.schema.json",
  "edge_schema": "schemas/edge.schema.json",
  "content_root": "../../works",
  "id_pattern": "^(EP|ST|TK|DEC)-[0-9]{3,}$"
}
```

Rules:

- Manifest chỉ chứa contract hiếm thay đổi; không chứa node list, counters hoặc mutable graph revision.
- Full graph revision là fingerprint derive từ sorted canonical node/edge content hashes.
- `content_root` phải resolve trong repository và chống path traversal/symlink escape.
- Init không overwrite manifest/schema hiện có. Version mismatch phải trả finding/error rõ thay vì tự migration.

## Node schema tối thiểu

```jsonc
{
  "schema_version": 1,
  "id": "TK-031",
  "kind": "ticket",
  "revision": 7,
  "title": "Phân loại lỗi refresh token",
  "status": "draft",
  "content_dir": "works/TK-031",
  "created_at": "2026-07-20T01:00:00Z",
  "updated_at": "2026-07-20T02:00:00Z"
}
```

Normative common fields trong slice:

- `schema_version`;
- `id` và `kind` khớp nhau;
- `revision` integer `>= 1`;
- non-empty `title`;
- `status` thuộc lifecycle vocabulary hiện hành;
- `content_dir` repository-relative, nằm dưới configured work-content root;
- RFC 3339 timestamps.

Slice chỉ tạo node ở `draft` và chỉ cho CAS-edit các common mutable fields được whitelist, trước mắt là `title`. Nó không cho caller đổi trực tiếp `id`, `kind`, `revision`, timestamps hoặc transition status qua generic JSON patch.

`content_dir` không còn là optional, nhưng một `draft` node có thể reference một safe, chưa materialized path. `pulse work create` không tạo empty directory hay prose giả vì Git không preserve empty directories. Khi capability sau materialize artifact đầu tiên, nó tạo `works/<ID>/`; ready/content gates khi đó mới bắt buộc referenced content tồn tại. `graph validate` của slice kiểm tra path safety và report missing draft content như advisory, không coi directory trống trong working tree là durable state.

Schema có thể cho phép các fields đã được reboot định nghĩa như `priority`, `risk`, `materialization`, `implementation`, `verification_profile`, nhưng slice không cần mutate hoặc validate semantic completeness của chúng ngoài JSON Schema đã khóa.

## Edge schema và rules

```jsonc
{
  "schema_version": 1,
  "id": "blocked-by--TK-031--TK-029",
  "type": "blocked_by",
  "from": "TK-031",
  "to": "TK-029",
  "revision": 1,
  "created_at": "2026-07-20T01:30:00Z",
  "created_by": "human:quannv"
}
```

Supported edge types:

- `parent`;
- `blocked_by`;
- `preferred_after`;
- `superseded_by`;
- `related`;
- `duplicates`.

Identity rules:

- Edge ID derive deterministic từ normalized `(type, from, to)`.
- Filename-safe type slug dùng hyphen, ví dụ `blocked_by` → `blocked-by`.
- `related` canonicalize endpoints bằng lexical comparison trên canonical work IDs; caller order không tạo hai edges.
- Retry add một edge đã tồn tại với cùng semantic payload trả `unchanged`, không rewrite file, không bump revision và không emit duplicate mutation event.
- Nếu deterministic ID đã tồn tại nhưng content không khớp canonical tuple, validate fail hard.

Graph rules:

- Endpoint phải tồn tại.
- Một node có tối đa một live `parent` edge.
- `parent`, `blocked_by` và `superseded_by` không tạo cycle theo relation-specific traversal.
- Reverse relations (`children`, `blocks`, `preferred_before`, `supersedes`, `has_duplicate`) chỉ là projection, không persist.
- `related` được project thành symmetric adjacency: query/export từ bất kỳ endpoint nào đều trả endpoint còn lại dù canonical edge chỉ lưu một hướng.
- Slice không tự suy ra dependency từ hierarchy hoặc priority.

## Crate và module layout đề xuất

Bắt đầu bằng một Cargo package `pulse` có library + thin binary. Chỉ tách workspace crates sau khi có hai usage boundary thật, tránh abstraction sớm.

```text
Cargo.toml
src/
  lib.rs
  error.rs
  id.rs
  canonical_json.rs               # recursive key ordering + stable bytes/hash
  storage/
    mod.rs                        # storage primitives, không chứa CLI formatting
    lock.rs                       # repository-scoped RAII write fence
    atomic.rs                     # same-filesystem temp + fsync/replace strategy
    transaction.rs                # prepared intent + recovery state machine
    paths.rs                      # safe repository-relative path resolution
  graph/
    mod.rs
    store.rs                      # JsonGraphStore adapter
    manifest.rs
    node.rs
    edge.rs
    validate.rs
    projection.rs
  event.rs                        # event envelope + immutable event write
  schema/
    node.schema.json              # bootstrap template, versioned with binary
    edge.schema.json
src/bin/pulse.rs                  # clap parse -> library call -> stable output

tests/
  init.rs
  schema.rs
  cas_concurrency.rs
  edge_rules.rs
  crash_recovery.rs
  projection.rs
  cli_contract.rs
fixtures/
  minimal/
```

`src/bin/pulse.rs` không được tự đọc files, lock, validate graph hoặc tính fingerprint. Library APIs trả typed outcome/error; renderer quyết định human/JSON representation.

## Canonical JSON và fingerprint

Không dựa vào incidental field order của Rust structs hoặc `serde_json::Map`.

Canonical serializer phải:

1. parse/construct JSON value;
2. recursively sort object keys;
3. preserve array order;
4. reject non-finite numbers và tránh float trong canonical schemas của slice;
5. emit UTF-8 pretty JSON với line ending `\n` ổn định;
6. append đúng một trailing newline;
7. hash exact canonical bytes.

Graph fingerprint derive từ:

```text
schema/fingerprint version
+ manifest contract hash
+ sorted list(node path, node content hash)
+ sorted list(edge path, edge content hash)
```

Fingerprint không include cache, runtime lock, pending transaction hoặc event files. Cùng canonical graph phải cho cùng fingerprint bất kể directory iteration order hoặc cache state.

Test “deterministic write” serialize **cùng semantic value** hai lần và so bytes/hash. Không gọi hai successful mutations là byte-identical vì mutation hợp lệ có revision/timestamp mới.

## Write fence

Proposal mặc định dùng một repository-scoped workgraph lock:

```text
.pulse/runtime/locks/workgraph.lock
```

```rust
pub struct WriteGuard { /* platform-specific held file lock */ }

impl WriteGuard {
    pub fn acquire(repo_root: &Path) -> Result<Self, PulseError>;
}
```

Requirements:

- RAII release khi scope kết thúc hoặc process chết.
- Mọi workgraph mutation và recovery giữ cùng fence trong toàn bộ critical section.
- Readers không đọc temp/pending files; chúng chỉ đọc last complete canonical files.
- Hai mutations trên hai node khác nhau có thể serialize qua fence ở v1 nhưng không được sửa một shared canonical graph file. Sharding giải quyết Git/write hotspot; fine-grained lock chỉ thêm sau benchmark.
- Lock timeout/cancellation phải có structured error; không treo vô hạn trong CI.
- Lock implementation (`fs2`, `fd-lock`, OS API hoặc equivalent) chỉ được chốt sau Q1 integration tests trên supported platforms. Proposal không coi POSIX `flock` semantics là portable proof.

## Atomic replace primitive

Mỗi canonical file write:

```text
canonical bytes
  -> create unique temp in same directory/filesystem
  -> write_all
  -> flush + fsync temp where supported
  -> platform-correct atomic replace target
  -> fsync parent directory where supported
  -> cleanup orphan temp on failure/recovery
```

Rules:

- Temp phải cùng filesystem với target.
- Existing target replacement phải được test riêng trên Windows và Unix; `std::fs::rename` không được assume có identical overwrite semantics trên mọi platform.
- Unsupported durability guarantee phải được trả bằng structured capability/error từ bootstrap hoặc mutation command; Phase 3 `pulse doctor` sẽ aggregate signal này, không phải slice này.
- Network filesystem hoặc path không đáp ứng atomic-replace contract phải fail/advisory theo platform policy được khóa sau prototype.
- Atomic replace bảo vệ **một file**. Multi-file mutation + event cần transaction protocol riêng ở mục tiếp theo.

## CAS mutation protocol

Node edit minh họa:

```text
1. acquire WriteGuard
2. recover hoặc refuse unresolved prior transaction
3. load manifest + target + affected graph
4. compare expected_revision với current revision
5. apply whitelisted mutation in memory
6. set revision = current + 1 và updated_at từ một operation context duy nhất
7. validate schema + graph invariants
8. build canonical target bytes, hashes, event ID và event payload
9. persist prepared transaction intent
10. atomic replace canonical target
11. create immutable event file
12. mark transaction complete, rồi cleanup runtime intent
13. release guard
```

Create dùng cùng protocol với `before = absent` và `after = expected hash`. Edge add trả một trong `created | unchanged`; `unchanged` không tạo transaction/event mới. Delete/edge-remove không thuộc slice vì cần existence-aware delete recovery và history semantics riêng.

CAS conflict JSON phải chứa ít nhất:

```json
{
  "code": "cas_conflict",
  "subject": "TK-031",
  "expected_revision": 6,
  "current_revision": 7
}
```

Không auto-retry semantic mutation trên stale state. Caller phải reload và quyết định lại.

## Crash recovery và event consistency

Canonical graph và immutable events nằm ở hai files khác nhau nên không thể giả định một filesystem rename tạo atomicity cho cả hai. Slice phải dùng prepared transaction intent hoặc một cơ chế tương đương đã được test.

Prepared intent tối thiểu lưu dưới `.pulse/runtime/transactions/`:

- transaction/event ID;
- operation kind và actor;
- target path;
- before state: `absent` hoặc `{hash, revision}`;
- after state: `{hash, revision}` cho create/update trong slice;
- canonical event payload hoặc đủ dữ liệu deterministic để rebuild;
- state/version và timestamps.

Khi process mới acquire fence, recovery đối chiếu intent với target/event. `before` có thể là `absent` cho create hoặc một hash cho update:

| Canonical target | Event | Recovery |
|---|---|---|
| khớp before state | absent | mutation chưa commit; cleanup temp/intent |
| khớp after hash | absent | write immutable event từ prepared intent, rồi complete |
| khớp after hash | đúng event hash | cleanup intent; mutation đã complete |
| khớp before state | event tồn tại | invalid partial state; stop với actionable recovery error |
| không khớp before/after hoặc event mismatch | bất kỳ | không tự đoán; stop, preserve evidence, yêu cầu inspect |

Event file creation là create-new, không overwrite. Retry với cùng event ID + cùng hash là idempotent; cùng ID khác content là corruption.

Crash model của candidate protocol là **process/host interruption trong khi repository-local runtime control directory vẫn được preserve**. `.pulse/runtime/` vẫn là local recovery state, không phải canonical work truth. Nếu directory này bị xóa/copy thiếu sau canonical replace nhưng trước event, exact audit completeness không còn được chứng minh từ node/edge files hiện tại. Vì vậy Q1 phải prototype và khóa một durable commit-marker strategy hoặc chấp nhận/document giới hạn crash model trước khi proposal thành work contract. Slice không được claim unconditional “mọi committed mutation luôn có event” ngoài model đã chứng minh.

## Node ID allocation

Slice đề xuất numeric IDs tương thích manifest hiện tại:

```text
EP-001, ST-014, TK-031, DEC-006
```

Create dưới write fence scan canonical filenames cùng prefix và chọn `max + 1`; manifest không giữ mutable counter để tránh merge hotspot.

Constraints:

- Normal mutation không hard-delete historical nodes; cancellation/supersession giữ identity, nên ID không được reuse.
- Manual deletion hoặc branch merge có thể tạo collision; `graph validate` phải detect duplicate identity/path/content mismatch và không tự renumber.
- Phase 5 Workers không trực tiếp allocate canonical graph IDs trong isolated worktree; control workspace/kernel apply mutation.
- Nếu benchmark hoặc Git workflow thực tế chứng minh numeric max-scan không đủ, thay đổi allocator cần Decision/schema migration riêng.

## CLI surface của slice

```text
# test/dev bootstrap helper gọi library primitive; public `pulse init` defer Phase 3

pulse work create --kind <epic|story|ticket|decision> --title "..." [--json]
pulse work show <id> [--json]
pulse work list [--kind <kind>] [--json]
pulse work edit <id> --expected-revision <n> --title "..." [--json]

pulse graph edge add --type <type> --from <id> --to <id> --actor <actor> [--json]
pulse graph validate [--json]
pulse graph export [--json]
```

Deferred khỏi slice:

```text
pulse work ready
pulse work packet
pulse work frontier
pulse graph neighborhood
pulse graph affected-by
pulse work transition/close
pulse graph edge remove
pulse init
pulse doctor
```

Output rules:

- Machine output luôn có `schema_version` và stable `code` cho errors.
- Invalid graph, schema mismatch, CAS conflict, unresolved transaction hoặc unsupported durability guarantee trả non-zero.
- Human output có thể thân thiện nhưng không được là input contract cho adapters.
- `graph export` trong slice chỉ cam kết nodes, edges, inverse indexes và graph fingerprint. Derived readiness/frontiers được thêm bởi slice sở hữu lifecycle/shaping, với output schema evolution rõ ràng.

## Projection và cache

`pulse graph export --json` build read model từ canonical files:

```jsonc
{
  "schema_version": 1,
  "graph_fingerprint": "sha256:...",
  "nodes": [],
  "edges": [],
  "inverse": {
    "children": {},
    "blocks": {},
    "preferred_before": {},
    "supersedes": {},
    "has_duplicate": {},
    "related": {}
  }
}
```

Rules:

- Nodes/edges và every ID list có deterministic sort order.
- Export validate graph trước khi trả success.
- Cache `.pulse/cache/workgraph.snapshot.json` được key bằng graph fingerprint + projection schema version.
- Missing/stale/corrupt cache bị bỏ và rebuild; cache không được repair ngược canonical files.
- Atomic cache replacement reuse storage primitive nhưng không emit semantic work event.
- Xóa cache không làm thay đổi output semantics ngoài non-contract timing/diagnostic fields.

## Validation layers

`pulse graph validate` chạy ít nhất:

1. manifest/schema path validation;
2. JSON parse + schema validation;
3. filename ↔ object ID consistency;
4. ID prefix ↔ kind consistency;
5. revision/timestamp/content path checks;
6. duplicate identity và deterministic edge-ID checks;
7. referential integrity;
8. one-parent rule;
9. relation-specific cycle detection;
10. canonical-byte drift advisory hoặc strict failure theo mode;
11. unresolved transaction/orphan temp/event mismatch checks trong local runtime nếu tồn tại;
12. cache ignored for canonical correctness.

Validation không tự sửa semantic graph. Một `recover` internal path chỉ hoàn tất transaction có outcome deterministic theo table ở trên; mọi ambiguous state phải stop.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| T1 | Bootstrap fixture repo trống qua library helper | prerequisite | Tạo layout/schema và proposed ignores, không overwrite file user |
| T2 | Tạo standalone Ticket | #2 | Node hợp lệ với safe `content_dir`, không cần parent; missing draft content chỉ advisory |
| T3 | EP → ST → TK independent files | #3 subset | Forward + inverse projection đúng |
| T4 | Hai process edit cùng node/revision | #5 | Một success, một `cas_conflict` rõ |
| T5 | Hai process edit hai nodes | #5 | Cả hai success; có thể serialize nhưng không shared canonical file conflict |
| T6 | Retry edge add cùng tuple | #6 | `unchanged`, một edge file, không duplicate event |
| T7 | Dangling edge | #6 | Reject trước canonical commit |
| T8 | Cycle `parent`/`blocked_by`/`superseded_by` | #6 | Reject với relation/path cycle detail |
| T9 | `related` add ở hai endpoint orders | #6 extension | Một canonical edge; symmetric adjacency query đúng từ cả hai nodes |
| T10 | Xóa cache và rebuild export | #7 | Cùng fingerprint và equivalent projection |
| T11 | Serialize cùng semantic value hai lần | determinism | Byte/hash identical |
| T12 | Crash trước canonical replace | Q1 | Target giữ before state; recovery cleanup intent/temp |
| T13 | Crash sau canonical replace, trước event | Q1 | Recovery hoàn tất đúng event, không mutate node lần hai |
| T14 | Crash sau event, trước intent cleanup | Q1 | Recovery idempotent, không duplicate event |
| T15 | Event ID tồn tại với content khác | integrity | Hard failure, preserve evidence |
| T16 | Corrupt/stale cache | #7 | Discard/rebuild; canonical graph không đổi |
| T17 | Existing manifest/schema/work dir | #1 subset | Init idempotent hoặc conflict rõ, không overwrite |
| T18 | Content path traversal/symlink escape | security | Reject |
| T19 | Existing edge deterministic ID nhưng payload mismatch | integrity | Graph invalid, không auto-repair |
| T20 | Platform replace/lock integration | Q1 | Pass trên supported matrix hoặc produce explicit unsupported result |

Concurrency tests phải dùng process thật, không chỉ threads dùng chung memory. Crash tests cần failpoints ở từng transaction boundary và ít nhất một kill-process integration test để kiểm chứng cleanup không phụ thuộc Rust unwind.

## Dependency direction

Dependency names/versions là implementation choice cần lockfile và spike, không phải product contract. Candidate roles:

- CLI parsing: `clap`.
- Serialization/schema: `serde`, `serde_json`, một Draft 2020-12 validator đã verify trên fixture.
- Hash/ID/time/error: SHA-256, ULID, RFC 3339 time, typed errors.
- File locking: cross-platform crate hoặc OS wrapper chứng minh qua Q1 tests.
- Temp/atomic replace: same-directory temp helper + platform-specific replace implementation.
- Test harness: temp repositories, CLI assertions, process barriers/failpoints.

Không kéo `tantivy`, `comrak`, async runtime, agent SDK hoặc orchestration dependencies vào slice này. Nếu sync filesystem implementation đủ cho acceptance, không thêm `tokio` chỉ vì roadmap tương lai có process runner.

## Definition of Done của slice

- [ ] Rust build, format, clippy và test suite sạch theo repository policy.
- [ ] Library bootstrap helper idempotent, không phá file user và tạo đúng plane boundaries; public `pulse init` được defer đúng roadmap Phase 3.
- [ ] Tracked workgraph schemas là canonical; embedded schemas chỉ bootstrap đúng version.
- [ ] Common node/edge fixtures pass JSON Schema validation.
- [ ] `content_dir` bắt buộc, safe và nằm dưới top-level `works/`; draft path có thể chưa materialize và không dựa vào empty directory.
- [ ] Create/show/list/CAS-edit hoạt động qua thin CLI + library boundary.
- [ ] Standalone Ticket hợp lệ.
- [ ] Deterministic edge identity, retry idempotency, dangling/cycle/one-parent rules pass.
- [ ] Canonical serializer và graph fingerprint deterministic.
- [ ] Cache delete/corrupt/rebuild không ảnh hưởng canonical truth hoặc projection semantics.
- [ ] Repository-scoped lock có timeout/error contract và process-level concurrency tests.
- [ ] Atomic replace pass supported-platform tests hoặc support boundary được ghi rõ.
- [ ] Prepared transaction recovery pass create/update failpoints trước/sau canonical write và event write trong crash model đã khóa.
- [ ] Mỗi committed mutation trong crash model đã chứng minh có đúng một immutable semantic event; no-op retry không tạo event giả. Nếu Q1 không chứng minh durable marker ngoài `.pulse/runtime/`, support boundary phải được ghi rõ thay vì claim unconditional audit completeness.
- [ ] `graph validate` phát hiện schema, filename/ID, reference, cycle, transaction và event-integrity failures thuộc scope.
- [ ] JSON CLI output có `schema_version`, stable error codes và non-zero exits đúng.
- [ ] Không có lifecycle/readiness/docs/knowledge semantics bị hard-code tạm trái với owner documents.

Exit của slice là bằng chứng cho D-18 đến D-22 và Q1, đồng thời cung cấp primitive cho phần còn lại của Phase 1. Exit này không đồng nghĩa Core v1 hoặc Phase 1 đã hoàn thành.

## Handoff sang các slice Phase 1 tiếp theo

Storage/graph APIs phải cho phép thêm mà không phá identity hiện có:

- lifecycle transitions, supersession và ready projection;
- shaping-map references, branch dispositions và decision/execution frontier;
- minimal receipt store/validator;
- documentation registry và applicable-doc projection;
- generated docs index + section-level lexical cache;
- one-learning-per-record knowledge store và typed relations.

Các plane sau có thể reuse canonical JSON, expected-revision CAS, lock, atomic replace, transaction recovery và fingerprint helpers. Chúng không mặc nhiên dùng chung graph schema, graph event types hoặc một untyped `Store<T>` public abstraction.

## Risks và open questions cho review

1. **Supported platform matrix:** Core v1 target chính xác macOS/Linux/Windows hay có tiering? Q1 phải chốt trước khi claim portable crash safety.
2. **Atomic replace durability:** replace-existing và directory fsync khác nhau theo OS/filesystem. Slice cần capability result nào, và Phase 3 doctor sẽ aggregate nó ra sao?
3. **Lock implementation/fairness:** repository-scoped lock đơn giản nhưng serialize writers. Chỉ chuyển fine-grained locking sau benchmark và proof không phá multi-file validation.
4. **Transaction intent durability:** runtime intent có đủ cho crash model mục tiêu không, hay semantic event/commit marker cần một tracked pending namespace? Đây là điểm phải prototype, không được lướt qua.
5. **Schema authority:** repository-owned schemas cần migration/version policy nào khi binary mới mở repo cũ? Slice chỉ fail rõ; migration tự động defer.
6. **Numeric ID allocation:** `max + 1` tránh manifest hotspot nhưng branch-local human creation có thể collision khi merge. Cần usage evidence trước khi đổi ID contract.
7. **Event actor input:** CLI nhận `--actor` trực tiếp ở slice hay resolve từ invocation context/config? Event vẫn phải có actor typed và không dùng display name mơ hồ.
8. **Canonical timestamp source:** operation context cần injectable clock cho tests; clock skew không được ảnh hưởng ID uniqueness hoặc fingerprint determinism ngoài canonical mutation content.
9. **Graph validation cost:** full scan cho mỗi mutation chấp nhận được ở ngưỡng nào? Benchmark Q2 sẽ quyết định incremental indexes; correctness trước optimization.
10. **Edge remove semantics:** hard-delete edge file + immutable event có đủ history hay cần tombstone cho một số relation? Slice defer command này cho tới khi delete recovery và history semantics được khóa.
11. **Canonicalization strictness:** non-canonical nhưng schema-valid hand edit nên warning hay hard fail trong CI? Normal mutation luôn rewrite canonical bytes; recovery không được rewrite unrelated human changes.
12. **Filesystem policy:** symlinks, network drives, case-insensitive paths và repository moves cần fixture coverage tối thiểu nào?

## Không quyết định trong slice này

Slice này không chốt lifecycle gate, work packet shape, docs impact, QA/evidence schema, learning applicability, capability packs hoặc orchestration authority. Nó chỉ chốt và chứng minh nền lưu trữ mà các contract đó có thể dựa vào mà không tạo nguồn sự thật thứ hai hoặc failure mode mơ hồ.
