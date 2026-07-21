# Phase 1 — Slice 1: Storage Primitive + Minimal Graph Store

> Trạng thái: **proposal để review**, chưa phải work contract.
> Sở hữu: slice implementation strategy cho Phase 1 phần storage/graph.
> Tham chiếu: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`02-work-graph.md`](../pulse-reboot/02-work-graph.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md).

## Nguyên tắc

- Pulse là harness **mới**, viết fresh bằng Rust. `references/**` là **reference-only**: học pattern thiết kế, không port code, không kế thừa contract.
- Slice này đập thẳng vào **#1 rủi ro kỹ thuật** (D-21, Delivery risks): multi-process CAS/locking trên JSON. Chứng minh nó deterministic + recoverable trước khi đụng phần cơ học hơn (docs/retrieval).
- Mọi quyết định triển khai phải trace ngược về một decision đã Accept hoặc một acceptance scenario trong roadmap.

## Mục tiêu (Goal)

Triển khai **kernel storage layer + graph store tối thiểu** để một Ticket standalone có thể tạo, link, query, validate và export deterministic, với mutation protocol CAS an toàn dưới concurrent writer và crash.

### Thỏa mãn

- **Phase 1 exit** (từ roadmap): node/edge JSON Schemas + sharded layout; create/show/list với revision CAS, deterministic edge IDs, atomic rename; lifecycle inverse projections; dependency cycles; readiness; `graph validate`, `graph neighborhood`, `graph export` + disposable fingerprinted cache; immutable semantic event files.
- **Acceptance scenarios** (Core v1, hiện có 65 scenarios): `#2` standalone Ticket, `#3` Epic→Story→Tickets thành file riêng + inverse roll-up, `#5` CAS conflict rõ + hai node khác nhau không shared-file conflict, `#6` edge retry idempotent + dangling/cyclic reject, `#7` xóa cache rebuild cùng fingerprint/semantics.
- **Decisions**: D-06 (tách lớp), D-18 (sharded JSON), D-19 (full graph = derived projection), D-20 (CLI là mutation surface), D-21 (không SQLite, files+locks+atomic replace).
- **Prototype question**: Q1 (repository-scoped lock, atomic rename, crash recovery).

### Không thuộc slice này (Non-goals)

- Documentation system + retrieval (comrak/tantivy) — slice sau.
- Single-agent run, assignment lease, handoff, close gate (Phase 2).
- `work packet`, ready gate đầy đủ, verification/QA/evidence store.
- Skills/scripts/hooks/capability packs (Phase 3).
- Lifecycle transitions đầy đủ (`shaped`/`active`/`verifying`/`done`/`rework`/`blocked`/`superseded`); slice này chỉ cần trạng thái tối thiểu để test graph mechanics, không phải run lifecycle.

## Crate & file layout

Single Cargo package `pulse` với `[lib]` + `[[bin]]` (workspace-ready, chưa tách crate — tách khi capability packs đến Phase 3).

```text
Cargo.toml                      # [package] pulse; [lib] + [[bin]] name="pulse"
src/
  lib.rs                        # re-export API công khai
  error.rs                      # PulseError (thiserror): CasConflict, InvalidGraph, DanglingEdge, CycleDetected, Io, Schema
  id.rs                         # ulid cho event; node-id derive max+1 per kind
  fence.rs                      # WriteGuard (RAII, flock exclusive, Drop unlock)
  storage/
    mod.rs                      # GraphStore trait + JsonGraphStore (orchestrate fence+atomic+validate+event)
    atomic.rs                   # canonical_write(temp->fsync->rename->dir fsync)
    paths.rs                    # resolve .pulse layout + manifest
  graph/
    mod.rs
    node.rs                     # Node, Kind, Status (subset), minimal fields
    edge.rs                     # Edge, EdgeType, deterministic id, canonicalization, rules
    manifest.rs                 # manifest.json contract (schema_version, id_pattern, content_root)
    validate.rs                 # schema (boon) + referential integrity + cycle detection
    projection.rs               # graph export: sorted nodes/edges + inverse indexes + fingerprint
  event.rs                      # immutable event file write (.pulse/events/<date>/evt_<ulid>.json)
  schema/
    node.schema.json            # JSON Schema draft 2020-12 (embedded via include_str!)
    edge.schema.json
src/bin/pulse.rs                # clap (derive), thin — gọi lib, không chứa logic
tests/
  concurrency.rs                # acceptance #5, #6
  recovery.rs                   # acceptance #7 + crash
  projection.rs                 # deterministic export + fingerprint
fixtures/
  minimal/                      # fixture repo cho integration test
```

Quy tắc: `src/bin/pulse.rs` chỉ parse args + gọi `lib` + format output. Toàn bộ logic nằm trong `lib`. Đây là D-20 ("CLI là interface chính; library API ở dưới").

## Storage layout (target repository)

```text
.pulse/
  workgraph/
    manifest.json               # tracked canonical contract (hiếm đổi)
    schemas/                    # optional copy; source-of-truth là embedded trong binary
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
    .writer.lock               # runtime coordination, gitignored (flock, không tracked)
  events/
    2026-07-20/
      evt_01J....json
  cache/                        # gitignored, disposable
    workgraph.snapshot.json
```

Git ownership: `manifest.json`, `schemas/`, `nodes/`, `edges/`, `events/` = tracked. `.writer.lock`, `cache/` = gitignored. `.gitignore` của Pulse phải thêm `.pulse/workgraph/.writer.lock` và `.pulse/cache/`.

## Schema (draft 2020-12)

### Node (subset cho slice này)

```jsonc
{
  "schema_version": 1,
  "id": "TK-031",
  "kind": "ticket",                 // enum: epic | story | ticket | decision
  "revision": 7,
  "title": "Phân loại lỗi refresh token",
  "status": "draft",                // slice này dùng subset: draft | ready | cancelled
  "risk": "medium",                 // optional slice này; default "low"
  "content_dir": "works/TK-031",    // optional slice này
  "created_at": "2026-07-20T01:00:00Z",
  "updated_at": "2026-07-20T02:00:00Z"
}
```

Normative tối thiểu: `schema_version`, `id`, `kind`, `revision`, `title`, `status`, timestamps. `id` match `^(EP|ST|TK|DEC)-[0-9]{3,}$` (từ manifest `id_pattern`). `revision` integer ≥ 1. Kind-specific fields (priority, risk, materialization, verification_profile...) **không** ép trong slice này — thêm theo slice sau, mỗi cái có schema evolution.

### Edge

```jsonc
{
  "schema_version": 1,
  "id": "blocked-by--TK-031--TK-029",   // deterministic: {type}--{from}--{to}
  "type": "blocked_by",                  // parent | blocked_by | preferred_after | superseded_by | related | duplicates
  "from": "TK-031",
  "to": "TK-029",
  "revision": 1,
  "created_at": "2026-07-20T01:30:00Z",
  "created_by": "human:quannv"
}
```

Quy tắc (từ `02-work-graph.md`):

- Edge ID derive deterministic từ `(type, from, to)` → retry `edge add` idempotent.
- Một node tối đa một live `parent` edge.
- `parent`, `blocked_by`, `superseded_by` không tạo cycle (mỗi loại có DFS riêng).
- `related` canonicalize theo thứ tự ID (min, max) để hai caller không tạo hai edge ngược nhau.
- Dangling edge (from/to không tồn tại) → `graph validate` reject.
- Reverse edges (`children`, `blocks`, `preferred_before`, `supersedes`, `has_duplicate`) = **projection only**, không persist lần hai.

## Storage primitive (fence + atomic) — fresh implementation

> **Reference-only**: pattern RAII-guard + exclusive lock + validate-before-atomic-rename học từ lý thuyết concurrency và các reference repo. Pulse tự viết, tự test, không kế thừa code hay contract.

### WriteGuard

```rust
pub struct WriteGuard { lock: File }   // exclusive flock tại .pulse/workgraph/.writer.lock

impl WriteGuard {
    pub fn acquire(repo_root: &Path) -> Result<Self, PulseError>;   // create dir + open + lock_exclusive (block)
}
impl Drop for WriteGuard {
    fn drop(&mut self) { let _ = self.lock.unlock(); }              // release kể cả khi panic
}
```

Lý do chọn `flock` (qua `fs2`): lock được kernel giải phóng tự động khi process chết → **không có stale-lock problem**, không cần PID file hay TTL detection. Mọi mutation graph phải giữ guard này suốt đời lệnh. (Nuance Windows: flock advisory semantics khác — flag Q1-windows ở Risks.)

### Atomic canonical write

```text
serialize(node)  -> BTreeMap (sorted keys, no floats)  -> pretty JSON
write to <path>.tmp.<pid>
fsync(<path>.tmp)
rename(<path>.tmp, <path>)            // atomic trên cùng filesystem
fsync(parent dir)
on any error: unlink(<path>.tmp)
```

Deterministic serialization: `serde_json` mặc định (không bật `preserve_order`) dùng `BTreeMap` → key sorted. Không dùng float trong canonical data. Kết quả: cùng node ghi 2 lần → byte-identical file (T8).

## CAS mutation protocol

```text
1. guard = WriteGuard::acquire(repo_root)
2. node = read_node(id); current_rev = node.revision
3. if expected_revision != current_rev -> Err(CasConflict { id, expected, current })
4. validate: schema(boon) + referential(edge from/to tồn tại) + cycle(rules per type)
5. new_node = node.with_revision(current_rev + 1).touch_updated_at()
6. atomic_canonical_write(nodes/<id>.json, new_node)
7. event = build_event(type, subject{id,rev=new}, actor, occurred_at)
8. write_event_file(events/<date>/evt_<ulid>.json)   // immutable
9. drop(guard)                                        // release lock
```

Revision bump chỉ sau khi atomic write thành công. Event file ghi sau node write, trong cùng guard: nếu crash giữa #6 và #8, graph vẫn valid (node đã ghi), event mất nhưng không corrupt canonical truth — event là audit, không phải source-of-work-state (D-06).

Node ID derivation (tạo mới): dưới guard, scan `nodes/<prefix>-*.json`, lấy max number, new = max+1. **Không** lưu counter trong manifest (chống merge hotspot). Hai create cùng kind serialize qua guard → không trùng ID.

## CLI surface (slice này)

```text
pulse init                           # tạo .pulse/workgraph/{manifest,schemas,nodes,edges} tối thiểu, không overwrite
pulse work create --kind <k> --title "..." [--json]   # → in ra id mới
pulse work show <id> [--json]
pulse work list [--kind <k>] [--json]
pulse graph edge add --type <t> --from <id> --to <id> [--json]   # idempotent
pulse graph edge remove --type <t> --from <id> --to <id>
pulse graph validate [--json]         # exit non-zero nếu invalid
pulse graph export [--json]           # sorted nodes/edges + inverse indexes + fingerprint
```

Output JSON luôn có `schema_version`. Exit non-zero khi graph invalid (D-20). `graph neighborhood`/`affected-by` defer sang slice kế (cần cho `work packet`).

## Test matrix

| ID | Scenario | Acceptance | Kỳ vọng |
|---|---|---|---|
| T1 | 2 writer cùng node revision (song song) | #5a | 1 thành công, 1 `CasConflict` rõ ràng |
| T2 | 2 writer 2 node khác nhau | #5b | cả 2 thành công, không shared-file conflict |
| T3 | `edge add` retry cùng (type,from,to) | #6a | idempotent, không duplicate, revision không tăng |
| T4 | edge `to` không tồn tại | #6b | reject `DanglingEdge` |
| T5 | cyclic `parent` / `blocked_by` / `superseded_by` | #6c | reject `CycleDetected` |
| T6 | xóa `cache/snapshot`, chạy lại `graph export` | #7 | cùng canonical fingerprint + semantics |
| T7 | crash giữa temp-write và rename | (recovery) | `graph validate` OK, không partial node, temp bị dọn |
| T8 | ghi cùng node 2 lần | (determinism) | byte-identical file |
| T9 | tạo ticket không có story/epic | #2 | hợp lệ |
| T10 | EP→ST→TK thành file riêng | #3 | inverse `children`/roll-up derive đúng |
| T11 | `related` add theo 2 thứ tự | (canonicalize) | 1 edge, không 2 edge ngược |
| T12 | `init` trên repo có sẵn `.pulse` | #1 (subset) | không overwrite file user |

Concurrency test (T1, T2) dùng N thread/process thật tranh guard. Recovery test (T7) mô phỏng bằng cách inject fail point (feature flag `fail-after-temp-before-rename`) hoặc kill process thật.

## Dependencies (Cargo.toml)

```toml
[dependencies]
clap        = { version = "4", features = ["derive"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"            # KHÔNG bật preserve_order → BTreeMap → sorted keys (deterministic)
boon        = "0.6"          # JSON Schema draft 2020-12 (cần verify maturity — xem Risks)
sha2        = "0.10"
ulid        = "1"
chrono      = { version = "0.4", features = ["serde"] }
fs2         = "0.4"          # flock (FileExt) — hoặc fd-lock, xem Risks
thiserror   = "1"
anyhow      = "1"
tracing     = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tempfile    = "3"
assert_cmd  = "2"            # CLI integration test
predicates  = "3"
```

**Không có** `tantivy`, `comrak`, `tokio` trong slice này (defer docs/retrieval + async process runner). Slice này toàn sync file I/O — đúng trọng tâm storage.

## Definition of Done (slice này)

- [ ] `cargo build` + `cargo clippy -- -D warnings` + `cargo fmt --check` sạch.
- [ ] `pulse init` bootstrap fixture repo không phá file user (T12).
- [ ] Node/edge JSON Schema validate mọi fixture (boon).
- [ ] Tạo standalone Ticket hợp lệ không cần Story/Epic (T9).
- [ ] EP→ST→TK thành file riêng, inverse `children` derive đúng (T10).
- [ ] CAS: cùng revision → 1 OK 1 reject rõ; khác node → cả OK (T1, T2).
- [ ] Edge retry idempotent; dangling/cyclic reject (T3, T4, T5, T11).
- [ ] `graph export` deterministic: xóa cache rebuild cùng fingerprint (T6).
- [ ] Crash giữa temp/rename không corrupt graph, temp được dọn (T7).
- [ ] Cùng node ghi 2 lần → byte-identical (T8).
- [ ] Immutable event file ghi sau mỗi mutation thành công.
- [ ] CLI output có `schema_version`; exit non-zero khi graph invalid.
- [ ] `.gitignore` Pulse thêm `.pulse/workgraph/.writer.lock` + `.pulse/cache/`.

Exit slice = D-21/D-18/D-19/D-20 chứng minh chạy deterministic + recoverable; nền cho docs/retrieval slice kế tiếp.

## Risks & open questions (cho review)

1. **Cross-platform atomic rename** — `std::fs::rename` atomic trên cùng filesystem ở Unix; Windows cần `REPLACE_EXISTING`. **Quyết định đề xuất**: yêu cầu `.pulse` nằm cùng filesystem local (không networked drive). Ghi vào `PULSE.md` policy + `pulse doctor` check.
2. **Windows flock semantics** — flock advisory khác nhau cross-platform. `fs2` trừu tượng hóa nhưng Windows không có flock POSIX đúng nghĩa. Cần integration test Windows hoặc chốt "Core v1 target macOS/Linux, Windows best-effort".
3. **boon vs Ajv equivalence** — boon hỗ trợ draft 2020-12 nhưng ecosystem trẻ hơn Ajv. Slice này dùng feature schema tối thiểu (type/enum/pattern/required) → an toàn. Nếu cần feature phức tạp sau, đánh giá lại crate (`jsonschema` crate là lựa chọn thay thế).
4. **Canonical float** — tránh hoàn toàn float trong canonical data (revision int, timestamps ISO8601 string). Nếu sau cần số đo, tách sang non-canonical sidecar.
5. **Event file naming & collision** — ULID (48-bit ms timestamp + 80-bit random) collision-resistant đủ cho single-repo. Không cần centralized counter.
6. **Lock fairness/starvation** — flock mặc định không guarantee FIFO; nhiều writer có thể starve một writer. Core v1 single-agent为主, chấp nhận được. Đo lại ở Phase 5 (orchestration).
7. **`related` canonicalization** — cần define rõ "ID nhỏ hơn" khi prefix khác (VD `DEC-006` vs `TK-031`: so sánh lex string hay theo (kind, number)?). **Đề xuất**: so sánh theo string thuần của id, document rõ.
8. **Fixture repo cho integration test** — tạo `fixtures/minimal/` khởi đầu, dùng cho toàn Phase 1 sau.

## Không quyết định thêm trong slice này

Slice này **không** chốt: lifecycle đầy đủ, ready-gate contract, work packet shape, docs impact, evidence schema. Mỗi cái là slice/Ticket riêng với schema evolution rõ ràng. Mục tiêu duy nhất: **storage + graph mechanics trustworthy**.
