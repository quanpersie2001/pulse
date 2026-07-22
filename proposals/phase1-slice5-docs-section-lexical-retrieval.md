# Phase 1 — Slice 5: Documentation Section Extraction + Lexical Retrieval

> Trạng thái: **proposal để review**, chưa phải work contract hay compatibility contract.
> Tiền đề: [`phase1-slice4-document-registry-applicability.md`](phase1-slice4-document-registry-applicability.md) đã được implement và cung cấp canonical document identity, lifecycle/authority/owner/scope metadata, Ticket documentation-impact posture, deterministic document-level applicability và registry-aware documentation receipt validation.
> Sở hữu: implementation strategy cho lát cắt Phase 1 tiếp theo: retrieval metadata, heading-aware Markdown section extraction, stable section references, generated documentation navigation, disposable Tantivy BM25 index, bounded `search|get|tree`, incremental rebuild, corruption recovery và retrieval evaluation.
> Tham chiếu normative: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md), [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md), [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md), [`11-documentation-retrieval.md`](../pulse-reboot/11-documentation-retrieval.md).

## Trạng thái đã verify trước proposal này

Repository hiện tại đã có implementation Rust cho Slice 1–4 và toàn bộ `cargo test --all-targets` đang pass.

Slice 4 đã triển khai:

- `.pulse/docs/registry.json` với shared repository identity, registry revision CAS và per-document revision;
- stable document ID, kind, authority, lifecycle, owner, summary, aliases, scope, review policy, generated contract và supersession;
- registry mutations `register|edit|retire|supersede`, immutable events, crash recovery và process-level CAS tests;
- typed Ticket documentation impact/routing metadata và `pulse docs impact`;
- document-level `pulse docs list|show|validate|applicable`;
- exact current document content hashes trong applicability output;
- registry-aware `documentation_validation` receipt verification.

Implementation hiện tại chưa có:

- Markdown parser hoặc section model;
- generated `docs/_index.md`;
- `.pulse/cache/docs-search/` state/index implementation;
- Tantivy/comrak dependencies;
- `pulse docs index|status|search|get|tree`;
- retrieval quality fixtures hoặc metrics.

Vì vậy slice kế tiếp đúng theo Phase 1 roadmap và handoff của Slice 4 là **Documentation Section Extraction + Lexical Retrieval**. Slice này không mở shaping/readiness, không tạo `work packet` đầy đủ và không thêm semantic/vector retrieval.

### Verification update after implementation

Slice 5 now includes a release-mode benchmark harness at
`benches/docs_retrieval.rs` and a three-platform GitHub Actions matrix in
`.github/workflows/rust.yml`.

The reference benchmark uses deterministic corpora of 10, 100 and 1,000
registered Markdown documents. It measures:

- cold full index build;
- warm lexical query against an already-openable immutable generation;
- one-document incremental refresh, including extraction reuse and publication;
- docs-search cache bytes divided by indexed UTF-8 source bytes.

A local macOS arm64 reference run on Rust 1.97.1 produced the following
1,000-document p95 results:

| Metric | Result | Slice target |
|---|---:|---:|
| Warm lexical search | 5.171 ms | <= 100 ms |
| Cold full build | 1,345.397 ms | <= 10,000 ms |
| One-document incremental refresh | 1,355.408 ms | <= 2,000 ms |
| Cache/source ratio | 0.863x | <= 3.0x |

The machine-readable local report is written under
`target/benchmark-evidence/` and remains disposable. CI uploads the Linux
reference report as a workflow artifact. Linux, macOS and Windows each run
format, Clippy, all Rust targets and the benchmark smoke profile. Platform
proof is considered complete only when that matrix passes on the commit under
review; a local run proves only the reported macOS arm64 environment.

## Vị trí của slice trong Pulse Reboot

Slice 4 trả lời:

- document nào tồn tại;
- identity/path/owner/authority/lifecycle của document;
- document nào applicable ở mức document cho một work item;
- exact current file hash là gì.

Nhưng Agent vẫn chưa thể hỏi:

- document có những section nào;
- section nào chứa command, invariant hoặc behavior cần tìm;
- exact line range và section content hash là gì;
- lấy một section bounded mà không đọc full file thế nào;
- cache/index có current với registry và document bytes hay không;
- xóa cache rồi rebuild có giữ retrieval semantics hay không;
- generated navigation cho human/cold Agent được tạo thế nào;
- lexical retrieval có tìm đúng exact identifier, natural-language paraphrase và Vietnamese terms trong budget hay không.

Slice 5 nối document registry với progressive disclosure:

```text
registry + current document bytes
  -> heading-aware section records
  -> generated navigation projection
  -> disposable lexical index
  -> bounded search metadata/snippets
  -> explicit section get
```

Slice tập trung vào sáu capability:

1. deliberate registry schema evolution cho retrieval metadata;
2. deterministic Markdown section extraction với stable refs/ranges/hashes;
3. generated root/selected-area `_index.md` projections;
4. versioned disposable Tantivy index generations với atomic publication;
5. `pulse docs index|status|search|get|tree` và work-aware ranking adjustment;
6. retrieval evals đo quality, exclusions, determinism và context budget.

## Nguyên tắc

- Canonical prose vẫn là normal Git files; section records, `_index.md` và search index đều là projections.
- Registry quyết định identity, lifecycle, authority và index eligibility; lexical score không tạo authority.
- Search trả metadata/snippet trước; `get` mới đọc bounded canonical content; full file cần explicit opt-in.
- `get` không phục vụ body từ cache. Cache chỉ giúp tìm section ref/range; canonical file hiện tại phải được re-read và hash-checked.
- Section identity derive từ stable document ID + heading anchor, không từ path.
- Heading rename có thể đổi section ref; stale ref phải fail rõ và gợi ý candidates, không silently lấy section khác.
- Cache correctness không phụ thuộc cache tồn tại. Missing/corrupt/incompatible cache được rebuild từ registry + canonical docs.
- Cache publication dùng immutable generation directory + atomic current pointer để reader không thấy mixed generation.
- Generated `_index.md` là human navigation projection, không được search như authoritative content và không được register trong Slice 5.
- Search eligibility và applicability eligibility là hai policy khác nhau. Informational docs có thể searchable nhưng không được tự trở thành required context.
- Work context chỉ filter/adjust ranking trong bounded limits; strong lexical match không bị broad scope metadata đè mất.
- Lexical retrieval là deterministic mechanism thuộc kernel. Query formulation, chọn section nào cần đọc và semantic interpretation thuộc Agent.
- Không download model, không gọi network và không cần daemon/MCP/SQLite để search.
- CLI handlers giữ thin; extraction, indexing, ranking, cache và projection nằm trong typed library modules.

## Mục tiêu

Triển khai documentation retrieval foundation để có thể:

- migrate exact Slice 4 registry schema sang schema hỗ trợ retrieval metadata mà không overwrite unknown schema;
- opt in/out document indexing và index materialization bằng typed metadata;
- parse current registered Markdown thành deterministic section records;
- tạo stable section refs, heading paths, exact line ranges, document hash và section hash;
- xử lý preamble, duplicate headings, fenced code blocks và oversized sections theo contract;
- generate root và selected-area `_index.md` deterministic, marked generated và checkable;
- build Tantivy BM25 index hoàn toàn offline;
- publish cache generation atomically để concurrent readers chỉ thấy complete generation;
- incrementally reuse unchanged extracted records;
- detect missing/stale/corrupt/incompatible cache;
- search current eligible docs với bounded results/snippets và stable tie-break;
- adjust ranking theo document applicability cho `--work` mà không đổi authority;
- get exact current section bounded theo lines/bytes;
- browse registry-derived documentation tree mà không parse full corpus;
- expose index/retrieval fingerprints và explainable inclusion/exclusion reasons;
- chạy fixture evals cho exact identifier, paraphrase, Vietnamese, exclusions, no-result và context budget;
- giữ extension point rõ cho Slice 6 shaping/readiness/work packet và Phase 4 knowledge retrieval.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

- **#27:** `pulse docs search` trả đúng section với document ID, heading path, exact range và content hash mà không đưa full corpus vào Agent context.
- **#28:** xóa docs-search cache và generated `_index.md`, rebuild cho deterministic fingerprint/projection và equivalent expected ranking.
- **#29, retrieval completion:** retired, stale, migration backup và generated-navigation docs bị exclude/label đúng policy.
- **#30, retrieval subset:** bounded required/suggested section refs và read-budget primitives sẵn sàng cho future work packet; Slice 5 chưa implement full `pulse work packet`.
- **#31:** incremental reindex chỉ cập nhật changed documents; corrupt/incompatible cache bị discard/rebuild.
- **#32:** retrieval eval cover exact identifier, natural-language paraphrase, Vietnamese/tokenization, no-result và context budget.

### Decisions liên quan

- D-07, D-23, D-24, D-27.
- D-34 đến D-41.
- D-61 ở shared-engine boundary: Slice 5 chỉ xây docs corpus nhưng module boundary không được khóa knowledge vào docs-specific storage internals.

### Slice exit

Slice hoàn thành khi section extraction, generated navigation, lexical search/get/tree, cache/fingerprint/rebuild và retrieval eval deterministic, bounded và recoverable.

Slice exit **không** đồng nghĩa:

- `pulse work packet` đã hoàn chỉnh;
- Ticket có thể transition sang `ready`;
- shaping result hoặc section requirements đã được compose;
- semantic contradiction giữa docs/code/Decision đã được giải quyết;
- semantic/vector/hybrid retrieval tồn tại;
- knowledge compounding search dùng chung index đã được triển khai;
- docs close gate, link checker hoặc generated source freshness runner đã hoàn chỉnh;
- Phase 1 hoặc Core v1 hoàn thành.

## Non-goals

- Embeddings, vector database, QMD runtime dependency, semantic query expansion, reranker hoặc RRF hybrid mode.
- SQLite, long-lived daemon, HTTP service hoặc MCP search server.
- Code AST indexing hoặc unified code+docs search.
- Full `pulse work packet`, readiness composition hoặc lifecycle `draft -> shaped -> ready`.
- Shaping-map section references, decision frontier hoặc execution frontier.
- Automatic semantic summaries bằng LLM.
- Automatic alias generation hoặc ownership inference từ prose.
- Semantic contradiction detection giữa documents, Decisions, QA baseline và source.
- Link crawling, runnable snippet execution, external URL validation hoặc generated freshness command execution.
- Editing canonical prose qua `pulse docs get/search`.
- Auto-registering every Markdown file.
- Indexing work prose, evidence, runtime state, migration backups, vendored docs hoặc arbitrary repository text.
- Rich query language với arbitrary Boolean AST, regex, proximity operators hoặc user-controlled Tantivy syntax.
- Generic typo-tolerant fuzzy search trong initial public contract. Prefix/fuzzy behavior chỉ được thêm sau eval bằng schema/config version bump.
- Perfect language-specific stemming/segmentation cho mọi language. Slice phải có defined Unicode behavior và fixtures, không claim universal linguistic quality.
- Cross-repository docs federation.
- Maintaining historical section bodies after canonical document changes.
- Stable section identity across heading rename. Document identity ổn định; heading-derived section identity có explicit stale-ref semantics.
- Making `_index.md` a second writable registry or indexing it as contract content.

## Repository layout

```text
AGENTS.md
PULSE.md

docs/
  _index.md                         # generated root navigation projection
  product/
    _index.md                       # optional generated selected-area projection
    authentication.md
  architecture/
    authentication.md
  domain/
    token-lifecycle.md
  operations/
    recovery.md

.pulse/
  docs/
    registry.json                   # canonical metadata, schema v2 after migration
    schemas/
      document.schema.json
    retrieval-evals/                # tracked fixture queries/expectations
      core.jsonl

  cache/
    docs-search/                    # gitignored, disposable
      CURRENT                       # atomic pointer to complete generation ID
      generations/
        gen_<fingerprint>/
          state.json
          sections.jsonl
          tantivy/
            ...engine files...
      builds/
        build_<id>/                 # incomplete, safe to delete

  runtime/
    locks/
      workgraph.lock                # existing repository mutation/recovery fence
      docs-search.lock              # index writer lock; readers do not require it
```

Ownership:

- `.pulse/docs/registry.json` và canonical docs files là tracked truth.
- `docs/**/_index.md` là tracked hoặc repository-policy-selected generated projection; luôn rebuildable.
- `.pulse/docs/retrieval-evals/**` là tracked harness fixtures.
- `.pulse/cache/docs-search/**` là gitignored/disposable.
- `CURRENT` chỉ trỏ tới complete immutable cache generation; nó không phải canonical truth.
- Incomplete `builds/**` không bao giờ được search reader sử dụng.

## Registry schema evolution

### Lý do cần evolution deliberate

Slice 4 registry/model/schema dùng `deny_unknown_fields` và embedded schema hash validation. Chỉ thêm field vào Rust struct hoặc thay embedded schema sẽ làm existing repository bị schema drift. Slice 5 phải có known-predecessor migration, không silently reinterpret schema v1.

### Chọn schema v2

Registry envelope bump:

```jsonc
{
  "schema_version": 2,
  "revision": 8,
  "repository_id": "repo_01J...",
  "retrieval": {
    "schema_version": 1,
    "root": "docs",
    "include_repository_map": true,
    "include_repository_policy": true,
    "default_index": true,
    "default_include_body": true,
    "default_search_limit": 8,
    "default_get_max_lines": 120,
    "default_get_max_bytes": 32768,
    "auto_refresh_max_documents": 200,
    "auto_refresh_max_source_bytes": 20971520,
    "materialize_root_index": true,
    "area_index_threshold": 5,
    "scopes": [
      {
        "path": "docs/architecture",
        "summary": "System boundaries, dependencies and invariants.",
        "materialize_index": true
      }
    ]
  },
  "documents": [
    {
      "id": "DOC-AUTH-DOMAIN",
      "revision": 3,
      "path": "docs/domain/token-lifecycle.md",
      "kind": "domain",
      "authority": "approved",
      "lifecycle": "current",
      "owner": "team:identity",
      "summary": "Token types, lifecycle transitions and invariants.",
      "aliases": ["refresh tokens"],
      "scope": {
        "paths": ["src/auth/**"],
        "domains": ["authentication"],
        "work_labels": ["auth"]
      },
      "review_policy": "independent",
      "verification_profile": "domain-doc",
      "generated": null,
      "superseded_by": null,
      "retrieval": {
        "index": true,
        "include_body": true,
        "materialize_index": false
      }
    }
  ]
}
```

### Migration rules

- Binary biết exact canonical hash của Slice 4 `document.schema.json` và exact v1 model shape.
- Nếu registry/schema khớp exact known predecessor:
  1. acquire repository write guard;
  2. recover existing canonical transactions;
  3. load/validate v1 registry;
  4. add retrieval defaults without changing document IDs, document revisions hoặc semantic metadata;
  5. bump registry schema version và registry revision exactly once;
  6. replace schema + registry bằng recoverable multi-target transaction;
  7. emit one `docs.registry.schema_migrated` event with before/after schema hashes and registry revisions.
- Retry sau completed migration là unchanged/idempotent.
- Unknown v1 shape, unknown schema hash, future schema hoặc manually altered predecessor bị reject và preserved.
- Migration không auto-generate `_index.md`, không build cache và không change canonical prose.
- Adding retrieval defaults does not bump every document revision because document semantic identity/ownership did not change.
- Per-document edits that change only `retrieval.index`, `retrieval.include_body` or `retrieval.materialize_index` bump the registry revision but **do not** bump `document_revision`. In Slice 5, `document_revision` remains the receipt-bound revision for document identity, path, authority/lifecycle/ownership and verification-relevant policy metadata. This prevents a cache/indexing preference from invalidating an otherwise current documentation receipt.
- A mixed patch containing retrieval-only and receipt-relevant fields follows normal document mutation semantics and bumps both document and registry revision.
- Typed mutation/event output must identify `retrieval_only=true`; arbitrary patching cannot choose revision behavior.

### Registry retrieval config rules

- `root` là safe repository-relative directory; initial managed documentation tree root là `docs`.
- Registered `AGENTS.md` (`kind=repository_map`) and `PULSE.md` (`kind=policy`) are first-class Slice 5 retrieval inputs when `include_repository_map` / `include_repository_policy` are true; both default true. They appear under a virtual `Repository` area in `tree` and root `_index.md` rather than being forced under `docs/`.
- `default_search_limit`: `1..=50`, default `8`.
- `default_get_max_lines`: `1..=2000`, default `120`.
- `default_get_max_bytes`: `1024..=1_048_576`, default `32768`.
- `auto_refresh_max_documents`: `1..=10_000`, default `200`.
- `auto_refresh_max_source_bytes`: `1 MiB..=1 GiB`, default `20 MiB`.
- `area_index_threshold`: `1..=1000`, default `5`.
- Scope paths unique, normalized, under retrieval root và không kết thúc bằng `_index.md`.
- Longest matching scope path cung cấp area summary/context.
- Per-document retrieval fields override registry defaults.
- `index=false` loại document khỏi section cache/search nhưng không loại khỏi registry/applicability/show.
- `include_body=false` index title/heading/summary/aliases/path/domains nhưng không index section body; `get` vẫn đọc canonical content khi caller có ref.
- `materialize_index=true` yêu cầu parent area index entry ngay cả khi dưới threshold.
- Authored approved/informational documents inherit `default_index`. Generated output documents are **opt-in** regardless of `default_index`: they require explicit `retrieval.index=true`, because generated corpora can be large/noisy. Generated navigation `_index.md` is always excluded.

### Generated navigation registration

Slice 4 hiện reject `docs/**/_index.md` registration. Slice 5 giữ rule này:

- generated navigation không được register như document;
- authority/summary của area đến từ top-level retrieval scopes, không từ registering `_index.md`;
- nếu future repository cần một authored index làm durable contract, nó phải dùng tên/path khác hoặc Decision schema extension riêng.

Điều này tránh index tự index chính nó và tránh hai sources of truth.

## Retrieval eligibility

Search eligibility không reuse nguyên applicability eligibility.

### Included mặc định

Document được index/search khi:

- registry record hợp lệ;
- `retrieval.index=true` sau default resolution;
- current path là regular UTF-8 Markdown file dưới managed `docs/` root, hoặc registered `AGENTS.md`/`PULSE.md` enabled by registry retrieval config;
- lifecycle `current`;
- authority `approved` hoặc `informational`, hoặc authority `generated` với explicit per-document `retrieval.index=true`;
- không thuộc protected/migration/work/runtime/evidence/cache path;
- không phải generated navigation `_index.md`.

### Excluded mặc định

- lifecycle `retired`, `superseded`, `stale`, `suspected_stale`;
- authority `draft`;
- missing/unsafe/non-UTF-8 content;
- migration backups, `works/**`, `.pulse/evidence/**`, `.pulse/runtime/**`, `.pulse/cache/**`;
- generated navigation;
- per-document `index=false`.

### Query flags

`search` có thể nhận:

- `--include-draft`;
- `--include-stale`.

Flags chỉ mở result eligibility và luôn label authority/lifecycle; chúng không làm document authoritative hoặc gate-eligible.

`retired` và `superseded` không được search mặc định. `--include-stale` có thể include `suspected_stale|stale`, không include retired/superseded trừ future explicit history mode.

### Informational boundary

Informational docs:

- searchable mặc định;
- nhận no authority boost hoặc slight demotion so với approved khi lexical relevance tương đương;
- không tự xuất hiện trong document-level `required` applicability;
- output luôn giữ `authority=informational`.

## Section extraction contract

### Parser direction

Dùng `comrak` sau khi prototype xác nhận:

- version được chọn tương thích repository MSRV Rust 1.78, hoặc proposal implementation review chấp thuận deliberate MSRV bump;
- GFM heading/fenced-code parsing và source positions đáp ứng exact line-range fixtures;
- parser chạy offline và không execute embedded content.

Nếu available comrak version không đáp ứng MSRV/source-position contract, implementation có thể dùng equivalent pure-Rust Markdown parser sau benchmark/Decision. Public contract là extraction semantics, không phải crate name.

### Input contract

- Canonical file phải decode UTF-8; UTF-8 BOM được accepted nhưng không thuộc heading text.
- CRLF/LF đều được support; line numbers là 1-based logical source lines.
- Document bytes được hash exact như stored.
- Frontmatter, nếu có, không override registry identity/authority/owner. Nó thuộc preamble content hoặc parser metadata only.
- Markdown content được treated as untrusted text; parser không render/execute HTML/scripts.

### Base section model

```jsonc
{
  "schema_version": 1,
  "section_ref": "DOC-AUTH-DOMAIN#refresh-token-lifecycle",
  "section_id": "DOC-AUTH-DOMAIN#refresh-token-lifecycle",
  "document_id": "DOC-AUTH-DOMAIN",
  "document_revision": 3,
  "path": "docs/domain/token-lifecycle.md",
  "document_title": "Token Lifecycle",
  "heading": "Refresh token lifecycle",
  "heading_path": ["Token Lifecycle", "Refresh token lifecycle"],
  "anchor": "refresh-token-lifecycle",
  "ordinal": 1,
  "range": {"start_line": 12, "end_line": 44},
  "document_content_hash": "sha256:...",
  "section_content_hash": "sha256:...",
  "summary": "Token types, lifecycle transitions and invariants.",
  "authority": "approved",
  "lifecycle": "current",
  "owner": "team:identity",
  "kind": "domain",
  "domains": ["authentication"],
  "aliases": ["refresh tokens"],
  "body_indexed": true,
  "chunk": null
}
```

`sections.jsonl` giữ derived records và searchable body khi `include_body=true`. Nó nằm trong gitignored cache và không phải public truth.

### Document title

Title resolve theo thứ tự deterministic:

1. first level-1 heading;
2. registry summary is not title và không được dùng thay title;
3. normalized file stem fallback.

Multiple level-1 headings là allowed Markdown nhưng `docs index/status` tạo warning `docs_multiple_document_titles`. Heading path vẫn theo AST order.

### Section boundaries

- Preamble trước heading đầu tiên tạo section reserved `#preamble` nếu có non-whitespace meaningful content.
- Heading section bắt đầu tại heading line.
- Base section kết thúc ngay trước heading tiếp theo có level nhỏ hơn hoặc bằng current heading level.
- Nested headings tạo section riêng; parent section body/search text có thể include intro trước child heading nhưng không duplicate toàn bộ child body.
- Fenced code block không bị split giữa chừng.
- Empty heading section vẫn tạo outline/anchor record nhưng body có thể empty.
- Trailing newline không tạo extra line.

### Stable anchor normalization

Anchor algorithm phải version hóa trong extractor config:

1. trim Unicode whitespace;
2. Unicode lowercase;
3. remove inline Markdown formatting while preserving visible text;
4. replace runs of whitespace and separator punctuation with `-`;
5. preserve Unicode letters/numbers;
6. trim leading/trailing `-`;
7. if empty, use `section`;
8. reserved `preamble` chỉ dành cho preamble; heading text `Preamble` dùng normal duplicate suffix rules.

Duplicate base anchors trong cùng document:

```text
DOC-ID#errors
DOC-ID#errors-2
DOC-ID#errors-3
```

Ordinal derive từ source order. Same document bytes luôn tạo same refs. Path rename giữ refs vì document ID giữ nguyên. Heading text/order change có thể đổi refs.

### Hashes và ranges

- `document_content_hash`: SHA-256 exact full file bytes.
- `section_content_hash`: SHA-256 exact source-byte slice covering base/chunk range, không hash rendered Markdown.
- Range là inclusive 1-based source lines.
- Cache state giữ file byte length và hash để detect changed bytes.
- `get` re-read file, verify document hash, resolve current section extraction và verify section hash before returning body.
- Nếu cache range/hash không current, `get` không dùng stale slice; nó refreshes/re-extracts hoặc trả `docs_anchor_stale`/`docs_index_stale` theo flags.

### Oversized sections và chunks

Initial limits là versioned index config, không public constants forever:

- soft max `8,000` UTF-8 bytes hoặc `160` lines cho one retrieval chunk;
- hard max `32,768` bytes cho default returned chunk;
- overlap target `8` lines, chỉ khi split base section;
- never split inside fenced code block.

Split precedence:

1. nested heading boundary;
2. blank-line paragraph boundary;
3. safe line boundary outside fence.

Chunk refs:

```text
DOC-ID#large-section@1
DOC-ID#large-section@2
```

Base section ref vẫn resolve outline metadata. `search` trả chunk ref khi indexed unit là chunk. `get` base ref mặc định trả first bounded chunk + outline/chunk count; `--full-section` explicit để lấy toàn base section trong configured max bytes.

Chunking algorithm/config version tham gia index fingerprint. Thay config làm old chunk refs stale và bắt rebuild.

## Generated `_index.md` projection

### Scope

Slice 5 materialize:

- root `docs/_index.md` khi `materialize_root_index=true`;
- selected area `_index.md` khi scope `materialize_index=true`, document override yêu cầu, hoặc direct registered-document count đạt threshold.

Không generate ở mọi directory.

### Content

```markdown
# Documentation Index

> Generated by `pulse docs index`. Do not edit manually.
> Registry fingerprint: `sha256:...`

## Domain

- [Token Lifecycle](domain/token-lifecycle.md)
  Token types, lifecycle transitions and invariants.
  Owner: `team:identity` · Authority: `approved`
```

Rules:

- deterministic ordering theo area path, kind precedence, document title/path/ID tie-break;
- repository-relative portable links dùng `/` separator;
- only current eligible documents;
- draft/stale/retired/superseded excluded mặc định;
- summary lấy từ registry, không LLM-generate;
- generated marker và projection schema/version marker bắt buộc;
- projection không include every section/chunk;
- no timestamps để cùng input cho exact same bytes;
- registry/index fingerprint marker dùng stable value, không machine path;
- `pulse docs index --check` so sánh expected bytes, không chỉ mtime.

### Publication và hand edits

- Mỗi projection file dùng same-directory atomic replace.
- Projection files là derived; partial crash giữa multiple area files có thể để một số files stale nhưng không corrupt canonical truth.
- `index --check` detect mixed/stale projection; next `index` repairs idempotently.
- Unknown existing `_index.md` không có Pulse generated marker bị preserve và command fail `docs_index_projection_conflict`; không overwrite user-authored file.
- Existing generated marker với unsupported projection schema bị preserve và require explicit migration/rebuild policy; không silently rewrite unknown contract.
- Generated projection update không emit semantic workgraph event. Status/check output là evidence; future verification profile có thể create receipt.

## Lexical index contract

### Engine direction

Use Tantivy BM25 behind a docs search-engine interface.

Prototype must verify:

- selected Tantivy version supports Rust 1.78 or an approved MSRV change;
- deterministic fixture ranking on supported platforms;
- atomic/versioned directory handling compatible with Pulse publication design;
- index size/latency at 10, 100, 1,000 documents and representative section counts.

Public compatibility contract is:

- section-level offline lexical ranking;
- stable typed output;
- deterministic eligibility/fingerprint/tie-break;
- rebuildable disposable cache;
- no model/native service dependency.

Raw Tantivy on-disk format is private cache implementation.

### Indexed fields

| Field | Purpose | Initial boost |
|---|---|---:|
| `heading` | exact section topic | 5.0 |
| `document_title` | document topic | 4.0 |
| `heading_path` | hierarchical context | 3.0 |
| `aliases` | approved alternate terminology | 3.0 |
| `domains` | typed domain terms | 3.0 |
| `summary` | authored document summary | 2.5 |
| `path` | exact path/component terms | 1.5 |
| `body` | section/chunk prose | 1.0 |
| `identifiers` | preserved command/error/version/hyphenated tokens | exact-match boost |

Boosts are index config, included in fingerprint and tuned by eval. They are not probabilities or public fixed constants.

Stored fields minimally include section ref, document ID/path, heading path/range, hashes, authority/lifecycle/owner/kind and snippet source. Search must not need to load all canonical Markdown files into memory to list hits.

### Tokenization

Initial tokenizer contract:

- Unicode lowercase;
- split normal prose on Unicode whitespace/punctuation;
- preserve an additional identifier representation for tokens containing `-`, `_`, `.`, `/`, `:` or digits;
- no language stemming in initial slice;
- quoted user text is treated as text, not executable Tantivy query syntax;
- max query length/term count enforced;
- control characters rejected or normalized safely.

Defined language behavior:

- Vietnamese diacritics are preserved; whitespace-delimited Vietnamese words are searchable exactly and through normal BM25 terms.
- Hyphenated identifiers such as `refresh-token`, work IDs and commands are searchable via both component tokens and preserved identifier field.
- Dotted versions such as `v2.1` are searchable as preserved identifiers.
- CJK without whitespace has defined whole-run token behavior in Slice 5; no broad segmentation-quality claim. Repository aliases can bridge critical terms. A future n-gram/tokenizer change requires config/fingerprint bump and eval.
- No fuzzy typo expansion in initial slice. No-result remains honest rather than silently broadening to noisy matches.

### Query parsing

Public query is plain text, not Tantivy query language.

- Escape/sanitize engine syntax.
- Empty/whitespace-only query returns `invalid_docs_query`.
- Default maximum 256 UTF-8 chars and 32 normalized terms.
- Kind/domain/authority/work filters are typed CLI options, not embedded query syntax.
- Exact phrase behavior is not promised unless a future query schema adds it.

### Ranking pipeline

```text
query text
  -> safe lexical terms
  -> registry eligibility filters
  -> Tantivy BM25 field scores
  -> bounded work-context metadata adjustment
  -> deterministic tie-break
  -> snippets/result contract
```

Metadata adjustment can use:

- explicit required document reference;
- document-level path/domain/label scope match from Slice 4;
- requested kind/domain/authority filters;
- current approved authority as a small tie influence.

Rules:

- retired/stale/draft exclusion happens before ranking unless flags enable them;
- explicit required document is strongest metadata signal but cannot produce a result with zero lexical match in normal search;
- work-scope adjustment is capped, proposal initial cap at maximum 20% of lexical score contribution;
- informational authority cannot outrank a substantially stronger approved lexical hit solely due to scope;
- results sort by adjusted score descending, then raw lexical score descending, then section ref lexical;
- score is engine-relative `f64`, not probability;
- JSON `--explain` returns matched fields and reason codes, not engine-internal query AST.

### No-result behavior

- Valid query with no hit exits success and returns `results: []`.
- Missing required index under `--no-refresh` returns typed stale/missing error.
- No-result does not trigger automatic fuzzy/semantic fallback.
- `search --work` may return suggested fallback query terms derived from typed work metadata only if available, but Slice 5 does not auto-run them.

## Cache architecture

### Why immutable generations

Publishing `state.json`, `sections.jsonl` and a Tantivy directory independently can expose mixed generations. Slice 5 uses immutable generation directories and one atomic pointer:

```text
build complete generation
  -> fsync files/directories
  -> rename build directory to generations/gen_<fingerprint>
  -> atomic replace CURRENT pointer
```

Readers:

1. read `CURRENT`;
2. validate pointer syntax;
3. open referenced generation;
4. validate state/fingerprint/file hashes;
5. search immutable generation;
6. optionally retry once if `CURRENT` changes during open.

A reader sees old complete generation or new complete generation, never half new generation.

### Generation state

```jsonc
{
  "schema_version": 1,
  "generation_id": "gen_sha256_...",
  "fingerprint": "sha256:...",
  "engine": {
    "mode": "lexical",
    "name": "tantivy",
    "version": "..."
  },
  "extractor": {
    "name": "pulse-markdown-sections",
    "version": 1,
    "anchor_version": 1,
    "chunk_version": 1
  },
  "config_hash": "sha256:...",
  "registry_retrieval_hash": "sha256:...",
  "documents": {
    "DOC-AUTH-DOMAIN": {
      "document_revision": 3,
      "path": "docs/domain/token-lifecycle.md",
      "content_hash": "sha256:...",
      "section_count": 7,
      "chunk_count": 9,
      "body_indexed": true
    }
  },
  "sections_file_hash": "sha256:...",
  "projection_hashes": {
    "docs/_index.md": "sha256:..."
  },
  "counts": {
    "registered": 12,
    "eligible": 10,
    "indexed": 10,
    "sections": 74,
    "chunks": 81,
    "excluded": 2
  }
}
```

No absolute machine paths or build timestamps participate in fingerprint.

### Retrieval fingerprint

Fingerprint derives from canonical serialization of:

- cache state schema version;
- extractor/anchor/chunk versions;
- engine name/version compatibility ID;
- tokenizer/query/index config;
- retrieval-relevant registry metadata only;
- sorted eligible document IDs, revisions, paths and exact content hashes;
- generated projection configuration.

Retrieval-relevant metadata includes summary, aliases, scope domains/work labels/paths, kind, authority, lifecycle and per-document retrieval config. Review policy or unrelated receipt policy need not invalidate lexical index unless stored/searchable output includes them.

Full registry fingerprint is reported for traceability but not necessarily used as sole retrieval fingerprint. This allows unrelated registry edits to avoid needless rebuild while preserving correctness.

### Incremental rebuild

On `pulse docs index`:

- load current valid generation if any;
- compare retrieval config/hash and per-document record/hash;
- reuse extracted section records for documents whose relevant metadata, document revision/path and content hash are unchanged;
- re-extract changed/new documents;
- remove no-longer-eligible/deleted documents;
- build a new complete Tantivy generation from resulting records;
- publish atomically.

Initial implementation may rebuild the Tantivy segment set from reused section records rather than performing in-place mutation. Acceptance concerns changed-document extraction reuse and equivalent output, not engine micro-optimization.

### Missing, stale, corrupt and incompatible states

- `missing`: no valid `CURRENT`/generation.
- `stale`: current registry/config/content fingerprint differs from generation.
- `corrupt`: pointer/state/sections/index hashes or engine open fail.
- `incompatible`: schema/engine/extractor version unsupported.
- `current`: all validated inputs match.

Default search behavior:

- current: search immediately;
- missing/stale and corpus at or below both configured auto-refresh limits: acquire docs-search writer lock, build/refresh, then search;
- missing/stale and corpus exceeds either auto-refresh limit: return `docs_index_refresh_required` with observed counts/bytes and the exact `pulse docs index` recovery command; do not start an unbounded build from a read-oriented query;
- corrupt/incompatible and corpus within limits: quarantine/delete disposable generation, rebuild, then search;
- corrupt/incompatible above limits: return typed error requiring explicit rebuild;
- `--no-refresh`: return typed error for missing/stale/corrupt/incompatible; stale search is not supported in the initial contract.

Corrupt cache handling never modifies canonical registry/docs. Auto-refresh limits are public cost guards, not performance claims.

### Writer locking and cleanup

- One docs-search-specific exclusive writer lock prevents duplicate publication work.
- Registry/canonical mutation recovery occurs before snapshot capture.
- Index writer captures registry + document hashes under a coherent snapshot phase, releases repository guard before expensive engine build, then revalidates fingerprint before publish.
- If inputs changed during build, generation is not published as current; command retries once or returns `docs_index_inputs_changed`.
- Search readers do not hold global repository write guard; they read immutable complete generations.
- Orphan `builds/**` and unreferenced old generations are safe to clean after TTL/count policy.
- Keep current generation and at least one previous complete generation during cleanup for reader safety.

## CLI surface

```text
pulse docs index [--changed] [--rebuild] [--check] [--json]
pulse docs status [--json]
pulse docs search <query>
  [--kind <kind>]
  [--domain <slug>]
  [--authority <authority>]
  [--work <work-id>]
  [--limit <n>]
  [--include-draft]
  [--include-stale]
  [--no-refresh]
  [--explain]
  [--json]
pulse docs get <document-id|section-ref|path:range>
  [--max-lines <n>]
  [--max-bytes <n>]
  [--full]
  [--full-section]
  [--no-refresh]
  [--json]
pulse docs tree [path]
  [--depth <n>]
  [--include-draft]
  [--include-stale]
  [--json]
```

Existing Slice 4 commands remain:

```text
pulse docs register|edit|retire|supersede
pulse docs list|show|validate|applicable|impact
```

No top-level `pulse docs-search` command.

## `pulse docs index`

### Normal mode

1. recover/refuse unresolved canonical transaction;
2. migrate known registry schema predecessor if command policy allows bootstrap migration;
3. validate registry/retrieval config;
4. capture eligible docs, metadata and content hashes;
5. reuse unchanged section records;
6. extract changed documents;
7. build complete Tantivy generation;
8. generate expected `_index.md` bytes;
9. revalidate canonical inputs;
10. atomically publish cache generation;
11. atomically write generated projections;
12. return status/fingerprint/change summary.

Cache may publish before projections; state separately reports projection freshness. Search correctness does not depend on projection files.

### `--changed`

Explicitly request incremental extraction. Since normal mode also attempts safe incremental reuse, flag mainly makes output/error contract strict: incompatible current generation may fallback full rebuild only when reported.

### `--rebuild`

Ignore reusable cache records, parse all eligible docs and build new generation. Same inputs must produce same fingerprint and equivalent section/ranking semantics.

### `--check`

Read-only:

- no migration;
- no cache/projection write;
- compute current expected fingerprint/projection hashes;
- exit zero only if cache and required projections current;
- return typed findings otherwise.

## `pulse docs status`

Example JSON:

```jsonc
{
  "schema_version": 1,
  "registry": {
    "revision": 8,
    "fingerprint": "sha256:..."
  },
  "index": {
    "state": "current",
    "fingerprint": "sha256:...",
    "generation_id": "gen_sha256_...",
    "engine": "tantivy",
    "mode": "lexical"
  },
  "documents": {
    "registered": 12,
    "eligible": 10,
    "indexed": 10,
    "excluded": 2,
    "changed": 0
  },
  "sections": 74,
  "chunks": 81,
  "projections": {
    "state": "current",
    "files": ["docs/_index.md"]
  },
  "warnings": []
}
```

`status` does not auto-refresh. It is cheap/read-only and reports current observed state.

## `pulse docs search`

Example JSON:

```jsonc
{
  "schema_version": 1,
  "query": "refresh token expiry",
  "normalized_terms": ["refresh", "token", "expiry"],
  "index": {
    "fingerprint": "sha256:...",
    "generation_id": "gen_sha256_...",
    "state": "current",
    "mode": "lexical"
  },
  "work": {
    "id": "TK-031",
    "revision": 4
  },
  "results": [
    {
      "rank": 1,
      "score": 6.284,
      "lexical_score": 5.91,
      "section_ref": "DOC-AUTH-DOMAIN#expired-tokens",
      "document_id": "DOC-AUTH-DOMAIN",
      "document_revision": 3,
      "path": "docs/domain/token-lifecycle.md",
      "heading_path": ["Token Lifecycle", "Expired tokens"],
      "range": {"start_line": 31, "end_line": 44},
      "document_content_hash": "sha256:...",
      "section_content_hash": "sha256:...",
      "summary": "Expired token semantics and transition rules.",
      "snippet": "TokenExpired represents a refresh token that was valid but is no longer...",
      "authority": "approved",
      "lifecycle": "current",
      "owner": "team:identity",
      "kind": "domain",
      "matched_fields": ["heading", "body", "domains"],
      "applicability_reasons": ["domain_scope_match", "path_scope_match"]
    }
  ],
  "budget": {
    "result_limit": 8,
    "snippet_max_bytes": 500,
    "returned_snippet_bytes": 2140
  }
}
```

### Snippets

- default 3–6 source lines or max 500 UTF-8 bytes per result;
- derive from stored indexed section text around matched terms;
- include no hidden full body;
- preserve safe plain text/Markdown excerpt;
- escape JSON correctly;
- line range in result always refers full indexed section/chunk; optional snippet subrange may be included;
- output byte/line budget is measured and returned.

### Filters

- `--kind`, `--domain`, `--authority` are exact typed filters.
- `--work` loads current work context and Slice 4 applicability reasons.
- Missing work item is typed error; invalid/unknown documentation posture does not block search, but output labels context gap.
- Filters cannot include documents excluded by protected-path policy.

## `pulse docs get`

### Accepted refs

- document ID: `DOC-AUTH-DOMAIN`;
- section ref: `DOC-AUTH-DOMAIN#expired-tokens`;
- chunk ref: `DOC-AUTH-DOMAIN#large-section@2`;
- explicit path range: `docs/domain/token-lifecycle.md:31-44`, only for registered current document path.

### Document ID behavior

Without `--full`, return:

- document metadata/summary;
- outline of section refs/headings/ranges;
- bounded preamble/first section preview;
- exact current document hash;
- read-budget metadata.

`--full` explicitly returns full canonical document subject to hard repository max bytes and protected-path policy.

### Section behavior

- Re-read canonical file.
- Re-extract current sections or validate cached extraction against exact document hash.
- Return exact requested section/chunk bounded by `--max-lines`/`--max-bytes`.
- Include line-numbered range metadata and hashes.
- If requested base section exceeds budget, return outline/chunk refs and `truncated=true`; do not silently omit tail while claiming complete.
- `--full-section` explicitly requests full base section, still subject to hard max bytes.

### Stale refs

If anchor not found:

- return non-zero `docs_anchor_stale`;
- include current document ID/path/hash;
- include up to five nearest current section candidates based on exact base anchor prefix, heading token overlap and source proximity if old cache metadata exists;
- never silently map to nearest section.

If document was superseded/retired/stale:

- normal get reports lifecycle error and replacement when known;
- future history flag may expose it, but not in initial Slice 5 contract.

## `pulse docs tree`

Tree derives from registry paths/scopes, not full Markdown bodies.

Example JSON:

```jsonc
{
  "schema_version": 1,
  "root": "docs",
  "nodes": [
    {
      "path": "docs/domain",
      "kind": "area",
      "summary": "Domain vocabulary, rules and state machines.",
      "children": [
        {
          "path": "docs/domain/token-lifecycle.md",
          "kind": "document",
          "document_id": "DOC-AUTH-DOMAIN",
          "summary": "Token types, lifecycle transitions and invariants.",
          "authority": "approved",
          "lifecycle": "current",
          "owner": "team:identity"
        }
      ]
    }
  ]
}
```

Rules:

- safe path prefix only;
- default depth bounded, proposal `3`;
- deterministic lexical/path ordering;
- no section bodies;
- optional section count/current index status may be attached from cache;
- tree remains available without search cache using registry only.

## Work-context integration

Slice 5 reuses Slice 4 document-level applicability, but preserves boundaries:

- `docs applicable --work` remains deterministic document routing without lexical query.
- `docs search --work` requires query and applies bounded metadata adjustment.
- Search output may report `explicit_required_document`, `path_scope_match`, `domain_scope_match`, `label_scope_match`.
- Search does not mutate Ticket references.
- Search does not turn optional scope match into required context.
- Explicit required document with no lexical hit does not appear as fake result; output can include `required_documents_without_hits` so future packet builder can separately include required outline/sections.

Slice 5 adds a library-level `RetrievalSuggestion` contract suitable for Slice 6:

```jsonc
{
  "section_ref": "DOC-AUTH-ARCH#error-mapping",
  "document_id": "DOC-AUTH-ARCH",
  "score": 4.21,
  "reason_codes": ["lexical_match", "domain_scope_match"],
  "content_hash": "sha256:...",
  "range": {"start_line": 40, "end_line": 62}
}
```

It does not yet decide final work-packet required/suggested budgets.

## Validation and findings

Extend `pulse docs validate` and retrieval commands with mechanical findings:

```text
docs_registry_retrieval_config_invalid
docs_registry_schema_migration_required
docs_document_not_utf8
docs_document_parse_failed
docs_multiple_document_titles
docs_section_anchor_collision
docs_section_range_invalid
docs_section_oversized
docs_index_missing
docs_index_stale
docs_index_corrupt
docs_index_incompatible
docs_index_inputs_changed
docs_index_projection_missing
docs_index_projection_stale
docs_index_projection_conflict
docs_search_query_invalid
docs_search_miss
docs_search_noise
docs_anchor_stale
docs_tokenization_gap
docs_context_bloat
```

Hard errors:

- unsafe/non-UTF-8 registered current indexed doc;
- parse/source-range invariant failure;
- unknown registry/index schema;
- projection conflict with user-authored `_index.md`;
- cache hash mismatch when `--no-refresh`;
- requested stale section ref.

Warnings/advisories:

- multiple H1 headings;
- missing/weak title fallback;
- oversized section split;
- missing area summary;
- retrieval eval miss/noise;
- draft/stale inclusion under explicit flags.

Kernel does not create semantic summary, decide prose correctness or resolve contradictions.

## Evidence and receipt boundary

Slice 5 does not introduce a mandatory new immutable receipt kind for every search/index call.

It does produce machine-readable outputs that future verification can bind:

- registry revision/fingerprint;
- retrieval fingerprint/generation ID;
- document and section hashes;
- generated projection hashes;
- eval fixture hash/results.

Optional fixture/eval receipt extension may be proposed later. Search result is retrieval evidence, not proof that content is correct.

### Documentation receipt compatibility owned by this slice

Current implementation intentionally supports `documentation_validation` payload version `1` only, and evidence verification currently reads a reduced registry JSON model separately from `src/docs`. Slice 5 must make registry schema v2 safe without broadening the immutable receipt payload contract:

- `documentation_validation` payload remains version `1` in Slice 5;
- evidence manifest/schema support remains version `1`; payload version `2` continues to be rejected;
- existing receipts remain byte-for-byte immutable and verify with existing integrity/binding/registry/policy semantics;
- before registry v2 migration is enabled, evidence receipt verification must load registry records through a typed docs snapshot/interface, or its reduced parser must be explicitly version-aware and contract-tested against both exact registry v1 and v2;
- retrieval-only metadata is ignored by documentation receipt verification;
- retrieval-only registry edits do not bump `document_revision`, so they do not stale current receipts;
- path, lifecycle, authority, owner/review-policy or other verification-relevant edits keep current Slice 4 document-revision and receipt invalidation behavior;
- registry migration must not rewrite the evidence manifest, receipt schemas or stored receipts;
- tests must prove a valid pre-migration v1 receipt has the same verification result before and after registry v2 migration.

A future receipt payload v2, section-level review receipt or retrieval-eval receipt requires a separate explicit schema proposal. It is not smuggled into Slice 5.

## Transaction, recovery and consistency

### Canonical registry migration

Registry schema + embedded schema replacement uses existing recoverable multi-target transaction and immutable migration event.

### Cache generation

Cache uses derived-generation publication, not semantic workgraph transactions:

- build directory is incomplete/non-addressable;
- completed generation is immutable;
- `CURRENT` atomic replace is publication point;
- crash before pointer leaves old generation current;
- crash after pointer leaves new complete generation current;
- orphan builds are cleaned;
- corrupt current generation is disposable and rebuilt.

### Generated projections

Each `_index.md` atomic replace is independently safe. Mixed projection freshness after crash is detectable and repairable because projections are derived. Canonical docs/registry remain untouched.

### Input snapshot consistency

Expensive index build must not hold repository-wide lock for its full duration.

Proposed protocol:

```text
1. acquire repository guard
2. recover canonical transactions
3. load/validate registry and capture relevant metadata
4. hash eligible document bytes
5. release repository guard
6. extract/build generation in private build dir
7. reacquire repository guard
8. recover, reload and recompute cheap input fingerprint
9. if changed: do not publish; retry once or return typed conflict
10. atomically publish generation pointer and projections
11. release guard
```

This gives optimistic snapshot validation without blocking graph/docs mutations during full index build.

Search reader opening immutable cache does not need global guard. `get` re-reads canonical file and validates hash; registry lifecycle/path must be checked before return.

## Security and trust

- Only registry-approved safe paths are indexed.
- Symlink escape is rejected using existing safe path primitives.
- Migration backups, work prose, evidence, runtime, cache, `.git`, vendored/generated navigation and protected secret roots are excluded before parsing.
- Maximum document bytes, sections per document, heading depth, query length, result count and snippet/get bytes are bounded.
- Markdown/HTML/scripts are never executed.
- Query text is not passed as raw Tantivy query syntax.
- Snippets are untrusted content and never promoted to Pulse instructions by ranking.
- Registry authority remains separate from content text.
- Cache files are validated before open/use and can be discarded safely.
- JSON output escapes control characters and preserves schema.
- Index must not include files ignored/protected by repository policy even if a malicious registry record attempts it.

## Library/module layout đề xuất

```text
src/
  docs/
    mod.rs
    model.rs                 # existing registry model + retrieval config v2
    manifest.rs              # known-predecessor schema migration
    registry.rs              # existing mutations + retrieval typed patch
    validate.rs              # registry + retrieval config/eligibility validation
    policy.rs                # shared lifecycle/path policy with consumer-specific authority rules
    applicability.rs         # existing document-level routing
    section.rs               # section/chunk models, refs, anchors, ranges
    markdown.rs              # comrak adapter and source extraction
    projection.rs            # root/area _index.md deterministic bytes/check
    cache.rs                 # generations/CURRENT/state validation/cleanup
    index.rs                 # orchestrate capture, incremental extraction, build/publish
    lexical.rs               # Tantivy adapter, schema/tokenizer/query
    search.rs                # typed filters, work adjustment, snippets/results
    get.rs                   # current canonical bounded retrieval/stale refs
    tree.rs                  # registry-derived navigation tree
    eval.rs                  # fixture load/run/metrics

  schema/
    docs/
      document.schema.json
      docs-index-state.schema.json
      docs-section.schema.json
      retrieval-eval.schema.json

  bin/
    pulse.rs                 # thin CLI only

tests/
  docs_registry_migration.rs
  docs_section_extraction.rs
  docs_projection.rs
  docs_index.rs
  docs_index_concurrency.rs
  docs_search.rs
  docs_get.rs
  docs_tree.rs
  docs_retrieval_eval.rs
  docs_retrieval_cli_contract.rs
```

Boundary rules:

- Markdown parser adapter returns typed source sections; it does not know CLI or work graph.
- Search engine consumes derived section records; it does not parse registry raw JSON.
- Registry policy exposes consumer-specific eligibility; applicability and search do not share one ambiguous `eligible()` boolean.
- `get` reads canonical files through a content resolver and verifies hashes; it does not trust stored body blindly.
- Cache manager owns generations/pointer/validation; Tantivy adapter does not decide publication.
- Work graph integration passes typed `WorkDocumentationContext` to search adjustment; lexical module does not load nodes itself.
- Evals call the same public library search path as CLI.
- Future knowledge search may reuse lexical engine/cache interfaces, not docs section schema/authority semantics.

## Retrieval evaluation

### Fixture format

Tracked JSONL fixture:

```jsonc
{
  "id": "docs-auth-expiry-natural-language",
  "query": "điều gì xảy ra khi refresh token hết hạn",
  "work_context": {
    "paths": ["src/auth/token.rs"],
    "domains": ["authentication"],
    "labels": ["auth"]
  },
  "filters": {},
  "expected": {
    "top_k": [
      "DOC-AUTH-DOMAIN#expired-tokens",
      "DOC-AUTH-ARCH#error-mapping"
    ],
    "must_exclude": [
      "DOC-AUTH-OLD#legacy-refresh"
    ],
    "max_first_relevant_rank": 3,
    "max_context_bytes_before_first_relevant": 3000
  }
}
```

### Command/library surface

Slice 5 may expose initially:

```text
pulse docs index --eval [--json]
```

hoặc keep eval as test/library harness if public command is not yet needed. Whichever is chosen, fixture schema/results must be stable enough for CI.

### Required fixture classes

- exact identifier/error/command;
- heading phrase;
- natural-language paraphrase;
- Vietnamese with diacritics;
- hyphenated identifier;
- dotted version;
- path/domain/work-context adjustment;
- retired/stale/draft/migration/generated-navigation exclusion;
- informational doc labeling;
- duplicate heading refs;
- no-result query;
- query with strong unrelated lexical match to ensure scope boost does not dominate;
- context-budget/truncation behavior.

### Metrics

- Recall@K;
- Mean Reciprocal Rank;
- required expected-section inclusion rate;
- must-exclude accuracy;
- first relevant rank;
- snippet/context bytes before first useful section;
- index build latency;
- search latency;
- generation size;
- changed-document extraction count;
- cache/projection deterministic rebuild equality.

Initial quality gates are fixture-defined rather than one universal threshold. Before Slice exit, core fixture set must have zero must-exclude violations and meet each fixture's top-K/budget bounds.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| R1 | Exact Slice 4 registry schema migration | prerequisite | v1 -> v2 once, IDs/revisions preserved, registry revision/event correct |
| R2 | Unknown/modified predecessor schema | integrity | Preserve files, reject migration rõ |
| R3 | Registry retrieval config normalization | integrity | Deterministic defaults/order/hash |
| R4 | `index=false` document | boundary | Registry/applicable vẫn thấy; search index không include |
| R5 | Informational current doc | authority | Searchable/labeled; không thành required applicability |
| R5a | Registered `AGENTS.md` and `PULSE.md` | D-24/D-34 | Indexed/gettable/tree-visible under Repository area with bounded sections |
| R5b | Generated current doc without explicit opt-in | noise boundary | Excluded; explicit `retrieval.index=true` includes it, `_index.md` never included |
| R6 | Retired/superseded/stale/draft docs | #29 | Default excluded; flags only expose allowed states with labels |
| R7 | Migration/work/runtime/evidence/cache paths | security/#29 | Never parsed/indexed |
| R8 | UTF-8 LF Markdown extraction | #27 | Exact heading path/range/hashes |
| R9 | CRLF Markdown extraction | portability | Same logical refs/ranges; exact byte hashes reflect CRLF |
| R10 | Preamble before first heading | extraction | Stable `#preamble` when meaningful |
| R11 | Duplicate headings | #27 | Deterministic `anchor`, `anchor-2`, `anchor-3` |
| R12 | Fenced code containing `#` | extraction | No false heading/split inside fence |
| R13 | Nested headings | extraction | Correct hierarchy and non-duplicated base bodies |
| R14 | Empty/multiple H1 headings | diagnostics | Stable fallback/warnings, no panic |
| R15 | Oversized section | budget | Safe chunks, stable refs, no fence split |
| R16 | Path rename stable document ID | identity | Section refs stable when headings unchanged; path/hash metadata update |
| R17 | Heading rename | stale refs | Old ref fails `docs_anchor_stale`, suggestions only |
| R18 | Root `_index.md` generation | #28 | Deterministic marker/links/order/summary |
| R19 | Selected area index threshold/config | #28 | Only configured/threshold areas materialize |
| R20 | User-authored `_index.md` conflict | safety | Preserve, fail; no overwrite |
| R21 | Projection delete/rebuild | #28 | Same expected bytes |
| R22 | Initial lexical index build | #27 | Complete generation/state/sections/Tantivy index |
| R23 | Atomic generation publication | consistency | Reader sees old or new complete generation |
| R24 | Crash before CURRENT publication | recovery | Old generation remains current; orphan safe |
| R25 | Crash after CURRENT publication | recovery | New complete generation opens; no mixed files |
| R26 | Delete cache and rebuild | #28 | Same fingerprint and equivalent expected rankings |
| R27 | Changed one document | #31 | Only changed extraction reruns; removed/changed hits update |
| R28 | Registry retrieval metadata change | #31 | Relevant fingerprint invalidates and rebuilds |
| R29 | Unrelated review-policy registry edit | incremental | No needless retrieval fingerprint change unless stored contract requires |
| R29a | Retrieval-only document edit | receipt compatibility | Registry revision changes; document revision/valid receipt remain current; index invalidates as needed |
| R29b | Pre-migration documentation receipt | compatibility | Same payload v1 verification result before/after registry v2 migration; payload v2 still rejected |
| R30 | Corrupt CURRENT/state/sections/Tantivy | #31 | Detect, discard/quarantine, rebuild; canonical docs untouched |
| R31 | Incompatible extractor/engine version | #31 | Full rebuild with typed status |
| R32 | Inputs change during build | concurrency | Stale build not published; retry/conflict rõ |
| R33 | Concurrent index writers | concurrency | One publisher/current generation; no corrupt pointer |
| R34 | Concurrent search during rebuild | consistency | Search uses complete old/new generation |
| R35 | Exact identifier search | #27/#32 | Expected section in fixture top K |
| R36 | Natural-language paraphrase | #27/#32 | Expected section meets fixture rank |
| R37 | Vietnamese query | #32 | Defined exact-diacritic behavior passes fixture |
| R38 | Hyphenated ID and dotted version | #32 | Preserved identifier matches |
| R39 | No-result query | #32 | Success with empty results, no broad fallback |
| R40 | Work-context adjustment | #30/#32 | Relevant scope improves tie/near-tie; cannot swamp strong lexical hit |
| R41 | Kind/domain/authority filters | contract | Typed exact filtering, deterministic results |
| R42 | Search snippet budget | #27/#30 | No full body leak, limits/returned bytes reported |
| R43 | `get` document ID | progressive disclosure | Summary+outline+bounded preview by default |
| R44 | `get` section/chunk | #27 | Exact canonical current range/body/hash |
| R45 | `get --full`/`--full-section` | explicit expansion | Explicit only, hard byte cap enforced |
| R46 | Document changes after cached search | freshness | Get revalidates current bytes; stale ref/range not silently returned |
| R47 | Tree without cache | navigation | Registry-derived tree works offline without index |
| R48 | Search JSON/human contracts | CLI | Stable schemas, ordering, exit codes |
| R49 | Index/status/check JSON contracts | CLI | Missing/stale/current/corrupt states stable |
| R50 | Eval exclusions and context bytes | #32 | Zero forbidden hits, per-fixture budget pass |
| R51 | Bench 10/100/1,000 docs | risk | Warm search p95 <= 100 ms at 1,000 docs; cold full build p95 <= 10 s; incremental one-doc refresh p95 <= 2 s; cache <= 3x indexed UTF-8 source bytes on reference fixture |
| R51a | Search auto-refresh cost guard | operability | Auto-refresh only within configured document/byte limits; larger corpus returns `docs_index_refresh_required` |
| R52 | `cargo fmt`, clippy, all targets | quality | Clean according to repository policy |

Tests cần real temporary repositories và real Markdown bytes. Cache concurrency/crash cases cần process-level tests. Ranking tests phải dùng fixture corpus và assert expected ordering classes/tie-break, không assert incidental raw floating score trừ engine-version-pinned unit tests.

## Definition of Done của slice

- [ ] Exact Slice 4 registry/schema được migrate idempotently sang retrieval-capable schema bằng known-predecessor validation, CAS/recovery và immutable event.
- [ ] Unknown schema/predecessor không bị overwrite.
- [ ] Retrieval config và per-document overrides typed, normalized và deterministic.
- [ ] Search/applicability authority policies được tách rõ; informational docs searchable nhưng không tự required.
- [ ] Registered eligible Markdown được parse offline bằng pure-Rust parser adapter.
- [ ] Section records có stable document ID, heading path, deterministic anchor/ref, exact line range, document hash và section hash.
- [ ] Preamble, duplicate headings, nested headings, fenced code và CRLF/LF có tested semantics.
- [ ] Oversized sections được chunk bounded, deterministic và không split fenced code.
- [ ] Heading rename tạo stale-ref error rõ; không silently resolve sai.
- [ ] Root/selected-area `_index.md` deterministic, marked generated, rebuildable và không thành writable truth.
- [ ] User-authored/unknown `_index.md` được preserve, không overwrite.
- [ ] Tantivy lexical index chạy offline và nằm sau typed engine interface.
- [ ] Query input không expose raw Tantivy syntax; query/result budgets được enforce.
- [ ] Index fields/boost/tokenizer config version hóa và tham gia fingerprint.
- [ ] Vietnamese, hyphenated identifier, dotted version và CJK boundary có defined/tested behavior; không overclaim semantic quality.
- [ ] `.pulse/cache/docs-search/` dùng immutable generation + atomic `CURRENT` publication.
- [ ] Concurrent reader không thấy mixed generation; concurrent writers không corrupt cache.
- [ ] Missing/stale/corrupt/incompatible cache được report/rebuild đúng và không mutate canonical docs.
- [ ] Incremental path reuse unchanged extracted records và re-extract changed docs only.
- [ ] Retrieval fingerprint derive từ relevant config/metadata/content, không machine path/timestamp.
- [ ] Xóa cache rồi rebuild giữ same fingerprint và equivalent fixture ranking semantics.
- [ ] `pulse docs index|status|search|get|tree` có stable human/JSON contracts.
- [ ] `search` trả bounded section/chunk metadata/snippets, không full document.
- [ ] `get` mặc định bounded và revalidates canonical current bytes; `--full` explicit.
- [ ] Search filters và `--work` adjustment explainable, capped và không override lifecycle/authority exclusions.
- [ ] Tree hoạt động từ registry khi cache absent.
- [ ] Retrieval eval fixtures đo Recall@K/MRR, exclusions, context bytes, latency và incremental behavior.
- [ ] Core fixtures pass top-K/budget expectations và zero must-exclude violations.
- [ ] Documentation receipt payload/manifest remains v1; payload v2 is still rejected.
- [ ] Valid pre-migration documentation receipts retain identical verification outcomes after registry v2 migration.
- [ ] Evidence receipt verification consumes a typed/version-aware docs registry boundary and ignores retrieval-only metadata.
- [ ] Retrieval-only document metadata edits do not bump receipt-bound `document_revision`; verification-relevant edits still do.
- [ ] Registered `AGENTS.md` and `PULSE.md` are first-class bounded retrieval inputs by default and appear in the Repository navigation area.
- [ ] Generated output documents require explicit indexing opt-in; generated navigation is never indexed.
- [ ] Auto-refresh cost limits are enforced and reference-fixture performance thresholds pass.
- [ ] CLI vẫn thin; parser/cache/index/search/get logic ở typed library modules.
- [ ] Không thêm embeddings, vector DB, SQLite, daemon, QMD runtime dependency hoặc full work packet.
- [ ] Rust format, clippy và full test suite sạch.

## Handoff sang Slice 6 — Shaping + Readiness Composition

Slice 6 có thể dùng:

```text
structural executability
+ implementation contract
+ shaping result/branch dispositions/authority
+ documentation impact
+ applicable document buckets
+ lexical section suggestions
+ required Decision/content references
= dispatch readiness + bounded execution packet inputs
```

Slice 6 mới sở hữu:

- shaping receipt/reference và critical ambiguity dispositions;
- `draft -> shaped` / `shaped -> ready` gating;
- destination, exit condition, bounded fog và decision/execution frontier;
- full `pulse work ready`;
- full `pulse work packet` với required/suggested section refs và read budget;
- readiness invalidation khi work, registry, document, section/extractor hoặc shaping revisions đổi.

Slice 5 chỉ cung cấp retrieval primitives và suggestion records; nó không quyết định semantic required sections cho every Ticket.

## Phase 2/3/4 follow-up

### Phase 2

- execution packet consumes required/applicable docs + section suggestions;
- assignment/handoff bind retrieval/document fingerprints;
- docs write candidates và close-gate references.

### Phase 3

- `pulse-docs-orient|impact|update|review|promote` capabilities;
- link check, command snippets, generated freshness runners;
- `pulse doctor` retrieval findings;
- Story QA/product-doc conflict checks.

### Phase 4

- reuse lexical engine/cache publication primitives for typed knowledge corpus;
- keep learning applicability/lifecycle/authority/result schema separate;
- add usage feedback and historical retrieval evals;
- semantic adapter spike only if lexical eval shows material recall gap.

## Risks và open questions cho review

1. **MSRV:** current crate declares Rust 1.78. Which comrak/Tantivy versions satisfy required APIs and MSRV? If not, is deliberate MSRV bump acceptable for Pulse distribution targets?
2. **Registry v2 migration entrypoint:** should exact-known migration happen automatically on mutating `docs index`/bootstrap, or require explicit `pulse docs migrate`? Revision semantics are fixed above and `--check` remains read-only.
3. **Future revision split:** Slice 5 keeps one receipt-bound `document_revision` and treats retrieval-only edits specially. If more non-verification metadata appears later, should a future schema introduce separate semantic/registry-record revisions rather than expanding special cases?
4. **Relevant registry hash:** excluding review policy from retrieval fingerprint improves incremental behavior, but state output stores owner/authority. Exact field set must be frozen and contract-tested.
5. **Informational docs:** default searchable aligns owner docs but creates noise risk. Should query default include informational or require a policy/config flag?
6. **Generated-doc defaults:** explicit per-document opt-in is fixed for Slice 5. Should future repositories be allowed to define a bounded generated-output policy default by path/kind, or must opt-in remain record-by-record?
7. **Section boundary:** parent intro-only indexing avoids body duplication but can reduce context for nested sections. Eval must compare intro-only vs subtree text.
8. **Setext headings:** comrak recognizes them though core contract only requires ATX. Should they receive normal stable refs or be treated as parser-compatible non-contract behavior?
9. **Anchor algorithm:** Unicode-preserving anchors are readable but normalization changes can break refs. Should accents be preserved or transliterated? Proposal preserves Unicode and versions algorithm.
10. **Chunk identity:** `@N` is simple but changes when earlier split boundaries move. Is content-derived chunk suffix worth complexity? Proposal chooses source-order ordinal and explicit staleness.
11. **Cache publication:** plain-text `CURRENT` pointer is portable and inspectable. On Windows, can open Tantivy readers prevent cleanup/rename? Keep prior generations and test process behavior.
12. **Projection consistency:** cache may become current while one `_index.md` remains stale after crash. Proposal treats projection state separately; should `index` exit fail until all projections write successfully even though search cache is usable?
13. **Auto-refresh defaults:** the cost-guard contract is fixed, but are the proposed defaults of 200 documents/20 MiB appropriate across fixture repositories, or should repository policy set lower defaults?
14. **Concurrent mutation:** optimistic capture/build/revalidate may repeatedly conflict in active repos. What retry/time budget is acceptable before returning `docs_index_inputs_changed`?
15. **Snippet source:** storing full section body in gitignored `sections.jsonl` simplifies snippets but duplicates corpus locally and may include sensitive text. Protected-path policy and cache permissions need explicit tests.
16. **Tokenizer:** Tantivy built-in tokenizers may not preserve identifiers exactly as required. Is a small custom tokenizer justified in Slice 5, or dual normalized/raw fields enough?
17. **Vietnamese/CJK:** exact-diacritic Vietnamese support is straightforward; accent-insensitive or CJK n-gram behavior may improve recall but changes noise/index size. Defer until fixtures demonstrate need?
18. **Raw scores:** Tantivy scores can vary across engine versions. Public tests should assert rank/reasons, not numeric score stability. Should JSON expose raw score at all or only rank + explain categories?
19. **`get` without cache:** should section refs be resolved by parsing one canonical document even when index missing? Proposal says yes for document/section get when registry resolves document ID.
20. **Path-range refs:** convenient for diagnostics but less stable than section refs. Should public CLI support them in Slice 5 or defer to avoid a second identity form?
21. **Eval command:** expose `pulse docs eval`/`index --eval` now, or keep fixture runner internal until harness CLI design stabilizes?
22. **Area scopes:** placing navigation scopes in registry envelope creates one more mutation surface. Could area summaries live in a separate generated/config file without creating a second truth?
23. **Repository-area ranking:** `AGENTS.md`/`PULSE.md` are now in scope by default. Should policy/map hits receive a small query-type boost for command/policy queries, or rely only on lexical fields and kind filters in Slice 5?
24. **Typed evidence boundary:** typed/version-aware registry loading is required before v2 migration. Should the interface live in `docs` as a narrow immutable snapshot or in a shared schema crate/module to prepare future consumers?
25. **Bench portability:** reference thresholds are fixed for the standard fixture, but CI hardware variance needs a documented benchmark environment/tolerance so performance gates remain meaningful.

## Không quyết định trong slice này

Slice này không chốt semantic/vector retrieval, work-packet query generation quality, shaping semantics, QA section applicability, actor authorization, docs close gate, knowledge corpus schema, cross-repository search, automatic summary generation hoặc rich query language.

Nó chỉ chốt deterministic path từ **registered current documentation** tới **bounded searchable sections** và **rebuildable lexical navigation**, để các readiness/execution layers sau không phải đọc full docs corpus hoặc phát minh section identity/cache semantics riêng.
