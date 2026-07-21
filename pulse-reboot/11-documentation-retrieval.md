# Documentation Navigation And Retrieval

[Trang vào](../PULSE_REBOOT.md) | [Bản đồ tài liệu](./README.md) | [Documentation system](10-documentation-system.md) | [Runtime harness](04-runtime-harness.md) | [Knowledge compounding](12-knowledge-compounding.md)

**Đọc khi:** cần biết Agent tìm đúng document/section mà không đọc toàn bộ docs corpus, index được build/cache thế nào và semantic search được thêm sau ra sao.
**Sở hữu:** generated navigation indexes, section extraction, lexical BM25 search, retrieval CLI, result packet, cache/fingerprint/staleness, work-packet integration, retrieval evals và optional semantic adapter.

## Khẳng định thiết kế

Pulse documentation retrieval dùng **progressive disclosure**:

```text
repository map / docs tree
  -> ranked section search
  -> bounded section get
  -> full document chỉ khi thật sự cần
```

Core v1 không nhúng toàn bộ documentation corpus vào Agent prompt và không yêu cầu Agent tự grep toàn bộ `docs/`. Core v1 cũng không kéo vector database, embedding model, LLM query expansion, reranker, MCP server hoặc daemon vào runtime.

Docs retrieval và knowledge recall có thể reuse section/lexical engine abstractions, nhưng không merge thành untyped corpus. `pulse docs` ưu tiên current authoritative truth; `pulse knowledge` ưu tiên reusable historical guidance với lifecycle/applicability riêng. Work packet aggregator preserve type, authority và reason khi combine results.

Nguyên tắc ngắn:

> **Search first, retrieve sections second, expand only on evidence.**

Pulse học từ QMD các primitive tốt: search/get tách biệt, path context, heading-aware chunks, field boosting, line-bound snippets, content hashes và rank fusion. Pulse không copy toàn bộ QMD stack vì repository docs retrieval cần nhỏ, deterministic và tích hợp ownership/applicability của Pulse.

## Reference evidence

Thiết kế này học có chọn lọc từ:

- QMD tách keyword `search`, semantic `vsearch`, hybrid `query` và explicit `get`; Agent nhận JSON/snippet trước khi lấy full content: [QMD README](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/README.md#L33-L69).
- QMD dùng hierarchical path context, chọn context cụ thể nhất theo path prefix: [example config](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/example-index.yml#L31-L50) và [implementation](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/collections.ts#L473-L509).
- QMD chunk Markdown theo heading/code-fence/paragraph boundaries: [breakpoint rules](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/store.ts#L110-L129) và [cutoff algorithm](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/store.ts#L195-L243).
- QMD dùng BM25 field weighting, line-bound snippets và RRF: [FTS ranking](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/store.ts#L3567-L3636), [snippet extraction](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/store.ts#L4544-L4627), [RRF](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/src/store.ts#L3982-L4025).
- QMD full stack kéo native SQLite/vector/model/grammar dependencies, nên không phù hợp Core v1: [package dependencies](https://github.com/tobi/qmd/blob/e428df76bc0274d9e93eb7ca3e95673315c42e90/package.json#L58-L78).
- MiniSearch cung cấp in-memory offline full-text, field boosting, prefix/fuzzy search, zero external dependencies và serialized index: [features](https://github.com/lucaong/minisearch/blob/3d239d1c3ae7aef1bf5d8945dd7b5f0709f646f5/README.md#L27-L57), [serialization](https://github.com/lucaong/minisearch/blob/3d239d1c3ae7aef1bf5d8945dd7b5f0709f646f5/src/MiniSearch.ts#L1498-L1544), [BM25+](https://github.com/lucaong/minisearch/blob/3d239d1c3ae7aef1bf5d8945dd7b5f0709f646f5/src/MiniSearch.ts#L2107-L2161).
- Knowledge Base Builder minh họa root/per-directory index + one-line summaries để Agent đi top-down mà không dùng vector DB: [progressive disclosure](https://github.com/shivdeepak/knowledge-base-builder/blob/cd565ded6b082ecf02ac1822c6a0935e8180890f/README.md#L26-L65).

Các reference này cung cấp bài học, không trở thành Pulse compatibility contract.

## Shared engine boundary với knowledge recall

Reusable primitives:

- Tantivy/BM25 engine wrapper.
- Content-hash keyed cache state.
- Bounded search/get contract.
- Snippet/token budgets.
- Deterministic rebuild/tie-break testing.

Không reusable như một schema chung:

- Docs section identity, authority, owner và applicability.
- Learning lifecycle, confidence, provenance, audience/moment và ratchet bucket.
- Ranking/filter fields và contradiction semantics.

Không có generic `pulse search everything` trong Core. Agent/context builder gọi typed query surfaces rồi compose bounded packet.

## Core v1 decision

Core v1 chốt:

- Canonical prose: normal Git files trong `docs/`, `AGENTS.md`, `PULSE.md`.
- Canonical metadata: `.pulse/docs/registry.json` theo [`10-documentation-system.md`](10-documentation-system.md).
- Human/cold-Agent navigation: generated `_index.md` projections.
- Retrieval unit: Markdown section, không phải full file.
- Search engine: in-process BM25+ bằng `tantivy` (pure-Rust) hoặc equivalent engine đã benchmark/contract-test tương đương. MiniSearch (JS) chỉ còn là reference lesson.
- Search cache: disposable, gitignored, content-hash/fingerprint keyed.
- Default search mode: `lexical`.
- Semantic/hybrid search: optional adapter sau Core v1.
- Không dùng SQLite cho docs search trong Core v1.

Technology choice `tantivy` (Rust) + `comrak` (Rust Markdown section parser) là implementation direction cho prototype, không phải public compatibility contract. Public contract là section-level lexical ranking, stable JSON output, deterministic rebuild và no-native-model dependency. Nếu prototype thay engine, acceptance semantics phải giữ.

## Ba lớp navigation và retrieval

### 1. Repository map

`AGENTS.md` chỉ route tới:

- `PULSE.md`.
- Root docs index.
- Architecture/product/domain/operations areas.
- `pulse docs` commands.

Nó không chứa full documentation catalog hoặc copied summaries của mọi file.

### 2. Generated navigation projection

`docs/_index.md` và selected per-directory `_index.md` giúp human/cold Agent browse theo area, document summary và link.

Projection này:

- Generated từ registry + document metadata.
- Không phải writable truth.
- Có marker cảnh báo không hand-edit.
- Xóa được và regenerate.
- Không chứa mọi section/chunk.
- Không được lớn thành một bản sao corpus.

### 3. Machine retrieval index

`.pulse/cache/docs-search/` chứa section records và serialized lexical index để `pulse docs search` trả ranked hits nhanh.

Cache:

- Gitignored.
- Disposable.
- Atomic replace.
- Key theo registry/index configuration/document hashes.
- Không cần thiết cho correctness vì có thể rebuild.

## Layout

```text
AGENTS.md
PULSE.md

docs/
  _index.md                         # generated root navigation projection
  product/
    _index.md                       # optional generated area index
    authentication.md
  architecture/
    _index.md
    authentication.md
  domain/
    token-lifecycle.md
  operations/
    auth-recovery.md

.pulse/
  docs/
    registry.json                   # canonical docs metadata
    schemas/
      document.schema.json
  cache/
    docs-search/
      state.json                    # fingerprint, engine, indexed document hashes
      sections.jsonl                # derived section records
      lexical-index.json            # serialized BM25 index
```

Per-directory `_index.md` chỉ materialize khi:

- Registry scope khai báo `materialize_index: true`.
- Hoặc directory vượt configured document threshold, mặc định đề xuất là 5.
- Hoặc repository policy yêu cầu human navigation cho area đó.

Không sinh `_index.md` ở mọi folder một cách máy móc.

## Document registry additions

Document record từ `10-documentation-system.md` bổ sung retrieval metadata tùy chọn:

```json
{
  "id": "DOC-AUTH-DOMAIN",
  "path": "docs/domain/token-lifecycle.md",
  "kind": "domain",
  "authority": "approved",
  "owner": "team:identity",
  "summary": "Token types, lifecycle transitions, error semantics and invariants.",
  "aliases": ["refresh tokens", "session credentials"],
  "scope": {
    "paths": ["src/auth/**"],
    "domains": ["authentication"],
    "work_labels": ["auth"]
  },
  "retrieval": {
    "index": true,
    "include_body": true,
    "materialize_index": false
  }
}
```

### Summary contract

- Document-level `summary` được khuyến nghị cho registered authoritative docs.
- Summary ngắn, một hoặc hai câu, mô tả nội dung và protected intent; không phải changelog.
- Summary được dùng trong `_index.md`, search ranking, result preview và work packet.
- Summary authored/approved cùng registry metadata, không tự sinh bằng LLM trong Core v1.
- Nếu thiếu summary, indexer có thể derive preview từ title + first meaningful paragraph nhưng `pulse doctor` có thể tạo advisory finding tùy policy.

### Aliases

Aliases chứa terminology hợp lệ mà code/user có thể dùng khác với docs wording. Không dùng aliases làm nơi nhồi keyword hoặc prompt injection. Duplicate/noisy aliases là quality finding.

### Scope-level context

Registry có thể khai báo area context:

```json
{
  "scopes": [
    {
      "path": "docs/architecture",
      "summary": "System boundaries, dependency direction and invariants.",
      "materialize_index": true
    },
    {
      "path": "docs/architecture/auth",
      "summary": "Authentication components, trust boundaries and token flow.",
      "materialize_index": false
    }
  ]
}
```

Khi một section match nhiều scopes, context cụ thể nhất theo longest path prefix được attach vào result.

## Generated `_index.md`

Ví dụ:

```markdown
# Documentation Index

> Generated by `pulse docs index`. Do not edit manually.

## Product

Current user-visible behavior and compatibility contracts.

- [Authentication](product/authentication.md)
  Login, refresh, expiry and compatibility behavior.

## Architecture

System boundaries, dependencies and cross-module invariants.

- [Authentication Architecture](architecture/authentication.md)
  Token flow, trust boundaries and error mapping.
```

Rules:

- Nội dung derive từ registry order/path/kind/summary.
- Stable deterministic ordering.
- Links repository-relative và portable.
- Generated marker bắt buộc.
- `pulse docs index --check` fail nếu projection stale.
- `_index.md` không được registry route như authoritative contract content trừ khi repository explicit đăng ký chính index như informational navigation.
- Projection generation không rewrite prose docs.

## Retrieval unit

### Section identity

Mỗi Markdown section tạo một derived record:

```json
{
  "section_id": "DOC-AUTH-DOMAIN#refresh-token-lifecycle",
  "document_id": "DOC-AUTH-DOMAIN",
  "path": "docs/domain/token-lifecycle.md",
  "document_title": "Token Lifecycle",
  "heading": "Refresh token lifecycle",
  "heading_path": ["Token Lifecycle", "Refresh token lifecycle"],
  "start_line": 12,
  "end_line": 44,
  "summary": "Refresh token transitions and failure semantics.",
  "body": "...",
  "content_hash": "sha256:...",
  "authority": "approved",
  "owner": "team:identity",
  "domains": ["authentication"],
  "aliases": ["refresh tokens", "session credentials"]
}
```

`section_id` derive deterministic từ document ID và normalized heading anchor. Duplicate anchors trong cùng file thêm stable ordinal suffix. Rename heading có thể đổi derived section ID; document ID vẫn ổn định.

### Markdown parsing

Core v1 parser hỗ trợ ATX headings `#` đến `######` và fenced code blocks.

Rules:

1. Document preamble trước heading đầu tiên là một section `#preamble` nếu có meaningful content.
2. Section bắt đầu tại heading và kết thúc trước heading ngang cấp hoặc cao hơn.
3. Searchable text include document title, heading ancestor path và body.
4. Fenced code block không bị split giữa chừng.
5. Frontmatter được parse cho identity/title nếu repository dùng, nhưng metadata canonical vẫn ở registry.
6. Generated/navigation docs bị exclude theo registry/config mặc định để tránh self-search noise.

### Oversized sections

Default target nên được benchmark, khởi điểm:

- Soft max khoảng 1,500-2,000 tokens hoặc 6,000-8,000 chars.
- Nếu section vượt ngưỡng, ưu tiên split tại nested heading.
- Sau đó split tại paragraph boundary.
- Chỉ dùng line boundary khi không còn lựa chọn.
- Overlap khoảng 100-150 tokens cho oversized chunks.
- Mỗi chunk giữ heading path, original section ref và exact line range.

Core v1 không cần Tree-sitter vì corpus mục tiêu là Markdown docs. Code snippets được giữ trong section và không trở thành separate code AST index.

## Lexical search engine

### Why BM25+

Repository docs có nhiều exact identifiers, commands, error names, domain terms và headings. BM25 cho precision tốt, deterministic, offline và không cần model. Field boosting cho phép heading/summary/aliases có trọng lượng lớn hơn body.

### Indexed fields

```text
document_title
heading
heading_path
summary
aliases
domains
path
body
```

Suggested initial boosts:

| Field | Boost |
|---|---:|
| `heading` | 5.0 |
| `document_title` | 4.0 |
| `heading_path` | 3.0 |
| `aliases` | 3.0 |
| `domains` | 3.0 |
| `summary` | 2.5 |
| `path` | 1.5 |
| `body` | 1.0 |

Các giá trị này là prototype defaults, phải được tune bằng retrieval eval; không phải public contract.

### Prefix và fuzzy behavior

- Exact lexical match luôn có trọng lượng cao nhất.
- Prefix search chỉ bật cho term đủ dài, đề xuất `>= 3` ký tự.
- Fuzzy search chỉ bật cho term dài, đề xuất `>= 6` ký tự và max edit ratio nhỏ.
- Identifiers chứa hyphen/dot như `TK-031`, `refresh-token`, `v2.1` cần normalization tests.
- Quoted phrases và negative terms có thể thêm sau; Core v1 ưu tiên stable simple query contract.
- Vietnamese/CJK/tokenization behavior phải có fixture eval trước khi claim support quality; fallback substring/alias matching có thể bổ sung mà không đổi CLI.

### Pulse metadata adjustment

Raw lexical rank được điều chỉnh nhẹ bằng metadata:

- Explicit Ticket/Story/Decision document reference.
- Registry path/domain/work-label applicability.
- Authority/freshness policy.
- Requested kind/domain filters.

Metadata không được làm một lexical-irrelevant doc đứng trên strong match chỉ vì cùng domain.

Suggested semantics:

- `retired`: excluded mặc định.
- `stale`: excluded hoặc demoted theo policy; result phải ghi rõ.
- `draft`: searchable chỉ khi `--include-draft` hoặc work explicitly references nó.
- `approved`: small authority boost.
- Explicit work reference: strongest metadata boost.

Score output phải được coi là engine-relative ranking signal, không phải xác suất correctness.

## Public CLI

```text
pulse docs index
pulse docs status
pulse docs search <query>
pulse docs get <document-or-section-ref>
pulse docs tree [path]
pulse docs applicable --work <id>
pulse docs validate
```

Không tạo top-level `pulse docs-search`; docs retrieval nằm dưới namespace `pulse docs`.

## `pulse docs index`

```text
pulse docs index
pulse docs index --changed
pulse docs index --rebuild
pulse docs index --check
pulse docs index --json
```

Behavior:

1. Validate docs registry và configured roots.
2. Resolve included/excluded docs.
3. Hash file content.
4. Reuse unchanged section records.
5. Parse changed documents thành sections.
6. Rebuild/update lexical index.
7. Generate selected `_index.md` projections.
8. Write cache/projections bằng atomic replace.
9. Emit stable status/fingerprint JSON.

`--check` là read-only: không mutate; exit non-zero nếu registry, cache hoặc generated projection stale/missing theo policy.

`--changed` chỉ reindex changed hashes; nếu engine serialization/config không compatible thì fallback full rebuild.

## `pulse docs status`

Human output tóm tắt:

```text
Documents: 63 registered, 61 indexed, 2 excluded
Sections: 412
Index: current (fingerprint sha256:...)
Engine: lexical/tantivy
Semantic: disabled
Warnings: 2 missing summaries, 1 stale generated projection
```

JSON:

```json
{
  "schema_version": 1,
  "engine": {
    "mode": "lexical",
    "name": "tantivy",
    "version": "..."
  },
  "documents": {
    "registered": 63,
    "indexed": 61,
    "excluded": 2,
    "stale": 0
  },
  "sections": 412,
  "fingerprint": "sha256:...",
  "cache_state": "current",
  "semantic": "disabled",
  "warnings": []
}
```

## `pulse docs search`

```text
pulse docs search "refresh token expiry"
pulse docs search "rollback migration" --kind operations
pulse docs search "authentication boundary" --domain authentication
pulse docs search "public error codes" --authority approved
pulse docs search "session lifecycle" --work TK-031
pulse docs search "rate limit" --limit 8 --json
pulse docs search "legacy behavior" --include-draft --include-stale
```

Default behavior:

- Incrementally refresh stale lexical cache unless `--no-refresh`.
- Search current/approved/informational docs.
- Exclude migration backups, retired docs và generated navigation projections.
- Return tối đa 8 section hits.
- Return summary/snippet/range, không full body.
- Include matching fields và ranking reason khi `--explain` hoặc JSON policy yêu cầu.

Human output:

```text
1. DOC-AUTH-DOMAIN › Refresh token lifecycle › Expired tokens
   docs/domain/token-lifecycle.md:31-44
   score: 0.91  authority: approved  owner: team:identity
   why: heading, domain=authentication, work scope src/auth/**
   TokenExpired represents a refresh token that was valid but is no longer...
```

JSON result contract:

```json
{
  "schema_version": 1,
  "query": "refresh token expiry",
  "index": {
    "fingerprint": "sha256:...",
    "stale": false,
    "mode": "lexical"
  },
  "results": [
    {
      "rank": 1,
      "score": 0.91,
      "section_ref": "DOC-AUTH-DOMAIN#expired-tokens",
      "document_id": "DOC-AUTH-DOMAIN",
      "path": "docs/domain/token-lifecycle.md",
      "heading_path": ["Refresh token lifecycle", "Expired tokens"],
      "range": {"start_line": 31, "end_line": 44},
      "summary": "Expired token semantics and transition rules.",
      "snippet": "TokenExpired represents...",
      "authority": "approved",
      "owner": "team:identity",
      "content_hash": "sha256:...",
      "matched_fields": ["heading", "body", "domains"],
      "applicability": {
        "work_id": "TK-031",
        "reasons": ["domain match", "source scope match"]
      }
    }
  ]
}
```

## `pulse docs get`

```text
pulse docs get DOC-AUTH-DOMAIN
pulse docs get DOC-AUTH-DOMAIN#expired-tokens
pulse docs get docs/domain/token-lifecycle.md:31-44
pulse docs get DOC-AUTH-DOMAIN#expired-tokens --max-lines 80 --json
pulse docs get DOC-AUTH-DOMAIN --full
```

Rules:

- Section ref trả exact section/chunk với line numbers và content hash.
- Document ID mặc định trả document summary + outline + bounded leading content, không full file nếu quá budget.
- `--full` là explicit opt-in.
- `--max-lines`/`--max-bytes` bảo vệ context budget.
- Nếu section anchor stale sau heading rename, CLI trả nearest current sections và non-zero/not-found semantics rõ; không silently lấy sai section.
- `get` đọc canonical file hiện tại, không trả body từ stale cache.

## `pulse docs tree`

```text
pulse docs tree
pulse docs tree docs/architecture
pulse docs tree --depth 2 --json
```

Tree derive từ registry/scopes, không cần parse full bodies. Nó trả area summaries, docs summaries, authority và links. Đây là navigation fallback khi user/Agent chưa có query đủ cụ thể.

## `pulse docs applicable`

```text
pulse docs applicable --work TK-031 --json
```

Kết quả gồm:

- Explicit required docs.
- Registry scope/domain matches.
- Excluded/stale docs và reasons.
- Suggested retrieval queries derive từ Ticket objective/acceptance/code anchors.
- Không nhúng full body.

`applicable` là graph/registry projection; `search --work` là lexical retrieval có applicability adjustment. Hai command không thay nhau.

## Cache, fingerprint và staleness

`state.json` minh họa:

```json
{
  "schema_version": 1,
  "engine": "tantivy",
  "engine_version": "...",
  "index_config_hash": "sha256:...",
  "registry_hash": "sha256:...",
  "documents": {
    "DOC-AUTH-DOMAIN": {
      "path": "docs/domain/token-lifecycle.md",
      "content_hash": "sha256:...",
      "section_count": 7
    }
  },
  "fingerprint": "sha256:..."
}
```

Fingerprint derive từ:

- Supported schema/index version.
- Search engine/config/tokenizer version.
- Registry retrieval metadata hash.
- Sorted included document content hashes.
- Generated projection configuration.

Search-time behavior:

- Cache current: search ngay.
- Cache missing: build on demand nếu policy/corpus budget cho phép.
- Cache stale: incremental refresh rồi search mặc định.
- `--no-refresh`: có thể search stale cache chỉ nếu policy cho phép; output bắt buộc cảnh báo và hashes.
- Strict/CI path dùng `pulse docs index --check`.
- Corrupt/incompatible cache bị bỏ và rebuild, không cố repair như truth.

Indexer dùng repository-scoped lock, temp file, fsync/atomic rename theo storage primitive chung. Concurrent readers tiếp tục dùng last complete index; không đọc partial write.

## Context budget contract

Retrieval API phải giúp Agent không tự bloat context:

- Search default limit: 8 hits.
- Snippet default: 3-6 lines hoặc khoảng 500 chars.
- `get` default: một section và bounded lines.
- Full document cần explicit `--full`.
- Work packet đưa summaries/section refs trước, không auto-inline tất cả hits.
- Packet có recommended initial read budget, ví dụ tối đa 4 sections/240 lines.
- Agent được search lại khi implementation discovery xuất hiện terminology mới.

Budget là guidance/policy, không thay thế required docs. Nếu required contract trải nhiều sections, packet phải chỉ rõ reason thay vì cắt im lặng.

## Work packet integration

`pulse work packet TK-031` resolve hai tầng:

### Deterministic required context

- Explicit Ticket/Story/Decision references.
- Required policy docs.
- Registry docs marked required bởi path/domain/risk rules.
- Document summary, section refs, content hashes và reason.

### Retrieval suggestions

Indexer tạo query terms từ:

- Ticket title/objective.
- Current/target behavior.
- Acceptance.
- Code anchors/path scopes.
- Domains/labels.
- Linked Decision titles.

Packet chỉ trả top section suggestions:

```json
{
  "documentation": {
    "required": [
      {
        "section_ref": "DOC-AUTH-DOMAIN#refresh-token-lifecycle",
        "content_hash": "sha256:...",
        "reason": "explicit Ticket reference"
      }
    ],
    "suggested": [
      {
        "section_ref": "DOC-AUTH-ARCH#error-mapping",
        "score": 0.78,
        "reason": "lexical + domain match"
      }
    ],
    "read_budget": {
      "recommended_sections": 4,
      "max_initial_lines": 240
    }
  }
}
```

Agent flow:

```text
read required sections
  -> inspect suggested summaries/snippets
  -> get selected sections
  -> search again only when needed
```

Work packet generation không fail chỉ vì suggested search không có hit, nhưng required explicit/applicable docs missing hoặc stale theo hard policy vẫn fail.

## Retrieval evaluation

Pulse phải eval retrieval quality, không chỉ test index code.

Fixture format:

```json
{
  "query": "what happens when a refresh token expires",
  "work_context": {
    "domains": ["authentication"],
    "paths": ["src/auth/**"]
  },
  "expected": {
    "top_k": [
      "DOC-AUTH-DOMAIN#expired-tokens",
      "DOC-AUTH-ARCH#error-mapping"
    ],
    "must_exclude": [
      "DOC-AUTH-OLD#legacy-refresh"
    ]
  }
}
```

Metrics:

- Recall@K cho expected sections.
- Mean Reciprocal Rank.
- Required-doc inclusion rate.
- Retired/stale/migration exclusion accuracy.
- Context bytes/lines retrieved trước first useful section.
- Search/index latency theo corpus size.
- Incremental refresh correctness.
- Vietnamese/identifier/tokenization fixture quality.

Không optimize chỉ một benchmark. Evals phải gồm exact identifier, natural-language paraphrase, path/domain constrained query, stale/retired exclusion và no-result behavior.

## Failure taxonomy

- `docs_index_missing`: index/cache không tồn tại và cannot build theo policy.
- `docs_index_stale`: fingerprint không khớp.
- `docs_index_corrupt`: serialized index/section records invalid.
- `docs_index_projection_stale`: `_index.md` khác deterministic projection.
- `docs_search_miss`: expected relevant section không vào top K.
- `docs_search_noise`: irrelevant/retired/generated docs dominate results.
- `docs_summary_missing`: registered doc thiếu required summary.
- `docs_anchor_stale`: stored section ref không resolve current content.
- `docs_tokenization_gap`: language/identifier không search đúng.
- `docs_context_bloat`: packet/search/get vượt budget không có reason.

Repeated retrieval failures tạo harness Ticket/eval trong cùng work graph.

## Optional semantic adapter

Semantic retrieval chỉ thêm sau khi lexical eval chứng minh có recall gap đáng kể.

Adapter contract minh họa:

```text
fingerprint() -> model/provider identity
embed(sections, hashes) -> disposable vector artifacts
search(query, filters, limit) -> ranked section refs
health() -> available/degraded/unavailable
```

CLI chuẩn bị mode:

```text
pulse docs search "..." --mode lexical
pulse docs search "..." --mode semantic
pulse docs search "..." --mode hybrid
```

Core v1 chỉ support `lexical`; unsupported mode trả capability error rõ, không silently fallback nếu caller yêu cầu semantic.

### Hybrid fusion

Khi semantic adapter tồn tại:

```text
lexical rank list
semantic rank list
explicit/applicability rank list
        |
        v
weighted Reciprocal Rank Fusion
```

Không cộng raw BM25 score với cosine similarity trực tiếp. RRF explain trace nên cho biết contribution từ từng list.

### QMD boundary

QMD có thể là optional external adapter/reference implementation vì đã có BM25, vectors, chunks, RRF và agent-friendly CLI. Pulse không phụ thuộc QMD để Core đúng. Adapter phải map QMD results về stable Pulse section refs, registry filters, authority và content hashes; QMD database/model state vẫn là disposable external cache.

Core không thêm ngay:

- Embedding model downloads.
- `node-llama-cpp`.
- `sqlite-vec`.
- Query expansion model.
- Cross-encoder/LLM reranker.
- MCP/HTTP daemon.
- Code AST grammars.

## Security và trust

- Search treats docs content as untrusted text; snippets không trở thành authority ngoài registry metadata.
- Exclude migration backups, vendored docs, generated outputs và secret paths bằng config trước indexing.
- Registry path validation ngăn traversal/symlink escape.
- Search output phân biệt content text với Pulse instructions/policy.
- Prompt-like text trong docs không được nâng authority chỉ vì lexical match.
- Cache không chứa unredacted secret files; indexer obeys protected-path policy.
- `--json` escape content an toàn và giữ stable schema.

## Core v1 acceptance scenarios

1. Agent tìm đúng section cho exact identifier mà không đọc full corpus.
2. Natural-language query trả relevant section trong configured top K trên fixture corpus.
3. Result luôn có document ID, heading path, exact line range và content hash.
4. `pulse docs get <section>` mặc định trả bounded section, không full file.
5. `--full` là explicit và vẫn tuân max bytes/policy.
6. Xóa docs-search cache rồi rebuild cho cùng fingerprint và equivalent ranked semantics.
7. Xóa generated `_index.md` rồi `pulse docs index` regenerate deterministic projection.
8. Retired, migration backup và generated navigation docs bị exclude mặc định.
9. Draft/stale docs chỉ xuất hiện theo policy/flags và được label rõ.
10. Ticket applicability cải thiện ranking nhưng không đè strong lexical relevance.
11. `pulse docs search` hoạt động offline, không tải model trong Core v1.
12. Changed document được incremental reindex; unchanged section records được reuse hợp lệ.
13. Corrupt/incompatible cache bị discard/rebuild, canonical docs không bị sửa.
14. Search result snippets không vượt default budget và trỏ đúng lines.
15. Work packet chứa required refs + suggested refs, không inline toàn bộ top hits.
16. Missing required doc làm ready/packet fail; no optional search hit chỉ là empty result.
17. Summary/index projection stale được `index --check` và doctor phát hiện.
18. Duplicate headings tạo stable unique section refs.
19. Heading rename làm stale ref fail rõ và trả suggestions, không silently lấy sai content.
20. Vietnamese, hyphenated identifier và dotted version fixtures có defined tested behavior.
21. Semantic adapter unavailable không làm lexical mode hỏng.
22. Hybrid mode, khi có, dùng RRF và expose per-list explain trace.
23. Search cache và `_index.md` không trở thành writable source of truth.
24. Retrieval eval đo top-K quality và context bytes, không chỉ command exit code.

## Deferred after Core v1

- Learned query expansion.
- LLM/cross-encoder reranking.
- Automatic LLM summaries.
- Cross-repository docs federation.
- Code + docs unified semantic index.
- GraphRAG/entity extraction.
- Long-lived search daemon/MCP server.
- Automatic ownership inference từ content.
