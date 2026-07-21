# Phase 1 — Slice 4: Document Registry + Applicable-Doc Projection

> Trạng thái: **proposal để review**, chưa phải work contract hay compatibility contract.
> Tiền đề: [`phase1-slice3-evidence-receipts.md`](phase1-slice3-evidence-receipts.md) đã hoàn thành và cung cấp immutable receipt identity, content-addressed artifacts, source/content/work bindings và documentation receipt foundation.
> Sở hữu: implementation strategy cho lát cắt Phase 1 tiếp theo: canonical Document Registry, document identity/lifecycle/authority metadata, Ticket documentation-impact posture, deterministic applicable-doc projection và registry-aware documentation receipt validation.
> Tham chiếu normative: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`02-work-graph.md`](../pulse-reboot/02-work-graph.md), [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md), [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md), [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md), [`11-documentation-retrieval.md`](../pulse-reboot/11-documentation-retrieval.md).

## Trạng thái đã verify trước proposal này

Repository hiện tại đã có implementation Rust cho Slice 1–3:

- sharded work graph, CAS, lock, crash recovery và immutable events;
- lifecycle, supersession, structural executability, neighborhood/affected/roll-up projections;
- evidence artifacts/receipts và supersession reconciliation receipt;
- source/content/work-revision binding validation.

Tại thời điểm viết proposal, `cargo test --all-targets` pass toàn bộ test suites hiện có, gồm storage, process concurrency, lifecycle, read models, workgraph transaction và evidence receipt integration. Vì vậy slice kế tiếp đúng theo handoff trong Slice 3 là **Document Registry + Applicable-Doc Projection**, không phải thêm một evidence format khác hoặc nhảy thẳng sang BM25 retrieval.

Việc test hiện tại pass chỉ xác nhận implementation state của Slice 1–3. Nó không biến các proposal thành public compatibility contract và không có nghĩa Phase 1 đã hoàn thành.

## Vị trí của slice trong Pulse Reboot

Slice 3 đã chứng minh một documentation receipt có thể bind exact source commit và exact file bytes, nhưng chưa biết:

- `proposed_document_id` có phải canonical document identity hay không;
- path hiện tại có thuộc document đó hay chỉ là path caller tự khai;
- document là product, architecture, domain, operations, reference hay generated;
- document có owner/authority/review policy nào;
- document còn current, đã stale, retired hay superseded;
- document có applicable cho Ticket nào;
- Ticket đã khai báo documentation impact `required`, `none`, `deferred` hay vẫn `unknown`;
- receipt đã cover đúng registered documents và declared policy checks hay chưa.

Slice 4 thêm machine-readable routing/ownership plane nhỏ dưới `.pulse/docs/` và nối plane đó với work graph/evidence plane hiện có.

Slice tập trung vào năm capability:

1. canonical registry có revision CAS và deterministic validation;
2. stable document identity tách khỏi path;
3. document lifecycle/authority/scope đủ để include hoặc exclude context đúng;
4. Ticket documentation-impact posture và routing context có mutation/audit rõ;
5. `pulse docs applicable --work` cùng registry-aware documentation receipt validation.

Slice 4 **không** parse Markdown thành sections, không build `_index.md`, không kéo Tantivy/comrak và không mở full readiness. Nó trả document-level refs/content hashes; section-level progressive retrieval thuộc Slice 5.

## Nguyên tắc

- Durable prose vẫn là normal Git files trong `docs/`, `AGENTS.md`, `PULSE.md`; registry chỉ giữ identity, ownership, applicability và policy metadata.
- `.pulse/docs/registry.json` là **một writable canonical registry**. `docs/manifest.json`, nếu có, chỉ là generated projection và không được edit như nguồn thứ hai.
- Registry không liệt kê mọi Markdown file. Chỉ register docs cần routing, authority, generated freshness hoặc durable contract identity.
- Stable document ID không derive hoàn toàn từ path. Rename path giữ nguyên ID và được audit bằng registry CAS mutation.
- Work graph vẫn là source of truth cho Ticket documentation-impact state. Không parse free-form `ticket.md` ở mỗi query để suy ra gate metadata.
- Applicability là deterministic projection từ explicit references + typed work routing context + registry scope. Lexical relevance chưa thuộc slice này.
- Authority, lifecycle và applicability là ba chiều khác nhau; không gộp thành một `active: true` mơ hồ.
- Retired, superseded, stale và migration-backup material không được route như current truth.
- `none` và `deferred` không phải escape hatch. Chúng cần rationale/references có schema, nhưng semantic honesty và authority vẫn do reviewer/human/policy plane đánh giá.
- Documentation receipt giữ immutable payload cũ; registry-aware verification resolve current registry state thay vì rewrite receipt.
- Kernel validate mechanics, references, hashes và declared policy coverage. Kernel không tự kết luận prose đúng, owner đã review thật hay hai documents có semantic contradiction.
- Mọi mutation reuse repository write fence, expected-revision CAS, canonical JSON, transaction recovery và immutable event primitive hiện có.

## Mục tiêu

Triển khai documentation registry/applicability layer để có thể:

- bootstrap `.pulse/docs/registry.json` và `document.schema.json` mà không overwrite unknown contract;
- register, show, list, CAS-edit, retire và supersede document records;
- validate unique document ID/path, safe paths, source hierarchy, owner, authority, lifecycle và generated contract;
- preserve document identity qua rename;
- exclude migration backup, retired, superseded, stale và generated-navigation material khỏi current routing;
- record Ticket documentation impact bằng typed graph metadata và expected-revision CAS;
- derive `required`, `optional`, `write_candidates` và `excluded` documents cho một work item;
- explain từng applicability/exclusion bằng stable reason codes;
- detect missing required document, missing owner, ambiguous scope và stale registry path;
- nâng `documentation_validation` receipt từ content-only validation lên canonical document-ID/path/lifecycle/policy-aware validation;
- emit immutable events cho registry và documentation-impact mutations;
- giữ extension point rõ cho Slice 5 section extraction/search và Slice 6 readiness composition.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

- **#19, foundation:** implementation Ticket có typed documentation impact; `unknown` được report là gate gap, `none` cần rationale. Slice chưa mở transition `ready`.
- **#20:** applicable-doc projection route đúng current docs và exclude migration backup/retired docs.
- **#22, registry extension:** documentation receipt invalid/ineligible khi document identity/path/lifecycle không khớp registry, ngoài source/content staleness đã có từ Slice 3.
- **#26:** offline query được registry ID/path/kind/owner/authority/scope và applicability.
- **#29, registry subset:** retired/stale/migration/generated-navigation docs được exclude hoặc label đúng policy; search-index behavior defer Slice 5.

### Decisions liên quan

- D-02, D-06, D-07.
- D-18 đến D-25.
- D-26 đến D-34.
- D-35 đến D-40 chỉ ở boundary: Slice 4 tạo registry identity/input cho retrieval nhưng không implement section search.

### Slice exit

Slice hoàn thành khi document identity, registry mutation/validation, Ticket documentation impact, applicable-doc routing và registry-aware receipt verification deterministic/recoverable.

Slice exit **không** đồng nghĩa:

- `_index.md` hoặc docs search cache đã tồn tại;
- Agent đã search/get được section;
- semantic contradiction đã được resolve;
- Ticket được transition sang `ready`;
- docs close gate hoặc generated freshness runner đã hoàn chỉnh;
- Phase 1 hoặc Core v1 hoàn thành.

## Non-goals

- Markdown heading extraction, section identity, snippets hoặc line ranges.
- Generated root/per-area `_index.md`.
- Tantivy BM25, tokenizer, fuzzy/prefix search, retrieval eval hoặc context-budget ranking.
- `pulse docs search|get|index|status|tree` đầy đủ; Slice 4 chỉ sở hữu registry-level `list|show|applicable|validate` và mutation cần thiết.
- Full `pulse work packet`.
- Shaping/readiness composition, decision/execution frontier hoặc transition `draft -> shaped -> ready`.
- Parse toàn bộ `ticket.md` thành implementation contract.
- Semantic contradiction detection giữa docs, Decisions, code và QA baseline.
- CODEOWNERS integration hoặc external team-directory resolver.
- Cryptographic authority/signature hoặc Agent Registry.
- Chạy link checker, command snippets, generated freshness command hoặc external URL validation; slice chỉ validate declared metadata/contracts.
- Brownfield automatic move/merge/rewrite. Chỉ support registry bootstrap, safe registration và exclusion của backup namespace.
- Register mọi Markdown file tự động.
- Document receipt TTL/ancestor policy hoàn chỉnh.
- Cross-repository docs graph hoặc remote docs source.
- Knowledge store/applicability; docs và learning giữ typed planes riêng.

## Repository layout

```text
AGENTS.md
PULSE.md

docs/
  product/
  architecture/
  domain/
  operations/
  reference/
  generated/
  _index.md                         # defer Slice 5; nếu tồn tại phải treated as generated navigation
  manifest.json                    # optional generated projection, không writable truth

.pulse/
  docs/
    registry.json                  # tracked canonical metadata
    schemas/
      document.schema.json

  workgraph/
    nodes/
      TK-031.json                  # documentation impact + routing context

  evidence/
    receipts/
      rcpt_01J....json

  events/
    2026-07-23/
      evt_01J....json

  migrations/
    docs-backups/
      mig_01J.../
        manifest.json
        ...original paths...

  runtime/
    locks/
      workgraph.lock
    transactions/
      txn_01J....json
```

Ownership:

- `.pulse/docs/registry.json` và schema là tracked canonical metadata.
- Registered document content vẫn ở normal repository paths.
- `docs/manifest.json` chỉ được phép là generated projection từ canonical registry; Slice 4 chưa bắt buộc materialize projection đó.
- `.pulse/migrations/docs-backups/**` không bao giờ là current documentation source và không được register/routed.
- Registry event là audit evidence, không phải registry truth thứ hai.
- Runtime intent/cache không tham gia document identity.

## Docs manifest/registry envelope

Proposal dùng một canonical file nhỏ có revision CAS:

```jsonc
{
  "schema_version": 1,
  "revision": 7,
  "repository_id": "repo_01J...",
  "documents": [
    {
      "id": "DOC-AUTH-DOMAIN",
      "revision": 3,
      "path": "docs/domain/token-lifecycle.md",
      "kind": "domain",
      "authority": "approved",
      "lifecycle": "current",
      "owner": "team:identity",
      "summary": "Token types, lifecycle transitions, error semantics and invariants.",
      "aliases": ["refresh tokens", "session credentials"],
      "scope": {
        "paths": ["src/auth/**"],
        "domains": ["authentication"],
        "work_labels": ["auth"]
      },
      "review_policy": "independent",
      "verification_profile": "domain-doc",
      "generated": null,
      "superseded_by": null
    }
  ]
}
```

### Envelope rules

- `repository_id` phải khớp evidence manifest identity từ Slice 3; không tạo docs-plane repository identity thứ hai.
- `revision` là expected-revision CAS của toàn registry file.
- Documents serialize theo lexical `id`; aliases/scopes được normalize/sort deterministic.
- Registry fingerprint derive từ canonical registry bytes + document schema hash, không include document content bytes. Applicability output thêm per-document content hash đọc từ current file.
- Unknown schema/predecessor không được silently overwrite.
- Registry bootstrap có thể tạo empty `documents: []`, revision `1`, dùng repository ID hiện có; nếu evidence plane chưa bootstrap thì docs bootstrap gọi shared repository-identity primitive thay vì phát minh ID khác.

### Vì sao dùng một registry file

Owner document đã chốt `.pulse/docs/registry.json` là canonical registry. Registry dự kiến nhỏ và thay đổi ít hơn graph; một file giúp uniqueness của ID/path và offline inspection đơn giản.

Trade-off:

- concurrent registry writers serialize và CAS conflict ở file-level;
- document content writers không bị serialize bởi registry revision;
- nếu benchmark/brownfield corpus chứng minh registry file thành hotspot, sharding cần Decision/migration riêng, không tự đổi layout trong Slice 4.

## Document record contract

### Identity

- `id`: stable uppercase slug, proposal pattern `^DOC-[A-Z0-9][A-Z0-9-]{2,63}$`.
- ID do caller chọn có chủ đích; Slice 4 không auto-number vì document identity thường mang domain meaning.
- Rename path giữ ID, bump document revision và registry revision.
- ID không reuse sau retire/supersede trong normal API.

### Kind

Initial enum:

```text
repository_map
policy
product
architecture
domain
operations
reference
decision_projection
generated
informational
```

Rules:

- `AGENTS.md` chỉ register với `repository_map`.
- `PULSE.md` chỉ register với `policy`.
- Work prose dưới `works/**` không được register như durable docs.
- Decision projection phải reference canonical Decision work item khi policy yêu cầu; Slice 4 chỉ validate typed optional reference nếu có.
- Repository-specific kind extension defer; không nhận arbitrary string rồi mất routing semantics.

### Authority

```text
draft
approved
informational
generated
```

Authority trả lời tài liệu có sức nặng nào, không trả lời freshness/currentness.

- `approved`: route như authoritative context khi lifecycle current.
- `informational`: route optional, không được tự thắng product/Decision contract.
- `draft`: exclude mặc định, trừ explicit work reference hoặc include flag; luôn label draft.
- `generated`: authority thuộc source/generator contract, không đồng nghĩa output đang fresh.

### Lifecycle

```text
current
suspected_stale
stale
retired
superseded
```

- `current`: eligible cho routing theo authority/scope.
- `suspected_stale`: optional/excluded theo risk policy; Slice 4 mặc định exclude khỏi `required` và label finding.
- `stale`: exclude khỏi current context.
- `retired`: giữ identity/history, exclude.
- `superseded`: phải có valid `superseded_by`, exclude old document và route replacement nếu replacement current/applicable.

`draft` không phải lifecycle; nó là authority. Một draft document có lifecycle `current` nghĩa là file hiện tồn tại nhưng chưa approved.

### Owner

Owner là typed string tối thiểu:

```text
human:<id>
team:<id>
role:<id>
system:<id>
```

Slice 4 validate syntax/non-empty, không resolve external directory. Approved product/architecture/domain/policy document thiếu owner là hard registry validation error; informational low-risk doc thiếu owner có thể là warning chỉ khi repository policy cho phép. Proposal mặc định yêu cầu owner cho mọi registered document để contract đơn giản.

### Summary và aliases

- `summary`: non-empty, bounded, proposal max 500 UTF-8 chars.
- `aliases`: unique normalized strings, proposal max 32 entries × 120 chars.
- Slice 4 dùng summary cho list/applicable output; Slice 5 dùng nó cho `_index.md` và lexical boosting.
- Aliases không tham gia lexical search trong Slice 4; chúng chỉ được validate/store.

### Scope

```jsonc
{
  "paths": ["src/auth/**", "tests/auth/**"],
  "domains": ["authentication"],
  "work_labels": ["auth", "public-api"]
}
```

Rules:

- Mỗi dimension optional; empty scope nghĩa là chỉ explicit reference, không global applicability.
- Path patterns là repository-relative glob subset được version hóa; reject absolute path, `..`, symlink-dependent pattern và protected backup namespace.
- Domain/label là normalized slugs.
- Scope match là OR trong cùng dimension, và any matched dimension tạo applicability reason; explicit work reference luôn mạnh hơn inferred scope.
- Document có scope toàn repository phải dùng explicit pattern `**` hoặc dedicated policy flag, không dựa vào empty scope.

### Review policy

```text
none
light
standard
independent
human
```

Slice 4 chỉ lưu/validate policy và map nó sang required declared check kinds cho documentation receipt. Nó chưa biết actor có thực sự independent/authorized hay không.

### Generated contract

Authored document dùng `generated: null`.

Generated document:

```jsonc
{
  "generated": {
    "sources": ["schemas/public-api/**"],
    "command": "cargo xtask docs-api",
    "outputs": ["docs/generated/api/**"],
    "editable": false,
    "freshness_check": "cargo xtask docs-api --check"
  }
}
```

Slice 4 validate:

- non-empty sources/outputs/command;
- registered path nằm trong outputs;
- paths safe;
- `authority=generated` và `kind=generated` nhất quán;
- generated navigation (`docs/**/_index.md`) mặc định không register như authoritative content.

Slice 4 không execute generator/freshness command.

### Supersession

- Lifecycle `superseded` yêu cầu `superseded_by` tới document ID khác tồn tại.
- Non-superseded document không được có `superseded_by`.
- Supersession chain không cycle và endpoint không retired/stale nếu được route như replacement.
- Supersession mutation old+replacement validation + registry event là một registry-file CAS mutation, không cần multi-target canonical docs mutation vì content files không tự move/rewrite.
- Slice không tự copy scope/owner/path từ old sang replacement.

## Registry mutation model

### Commands

```text
pulse docs register --file <document-record.json>
  --expected-registry-revision <n>
  --actor <actor>
  [--json]

pulse docs edit <document-id>
  --expected-registry-revision <n>
  --expected-document-revision <n>
  --patch <typed-patch.json>
  --actor <actor>
  [--json]

pulse docs retire <document-id>
  --expected-registry-revision <n>
  --expected-document-revision <n>
  --reason <text>
  --actor <actor>
  [--json]

pulse docs supersede <old-id> --by <replacement-id>
  --expected-registry-revision <n>
  --expected-document-revision <n>
  --reason <text>
  --actor <actor>
  [--json]
```

Mutation API là typed; không expose arbitrary JSON Patch cho immutable fields.

### Mutation protocol

```text
1. acquire repository WriteGuard
2. recover/refuse unresolved transaction
3. load evidence repository identity + registry + schema
4. compare expected registry/document revisions
5. apply typed mutation in memory
6. validate full registry + referenced content paths
7. set document revision/registry revision/timestamp context if schema includes timestamps
8. canonicalize registry bytes and prepare semantic event
9. commit registry target + event bằng single-target transaction primitive
10. release guard
```

Events:

```text
docs.document.registered
docs.document.updated
docs.document.retired
docs.document.superseded
```

Event payload giữ document ID, before/after revisions, changed field names, reason và registry fingerprints; không duplicate full registry.

### Rename semantics

Path rename là explicit typed edit:

- new path phải tồn tại và safe;
- old path có thể còn tồn tại nhưng registry validate cảnh báo duplicate durable truth nếu cả hai được registered/current;
- ID giữ nguyên;
- old documentation receipts path-bound trở thành stale cho current bytes/path, đúng theo Slice 3;
- future receipt dùng same stable document ID với new path/content binding;
- Slice 4 không tự `git mv` file.

## Ticket documentation-impact contract

### Vì sao cần machine metadata

`works/<TK>/ticket.md` vẫn là human-facing implementation brief, nhưng `pulse docs applicable`, future ready gate và work packet không được parse prose bằng heuristic. Vì vậy Ticket node bổ sung typed metadata tối thiểu.

Đề xuất optional field; missing field derive thành `unknown` để existing nodes migrate an toàn:

```jsonc
{
  "documentation": {
    "impact": {
      "posture": "required",
      "rationale": "Public refresh-token error behavior changes.",
      "required_documents": ["DOC-AUTH-DOMAIN", "DOC-AUTH-PRODUCT"],
      "deferred_to": []
    },
    "routing": {
      "paths": ["src/auth/**", "src/http/**"],
      "domains": ["authentication"],
      "labels": ["auth", "public-api"]
    }
  }
}
```

### Posture vocabulary

```text
unknown
required
none
deferred
investigate
```

Rules:

- missing block = `unknown`.
- `required`: ít nhất một required document hoặc rationale giải thích missing-doc gap cần tạo; proposal mặc định yêu cầu `required_documents` non-empty cho normal implementation Ticket.
- `none`: rationale non-empty; `required_documents` và `deferred_to` empty.
- `deferred`: rationale non-empty và ít nhất một linked follow-up work item tồn tại, non-terminal hoặc accepted per future policy.
- `investigate`: chỉ hợp lệ cho Discovery/Spike subtype khi subtype tồn tại; trước subtype schema Slice 6, caller phải cung cấp typed provisional work-role assertion hoặc command reject. Proposal ưu tiên defer public `investigate` mutation nếu work-role chưa canonical.
- `unknown` không được caller dùng để “set complete”; nó là default/gap state.

Slice 4 không tự đánh giá `none` có trung thực với diff hay defer có authority. Projection trả `policy_status=not_evaluated` và future readiness composer quyết định.

### Routing context

- `paths`: source/test/config paths hoặc globs mà Ticket dự kiến ảnh hưởng; không phải writable scope authority.
- `domains`: product/domain slugs.
- `labels`: routing labels, không dùng làm priority.
- Explicit required document IDs là strongest signal.
- Parent inheritance chưa tự động copy vào Ticket; applicable query có thể đọc ancestors và label reason `inherited_from_parent` khi parent có compatible routing metadata trong future schema. Slice 4 initial implementation chỉ dùng subject metadata + explicit document refs để tránh hidden inheritance contract, trừ khi review khóa parent routing fields cùng slice.

### Mutation command

```text
pulse docs impact <ticket-id>
  --expected-revision <n>
  --posture <required|none|deferred>
  [--rationale <text>]
  [--required-doc <document-id>]...
  [--deferred-to <work-id>]...
  [--path <glob>]...
  [--domain <slug>]...
  [--label <slug>]...
  --actor <actor>
  [--json]
```

Mutation:

- chỉ áp dụng cho Ticket;
- dùng node expected-revision CAS;
- validate document/work references;
- bump node revision và emit `work.documentation_impact.updated`;
- không transition status;
- làm graph fingerprint/cache stale theo node content change;
- không mutate `ticket.md` tự động. Human/Agent capability phải giữ prose và metadata aligned; Slice 4 `docs validate` phát hiện presence/reference mismatch cơ học khi có declared artifact format, semantic sync sâu defer Slice 6.

## Applicable-doc projection

### Command

```text
pulse docs applicable --work <id> [--include-draft] [--include-stale] [--json]
```

Default query giữ repository guard xuyên recovery + graph/registry/content read để không quan sát half-applied metadata mutation.

### Inputs

- work item kind/status/revision;
- Ticket documentation impact và routing context;
- explicit required document IDs;
- registry document records;
- current document file existence/content hash;
- document authority/lifecycle/supersession;
- protected path/exclusion policy.

Slice 4 không đọc full Markdown body và không lexical-rank.

### Output

```jsonc
{
  "schema_version": 1,
  "work": {
    "id": "TK-031",
    "revision": 4,
    "documentation_posture": "required"
  },
  "registry": {
    "revision": 7,
    "fingerprint": "sha256:..."
  },
  "required": [
    {
      "id": "DOC-AUTH-DOMAIN",
      "path": "docs/domain/token-lifecycle.md",
      "kind": "domain",
      "authority": "approved",
      "owner": "team:identity",
      "summary": "Token types and lifecycle invariants.",
      "content_hash": "sha256:...",
      "document_revision": 3,
      "reasons": ["explicit_required_document", "domain_scope_match"]
    }
  ],
  "optional": [],
  "write_candidates": [
    {
      "id": "DOC-AUTH-DOMAIN",
      "reasons": ["impact_required", "explicit_required_document"]
    }
  ],
  "excluded": [
    {
      "id": "DOC-AUTH-OLD",
      "path": "docs/domain/token-lifecycle-old.md",
      "reason_codes": ["document_superseded"],
      "replacement": "DOC-AUTH-DOMAIN"
    }
  ],
  "gate": {
    "status": "incomplete",
    "reason_codes": [],
    "policy_status": "not_evaluated"
  }
}
```

### Bucket rules

#### Required

Document vào `required` khi:

- Ticket `required_documents` explicit reference nó; hoặc
- repository policy record đánh dấu global required cho matching scope trong future extension.

Slice 4 initial contract không tự nâng mọi path/domain scope match thành required. Scope match mặc định vào `optional`; điều này tránh kernel biến broad glob thành hard gate ngoài ý muốn.

Explicit required document chỉ route được nếu:

- ID tồn tại;
- path/content tồn tại;
- lifecycle current;
- authority approved/generated theo policy;
- không nằm trong protected backup/generated-navigation namespace.

Nếu không, output gate `incomplete` với reason như `required_document_missing`, `required_document_stale`, `required_document_not_authoritative`.

#### Optional

Current documents match path/domain/label scope nhưng không explicit required đi vào `optional`, deterministic sort theo:

1. explicit non-required reference nếu sau này có;
2. number/type of scope matches;
3. authority class;
4. document ID tie-break.

Đây không phải lexical relevance score và không được render như search ranking probability.

#### Write candidates

- posture `required`: explicit required docs mặc định là write candidates;
- scope-matched docs có kind phù hợp với changed surface có thể là candidates, nhưng output phải ghi inferred reason;
- posture `none`: empty, trừ conflict/finding;
- posture `deferred`: current Ticket không có required write candidate nhưng output link follow-up work;
- Slice 4 không tự sửa document.

#### Excluded

- lifecycle `retired`, `superseded`, `stale`;
- `suspected_stale` theo default strict routing;
- authority `draft` nếu không include/explicit policy;
- migration backup;
- generated navigation `_index.md`;
- unsafe/missing path;
- scope non-match chỉ xuất hiện trong excluded khi caller yêu cầu explain-all; default output không cần list toàn registry noise.

### Supersession routing

Nếu explicit old document đã superseded:

- old vào `excluded` với replacement;
- replacement được evaluate lại;
- replacement không tự trở thành `required` nếu path/authority/current state invalid;
- output reason chain rõ, không silently đổi ID khiến caller tưởng original reference vẫn current;
- future mutation nên update Ticket explicit reference sang replacement qua audited docs-impact edit.

### Determinism

- Mọi buckets sort theo document ID sau priority class.
- Reasons sort theo fixed reason precedence.
- Content hash là exact current file bytes.
- Cùng graph revision + registry fingerprint + document bytes cho equivalent output.
- Không cache trong Slice 4; scan nhỏ đủ correctness. Slice 5 cache/index dùng fingerprint riêng.

## Registry-aware documentation receipt validation

Slice 3 payload dùng `proposed_document_id`. Slice 4 cần deliberate schema evolution thay vì reinterpret field im lặng.

### Payload v2 đề xuất

```jsonc
{
  "payload_version": 2,
  "documents": [
    {
      "document_id": "DOC-AUTH-DOMAIN",
      "document_revision": 3,
      "path": "docs/domain/token-lifecycle.md",
      "content_hash": "sha256:...",
      "result": "passed"
    }
  ],
  "checks": [
    {"kind": "link_check", "result": "passed", "artifact": null},
    {"kind": "semantic_review", "result": "passed", "artifact": "sha256:..."}
  ]
}
```

Rules:

- New `documentation_validation` receipts dùng payload v2.
- Historical v1 receipts vẫn integrity-verify theo schema cũ nhưng registry status là `legacy_unresolved` hoặc resolve conservatively bằng exact path only; không rewrite receipt.
- `document_id` phải tồn tại và revision/path/content hash khớp bound snapshot.
- Receipt document path/hash phải có matching content binding như Slice 3.
- Current verification detect registry revision/path/lifecycle changes.
- Rename path làm receipt cũ không current cho new path dù stable document ID giữ nguyên; receipt vẫn historical-valid.
- Retired/superseded/stale document receipt không gate-eligible cho current document state.
- Generated doc receipt phải include declared freshness check kind khi policy yêu cầu; Slice 4 chỉ validate declaration/artifact, chưa chạy command.

### Validation report evolution

Đề xuất bump report schema:

```jsonc
{
  "schema_version": 2,
  "receipt_id": "rcpt_01J...",
  "receipt_hash": "sha256:...",
  "integrity": {"status": "valid", "reason_codes": []},
  "bindings": {"status": "current", "reason_codes": []},
  "registry": {"status": "current", "reason_codes": []},
  "policy": {
    "status": "structurally_satisfied",
    "reason_codes": ["authority_not_evaluated"]
  },
  "authorization": {
    "status": "not_evaluated",
    "reason_codes": ["authority_resolver_unavailable"]
  },
  "gate_eligible": false
}
```

`policy=structurally_satisfied` chỉ nghĩa required declared check kinds/profile references có mặt và pass. Nó không chứng minh reviewer independent/authorized.

`gate_eligible` vẫn false nếu review policy cần authority mà Slice 4 chưa resolve. Slice 6/Phase 2 close-gate consumer mới compose actor authority và work/source scope.

### Review-policy check matrix ban đầu

| Policy | Required declared checks | Slice 4 conclusion |
|---|---|---|
| `none` | none | structurally satisfied; semantic correctness not evaluated |
| `light` | `content_review` hoặc repository-defined exact profile | structurally satisfied nếu declared check pass |
| `standard` | `link_check`, `semantic_review` | structural only; authority not evaluated |
| `independent` | `link_check`, `semantic_review`, independent actor requirement | structural checks validate; independence unresolved |
| `human` | human approval reference | unresolved nếu chưa có authority resolver |

Exact extensible profile schema có thể thay matrix cứng trước implementation review. Không được chấp nhận arbitrary check string như đủ policy nếu registry profile không khai báo nó.

## CLI surface của slice

```text
pulse docs register --file <record.json> --expected-registry-revision <n> --actor <actor>
pulse docs edit <id> --patch <typed-patch.json> --expected-registry-revision <n> --expected-document-revision <n> --actor <actor>
pulse docs retire <id> --reason <text> --expected-registry-revision <n> --expected-document-revision <n> --actor <actor>
pulse docs supersede <old-id> --by <new-id> --reason <text> --expected-registry-revision <n> --expected-document-revision <n> --actor <actor>

pulse docs list [--kind <kind>] [--authority <authority>] [--lifecycle <state>] [--json]
pulse docs show <document-id> [--json]
pulse docs applicable --work <work-id> [--include-draft] [--include-stale] [--json]
pulse docs validate [--json]

pulse docs impact <ticket-id>
  --expected-revision <n>
  --posture <required|none|deferred>
  [--rationale <text>]
  [--required-doc <document-id>]...
  [--deferred-to <work-id>]...
  [--path <glob>]...
  [--domain <slug>]...
  [--label <slug>]...
  --actor <actor>
  [--json]

pulse evidence receipt verify <receipt-id> --current [--json]
```

Deferred sang Slice 5:

```text
pulse docs index|status
pulse docs search
pulse docs get
pulse docs tree
```

Deferred sang Slice 6/Phase 2+:

```text
pulse work ready
pulse work packet
pulse docs promote
pulse docs drift
pulse docs validate --changed   # khi source diff/profile runner hoàn chỉnh
```

## Library/module layout đề xuất

```text
src/
  docs/
    mod.rs
    manifest.rs          # bootstrap registry/schema, shared repository identity
    model.rs             # DocumentRecord, authority/lifecycle/scope/generated contract
    registry.rs          # load/list/show + CAS mutations
    validate.rs          # uniqueness, path, lifecycle, owner, generated, supersession rules
    applicability.rs     # deterministic work -> document buckets/reasons
    impact.rs            # Ticket docs-impact typed mutation/validation
    receipt.rs           # documentation receipt v2 registry/policy validation

  graph/
    node.rs              # optional documentation metadata
    store.rs             # typed CAS impact mutation only

  evidence/
    model.rs             # documentation payload v2 + historical v1 decoder
    receipt.rs           # registry validation hook, no docs-store ownership leak

  schema/
    docs/
      document.schema.json
    evidence/
      documentation-validation.v2.schema.json

  bin/
    pulse.rs

tests/
  docs_registry.rs
  docs_applicability.rs
  docs_impact.rs
  docs_receipt_registry.rs
  docs_crash_recovery.rs
  docs_cli_contract.rs
```

Boundary rules:

- `DocsRegistry` reuse shared storage primitives nhưng không thành `Store<T>` generic.
- Applicability pure function nhận typed graph/work context + registry snapshot + content-state resolver.
- Filesystem hashing/path checks ở adapter layer, không nằm trong CLI renderer.
- Evidence module không tự parse registry raw JSON; nó gọi typed docs validation interface.
- Slice 5 có thể reuse `DocumentRecord` và registry fingerprint nhưng không nhét search engine vào registry module.

## Registry validation layers

`pulse docs validate` chạy ít nhất:

1. registry/schema parse và canonical-byte validation;
2. repository ID consistency với evidence manifest;
3. unique document ID và canonical path;
4. ID/path/kind syntax;
5. path traversal/symlink escape/protected backup exclusion;
6. registered content existence và regular-file checks;
7. work prose (`works/**`) không bị register như durable docs;
8. owner/authority/lifecycle consistency;
9. summary/aliases/scope limits và deterministic normalization;
10. generated contract consistency;
11. supersession target existence/cycle/terminal rules;
12. duplicate current truth warning khi multiple approved docs có same exact scope/kind/path intent signal; semantic duplicate vẫn chỉ là finding candidate, không hard inference;
13. `docs/manifest.json`, nếu tồn tại, không được treated as writable registry và phải marked generated;
14. Ticket explicit required document references resolve;
15. documentation-impact posture structural validation;
16. receipt v2 registry references/path/revision consistency khi `--include-evidence` mode được thêm hoặc qua receipt verify.

Validation không:

- sửa prose;
- auto-retire duplicate docs;
- auto-select winner giữa conflicting docs;
- auto-move brownfield files;
- execute generator/link/semantic review commands.

Findings/errors tối thiểu:

```text
docs_registry_missing
docs_registry_schema_invalid
docs_registry_revision_conflict
document_id_duplicate
document_path_duplicate
document_id_invalid
document_path_unsafe
document_content_missing
document_owner_missing
document_scope_invalid
document_lifecycle_invalid
document_supersession_cycle
document_generated_contract_invalid
document_migration_backup_forbidden
document_work_content_forbidden
document_required_missing
document_required_not_current
document_required_not_authoritative
document_context_gap
documentation_impact_unknown
documentation_impact_invalid
documentation_defer_target_missing
document_receipt_registry_mismatch
document_receipt_revision_stale
document_receipt_policy_incomplete
```

## Transaction, recovery và consistency

### Registry mutation

Registry + event là single canonical target + immutable event, reuse transaction primitive Slice 1/3:

- before hash/revision;
- after hash/revision + durable event payload;
- crash trước target: cleanup;
- crash sau target trước event: complete event;
- event mismatch/manual edit: stop, preserve evidence.

### Docs-impact mutation

Ticket node + event reuse graph single-target CAS transaction. Registry references được validate dưới cùng repository guard trước commit.

Nếu registry đổi ngay sau impact validation, shared write fence ngăn concurrent mutation trong critical section. Sau commit, future registry change có thể làm Ticket reference stale; `docs applicable/validate` phải report gap, không auto-rewrite Ticket.

### Readers

`docs list/show/applicable/validate` giữ repository guard xuyên recovery + coherent registry/graph/content read theo Slice 2 read-consistency policy. V1 có thể serialize readers/writers; optimization defer benchmark.

### Content files

Slice 4 không transactionally mutate document prose. A code/doc edit commit có thể thay file content ngoài Pulse registry mutation; applicable/receipt validation luôn rehash current bytes và detect missing/stale state. Git commit remains source snapshot boundary for receipts.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| D1 | Bootstrap empty docs registry | prerequisite | Dùng existing repository ID, schema/layout đúng, không overwrite unknown files |
| D2 | Register approved domain doc | #26 | ID/path/owner/authority/scope query offline được |
| D3 | Duplicate ID hoặc canonical path | integrity | Reject trước commit |
| D4 | Unsafe path/symlink/migration backup/work content | security/#20 | Reject registration |
| D5 | Register missing content path | integrity | Reject hoặc explicit draft policy; approved current mặc định reject |
| D6 | Registry CAS conflict | concurrency | Một mutation thắng, stale writer nhận conflict rõ |
| D7 | Rename path giữ stable ID | identity | Document revision bump; ID không đổi; old receipt historical-only/stale current |
| D8 | Retire document | #20/#29 | Không còn route current; list/show vẫn giữ history |
| D9 | Supersede document | #20/#29 | Old excluded + replacement reason chain; cycle/self reject |
| D10 | Draft/stale/suspected-stale docs | #29 | Default excluded/labeled đúng; include flags không giả authority |
| D11 | Generated navigation `_index.md` | #20/#29 | Không route như authoritative content |
| D12 | Generated doc contract inconsistent | generated boundary | Validate fail, không chạy command |
| D13 | Ticket impact missing | #19 | Derived `unknown`, applicable gate incomplete |
| D14 | Ticket impact `none` thiếu rationale | #19 | Reject mutation |
| D15 | Ticket impact `deferred` thiếu linked work | #19 | Reject mutation |
| D16 | Ticket impact required với missing doc ID | #19 | Mutation reject hoặc projection incomplete theo command policy; không ready claim |
| D17 | Explicit required current approved doc | #20 | Vào required + write candidates với exact content hash |
| D18 | Path/domain/label scope match | #20/#26 | Vào optional với explainable reasons, không tự nâng required |
| D19 | Broad unrelated registry docs | bounded routing | Không xuất hiện như applicable noise mặc định |
| D20 | Required document retired sau impact record | invalidation | Applicable gate incomplete, old Ticket không auto-mutated |
| D21 | Required document superseded | routing | Old excluded, replacement evaluated rõ; Ticket ref vẫn báo cần reconcile |
| D22 | Registry/content bytes unchanged | determinism | Same fingerprint/hash/bucket semantics |
| D23 | Document content changes | #22 | Applicable content hash đổi; old receipt binding stale |
| D24 | Documentation receipt v2 canonical ID/path/revision | #22 | Registry dimension current |
| D25 | Receipt uses wrong ID for same path | #22 | `document_receipt_registry_mismatch` |
| D26 | Receipt for stale/retired/superseded doc | #22/#29 | Historical integrity valid, current registry/gate ineligible |
| D27 | Historical documentation receipt v1 | compatibility | Integrity inspectable; no rewrite; registry status legacy/conservative |
| D28 | Independent review policy without authority resolver | authority boundary | Structural checks report, authorization not evaluated, gate false |
| D29 | Crash after registry write before event | recovery | Recovery emits exactly one event |
| D30 | Crash after docs-impact node write before event | recovery | Recovery completes event, no double revision |
| D31 | Concurrent applicable read and registry mutation | consistency | Reader sees coherent before hoặc after snapshot, không half registry |
| D32 | JSON CLI outputs/errors | contract | Stable schema/code, deterministic ordering, non-zero exits đúng |
| D33 | `cargo fmt`, clippy, full tests | quality | Clean according to repository policy |

Tests phải dùng real temp Git repositories cho content/source binding cases. Registry concurrency cần process-level test, không chỉ threads. Crash tests cần failpoints registry target/event và Ticket impact target/event.

## Definition of Done của slice

- [ ] `.pulse/docs/registry.json` bootstrap idempotent, dùng shared repository identity và không overwrite unknown schema/registry.
- [ ] Registry có revision CAS, canonical deterministic bytes và immutable mutation events.
- [ ] Document ID stable, path-independent và preserve qua rename.
- [ ] Document schema cover kind, authority, lifecycle, owner, summary, aliases, scope, review policy, generated contract và supersession.
- [ ] Duplicate ID/path, unsafe path, missing approved content, backup/work-content registration và supersession cycle bị reject.
- [ ] Registry register/edit/retire/supersede mutations crash-recoverable và process-concurrency tested.
- [ ] Ticket node có typed documentation-impact/routing metadata; missing metadata derive `unknown`.
- [ ] `required`, `none`, `deferred` structural rules được enforce bằng CAS mutation + event.
- [ ] Slice không tự đánh giá semantic honesty hoặc defer authority.
- [ ] `pulse docs list|show|applicable|validate` có stable human/JSON contracts.
- [ ] Applicability compose explicit required refs + typed path/domain/label scopes và trả reason codes.
- [ ] Scope match mặc định optional, không tự nâng thành required ngoài explicit/policy rule.
- [ ] Required/current/approved docs route với exact current content hash.
- [ ] Retired, superseded, stale, suspected-stale, draft, migration backup và generated-navigation docs được exclude/label đúng default policy.
- [ ] Document supersession không silently rewrite Ticket references.
- [ ] Documentation receipt payload v2 bind canonical document ID + document revision + path/content/source.
- [ ] Historical receipt v1 vẫn inspect/verify integrity được mà không rewrite.
- [ ] Receipt validation tách integrity, bindings, registry, policy và authorization; không report authorized khi resolver chưa có.
- [ ] Independent/human review policy không trở thành gate pass chỉ vì actor tự khai.
- [ ] Registry/content changes invalidate applicable/receipt projections đúng mà không mutate immutable evidence.
- [ ] CLI vẫn thin; docs/applicability logic nằm trong typed Rust library modules.
- [ ] Không kéo comrak, Tantivy, semantic search hoặc full work packet vào slice.
- [ ] Rust format, clippy và full test suite sạch.

## Handoff sang các slice tiếp theo

### Slice 5 — Docs Section Extraction + Lexical Retrieval

Dùng registry identity/scope/lifecycle từ Slice 4 để thêm:

- comrak heading-aware section extraction;
- stable section refs, line ranges và content hashes;
- generated root/selected-area `_index.md`;
- disposable Tantivy BM25 cache;
- `pulse docs index|status|search|get|tree`;
- incremental rebuild, corruption recovery, retrieval fingerprint và evals;
- work-context ranking adjustment nhưng không override authority/exclusion.

Slice 5 không cần phát minh lại document owner/authority/applicability hoặc path lifecycle.

### Slice 6 — Shaping + Readiness Composition

Dùng:

```text
structural executability
+ implementation contract
+ shaping receipt/branch dispositions/authority
+ documentation impact + applicable docs
+ QA impact references
+ required Decisions/content references
= dispatch readiness
```

Slice 6 mới cân nhắc:

- mở `draft -> shaped` và `shaped -> ready`;
- decision/execution frontier;
- full `pulse work ready`;
- bounded work packet references;
- invalidate readiness khi registry/doc/shaping revisions đổi.

### Phase 2/3 follow-up

- docs write/read scope trong assignment packet;
- generated freshness/link/profile execution;
- documentation close gate;
- promotion candidates/handoff;
- `pulse doctor` docs findings;
- brownfield migration assistant có human approval.

## Risks và open questions cho review

1. **Single registry file:** owner docs đã chọn `registry.json`, nhưng corpus/parallel mutation threshold nào trigger sharding Decision? Slice 4 cần benchmark basic 10/100/1,000 records dù correctness không phụ thuộc cache.
2. **Document revision vs registry revision:** giữ cả hai giúp receipt bind narrow identity nhưng tăng schema. Có chấp nhận document revision riêng, hay receipt chỉ bind registry hash + ID? Proposal chọn cả hai để unrelated registry edit không stale receipt.
3. **Timestamp fields:** có cần `created_at/updated_at` trong document record hay immutable events đủ audit? Nếu thêm timestamp, deterministic tests dùng operation clock như graph.
4. **Scope glob semantics:** dùng crate/glob subset nào và normalization trên Windows ra sao? Contract phải version hóa separator/case behavior, không dựa incidental filesystem glob.
5. **Required-vs-optional inference:** proposal chỉ explicit refs là required. Repository policy/global required docs được biểu diễn trong registry scope hay PULSE policy adapter ở slice sau?
6. **Ticket routing metadata:** đặt `paths/domains/labels` trong docs-specific block tránh mở full implementation schema, nhưng Slice 6 có thể cần cùng context cho QA/knowledge. Có nên sớm tạo typed reusable `work_context`, hay chờ hai consumers để tránh abstraction sớm?
7. **`ticket.md` sync:** machine metadata và human prose có thể lệch. Slice 4 chỉ validate references cơ học; Slice 6 shaping/parser có cần canonical generated section hoặc receipt để chứng minh alignment?
8. **Investigate posture:** owner docs cho Discovery/Spike dùng `investigate`, nhưng current node chưa có work subtype. Nên defer posture này hay thêm minimal typed work role trong Slice 4?
9. **Authority resolver:** owner syntax chỉ structural. Trước readiness/close, team/role/human IDs được resolve từ `PULSE.md`, config hay Agent Registry nào?
10. **Review-policy profiles:** hard-code five policies và check matrix hay registry trỏ tới profile manifest? Slice này cần đủ typed để không accept arbitrary checks nhưng không nên kéo full harness capability manifest sớm.
11. **Suspected stale default:** exclude strict giúp safety nhưng có thể làm brownfield context biến mất. Có cần policy theo risk/kind thay vì một default global?
12. **Document rename:** stable ID giữ identity nhưng old content-bound receipt stale. Có cần explicit rename event + old-path alias để explain historical receipts tốt hơn?
13. **Supersession reference reconciliation:** old Ticket refs có nên auto-follow replacement trong required bucket hay chỉ report redirect/incomplete? Proposal route replacement để orient nhưng vẫn báo reconcile, tránh silent mutation.
14. **Generated docs:** một registry record đại diện output set hay mỗi important generated file một record? Schema cần support cả hai mà không làm path identity mơ hồ.
15. **`docs/manifest.json`:** Slice 5 có nên generate human projection ngay từ registry, hay chỉ `_index.md` đủ? Không được tạo hai writable registries.
16. **Schema evolution:** Node hiện `deny_unknown_fields` và schema v1; thêm documentation metadata cần explicit known-predecessor migration như Slice 2. Có bump node schema version hay tiếp tục pre-contract v1 với exact schema migration event?
17. **Legacy Pulse files:** repo hiện còn `.pulse/workgraph/items.jsonl` từ architecture cũ. Slice 4 không được dùng legacy JSONL làm docs/work source; public cutover vẫn cần migration proposal riêng.
18. **Content path case sensitivity:** duplicate canonical path detection trên macOS/Windows cần normalized comparison nhưng không được làm Linux repositories đổi identity bất ngờ.

## Không quyết định trong slice này

Slice này không chốt section chunking, lexical ranking, tokenizer, context budget, semantic search, shaping quality, QA applicability, actor authorization, close gate, generated command runner hoặc brownfield semantic migration. Nó chỉ chốt document identity/ownership/routing mechanics và Ticket documentation-impact facts để retrieval/readiness layers sau không phải đoán durable docs từ filesystem hoặc free-form prose.
