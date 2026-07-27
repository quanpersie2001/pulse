# Phase 1 — Slice 6: Knowledge Store Foundation

> Trạng thái: **implemented và verified historical implementation record**.
> Current knowledge-store contract được xác định bởi source, schemas và active
> tests. Full compound/search/applicable/injection vẫn thuộc Phase 4; checklist
> bên dưới ghi Slice 6 foundation scope, không phải compatibility contract.
> Tiền đề:
> [`phase1-slice5-docs-section-lexical-retrieval.md`](phase1-slice5-docs-section-lexical-retrieval.md)
> đã được implement và verify.
> Sở hữu: implementation strategy cho canonical learning store tối thiểu của
> Phase 1: manifest/schema, one-learning-per-record, typed provenance/relations,
> revision CAS, deterministic projection/fingerprint và disposable index
> boundary.
> Tham chiếu normative:
> [`PULSE_REBOOT.md`](../PULSE_REBOOT.md),
> [`02-work-graph.md`](../pulse-reboot/02-work-graph.md),
> [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md),
> [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md),
> [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md),
> [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md) và
> [`12-knowledge-compounding.md`](../pulse-reboot/12-knowledge-compounding.md).

## Trạng thái đã verify trước proposal này

Repository hiện đã implement Slice 1–5:

- sharded work graph, lifecycle, supersession, projections và crash recovery;
- immutable evidence receipts và content-addressed artifacts;
- canonical docs registry và document-level applicability;
- Markdown section extraction, generated navigation và Tantivy retrieval;
- process-level concurrency, kill-process recovery và cache publication tests;
- Slice 5 reference benchmark cho 10/100/1,000 documents;
- CI matrix được khai báo cho Linux, macOS và Windows.

Các lệnh hiện có chưa expose namespace `pulse knowledge`. Source tree cũng chưa
có `.pulse/knowledge/` adapter, learning schema, typed learning relations hoặc
knowledge fingerprint/projection.

Roadmap Phase 1 vẫn yêu cầu:

> Knowledge manifest/schema/store tối thiểu cho one-learning-per-record,
> revision CAS, provenance relations, status/confidence/applicability/promotion
> và disposable index boundary.

Vì vậy Slice 6 đóng knowledge foundation còn thiếu của Phase 1. Knowledge và
shaping/readiness là hai sibling foundations độc lập phần lớn với nhau; numbering
chỉ thể hiện delivery order, không phải dependency kỹ thuật. Full compound,
applicability-aware BM25 recall, prompt injection, feedback và promotion workflow
vẫn thuộc Phase 4.

## Vị trí của slice trong Pulse Reboot

Slice 6 tạo canonical plane mà Phase 4 có thể dựa vào:

```text
observation/evidence
  -> canonical learning entry
  -> typed provenance and knowledge relations
  -> deterministic validation/projection/fingerprint
  -> future compound/retrieval/promotion/feedback
```

Slice này trả lời các câu hỏi cơ học:

- Learning identity nằm ở đâu và có stable qua rewrite không?
- Một record có đúng một semantic learning hay đang là prose bag?
- Revision CAS và concurrent writer conflict hoạt động thế nào?
- Provenance nào được khai báo và reference có resolve được không?
- Relation ID, direction, endpoint và retry semantics là gì?
- Status/confidence/applicability/routing/promotion/freshness có typed schema
  không?
- Candidate/disputed/superseded/retired có bị phân loại khỏi future default
  eligibility không?
- Xóa projection/cache rồi rebuild có giữ canonical fingerprint và semantics
  không?

Slice này không trả lời các câu hỏi judgment:

- Candidate có thật sự reusable không?
- Root cause có đúng không?
- Hai learnings có semantic duplicate không?
- Learning có đủ authority để thành required ratchet không?
- Learning nào applicable nhất cho current Ticket?
- Learning nên promote vào document, Decision, skill hay check nào?

Các câu hỏi đó thuộc `pulse-compound`, reviewer/human authority và Phase 4
retrieval/promotion capabilities.

## Nguyên tắc

- Learning record là reusable guidance có provenance/applicability, không phải
  current docs truth, Decision hoặc work item.
- Canonical store dùng one-learning-per-record sharded JSON dưới
  `.pulse/knowledge/entries/`.
- Typed relations nằm trong plane riêng; không nhét arbitrary external links
  vào một string array không validate được.
- Optional Markdown detail chỉ là referenced narrative content; JSON entry giữ
  machine semantics.
- Work graph, docs registry, evidence và knowledge là các typed planes riêng.
  Shared storage primitive không đồng nghĩa shared schema hoặc authority.
- Mọi mutation dùng repository write fence, expected-revision CAS, canonical
  JSON, recoverable transaction và immutable event.
- Candidate được tạo với authority thấp. Kernel không cho create/edit tự khai
  `validated`, `promoted`, `enforced` hoặc `required_when_applicable` khi chưa
  có lifecycle/authority consumer của Phase 4.
- Candidate/disputed/superseded/retired không nằm trong future automatic
  eligibility. Slice 6 derive policy classification nhưng chưa inject/search.
- Explicit exclusions và promotion relation IDs được preserve typed; canonical
  target detail chỉ nằm trong relation files và kernel chưa đánh giá semantic
  correctness.
- Cache/projection là disposable. Xóa cache không làm mất entry, relation,
  revision hoặc provenance.
- Không dùng docs search index như knowledge index. Có thể reuse canonical JSON,
  cache publication và future lexical interfaces, nhưng corpora/result policy
  phải tách.
- Không thêm SQLite, vector database, embedding/model dependency hoặc daemon.
- CLI handlers giữ thin; validation, identity, CAS, relation và projection nằm
  trong typed Rust modules.

## Mục tiêu

Triển khai knowledge foundation để có thể:

- bootstrap `.pulse/knowledge/manifest.json` và schemas mà không overwrite
  unknown contract;
- tạo một candidate learning với stable ID, revision và immutable event;
- show/list/CAS-edit learning records qua typed APIs;
- lưu actionable guidance, typed applicability, routing, promotion posture,
  freshness và optional exact content binding;
- tạo typed relation với deterministic identity và idempotent retry;
- bind provenance tới work, receipt, commit, document và Decision bằng typed
  resolvers hiện hữu;
- reject unsafe content path, malformed target, missing required provenance,
  unsupported status claim và invalid relation direction;
- derive deterministic knowledge fingerprint, status summary và export
  projection;
- materialize disposable knowledge snapshot cache mà future Phase 4 search
  index có thể replace/extend;
- recover entry/relation/event mutations sau interruption;
- verify process-level CAS và relation retry behavior;
- giữ extension points cho compound, lifecycle authority, BM25 retrieval,
  applicable recall, usage feedback và promotion history.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

Slice 6 chỉ sở hữu foundation subset của các scenarios Core:

- **#53, storage/schema subset:** one-learning-per-record có actionable guidance,
  typed applicability và provenance inspectable. Semantic classification
  `non_durable` thuộc compound Phase 4.
- **#54, relation subset:** canonical prior identity, corroborates/contradicts/
  superseded relation mechanics tồn tại; semantic dedup/reconciliation defer.
- **#55, eligibility subset:** status policy phân loại candidate/disputed/
  superseded/retired khỏi default eligibility; search/get defer.
- **#60, persistence subset:** typed promotion target links/hashes được lưu và
  validate cơ học; promotion workflow/documentation gate defer.
- **#61, boundary subset:** knowledge canonical fingerprint và disposable
  projection rebuild deterministic; BM25 ranking defer.
- **#64, storage safety subset:** field/size/path/trust posture ngăn raw
  unbounded prompt/transcript/secret-like payload đi qua generic JSON bag;
  redaction scanner/reviewer gate defer.

### Decisions liên quan

- D-02, D-06 và D-07.
- D-18 đến D-22 cho local-first sharded storage và Rust kernel.
- D-52 đến D-61 cho knowledge identity, authority, applicability, relations,
  retrieval boundary và typed corpus separation.

### Slice exit

Slice hoàn thành khi canonical knowledge entry/relation store deterministic,
recoverable và query được offline; schema đủ để Phase 4 thêm lifecycle,
compound và retrieval mà không đổi identity/layout cơ bản.

Slice exit **không** đồng nghĩa:

- `pulse compound` đã tồn tại;
- candidate đã được semantic review hoặc validate;
- knowledge BM25 search/index đã tồn tại;
- `knowledge applicable --work` hoặc role-specific buckets đã tồn tại;
- learning được inject vào shaping/work packet;
- promotion target đã thực sự được mutate/verified;
- usage feedback/reinforcement/retirement workflow đã tồn tại;
- Phase 1 hoàn thành — shaping/readiness foundation vẫn còn thiếu;
- Core v1 hoàn thành.

## Non-goals

- Semantic candidate extraction từ run/review/QA/handoff.
- `pulse compound`, deduplication, contradiction resolution hoặc
  `no_reusable_learning` disposition.
- Lifecycle commands `review|validate|promote|supersede|retire` có authority.
- Automatic confidence upgrade hoặc reproduction counting.
- BM25/Tantivy knowledge index, search/get/applicable ranking hoặc evals.
- Prompt packet injection, audience budgets hoặc required overflow policy.
- Usage feedback, reinforcement, noise metrics hoặc known-failure replay.
- Doctor knowledge findings ngoài structural `knowledge validate/status`.
- Automatic docs/Decision/skill/check/hook/policy/eval mutation.
- Full actor authority, signatures, Agent Registry hoặc cryptographic approval.
- Generic user-defined learning kinds/relation types.
- Arbitrary untyped provenance payload.
- Stable symbol/path existence proof qua language server. Slice chỉ validate
  syntax/path safety và current file existence khi reference kind yêu cầu.
- Run/finding canonical stores. Slice 6 không record relation tới namespace chưa
  có canonical resolver; các target kinds đó được thêm bằng schema evolution khi
  resolver tồn tại.
- Knowledge narrative Markdown indexing hoặc rendering vào prompt.
- Secret scanning engine. Slice áp field limits/trust posture và không accept raw
  transcript/log fields; scanner/review gate thuộc later capability.
- Cross-repository learning federation/import/export.

## Repository layout

```text
.pulse/
  knowledge/
    manifest.json
    schemas/
      learning.schema.json
      relation.schema.json
    entries/
      LRN-001.json
      LRN-002.json
    relations/
      derived-from--LRN-001--work--TK-031.json
      derived-from--LRN-001--receipt--rcpt_01J.json
      corroborates--LRN-002--learning--LRN-001.json

  events/
    2026-07-24/
      evt_01J....json

  cache/
    knowledge.snapshot.json
    knowledge-search/             # reserved, not built by Slice 6

  runtime/
    locks/
      workgraph.lock
    transactions/
      txn_01J....json

knowledge/
  learnings/
    LRN-001.md                    # optional tracked narrative detail
```

Ownership:

- manifest, schemas, entries và relations là tracked canonical knowledge truth;
- events là immutable audit, không phải writable learning state;
- optional `knowledge/learnings/*.md` là tracked narrative referenced by entry;
- `.pulse/cache/knowledge.snapshot.json` là disposable projection;
- `.pulse/cache/knowledge-search/` chỉ là reserved boundary cho Phase 4;
- runtime locks/transactions không phải knowledge truth;
- evidence bytes không được copy vào learning entry/content file.

## Knowledge manifest

```jsonc
{
  "schema_version": 1,
  "repository_id": "repo_01J...",
  "learning_schema": {
    "path": "schemas/learning.schema.json",
    "sha256": "sha256:..."
  },
  "relation_schema": {
    "path": "schemas/relation.schema.json",
    "sha256": "sha256:..."
  },
  "id_pattern": "^LRN-[0-9]{3,}$",
  "content_root": "../../knowledge/learnings",
  "projection_schema_version": 1
}
```

Rules:

- `repository_id` reuse evidence repository identity; không tạo repository ID
  riêng cho knowledge.
- Manifest không chứa entry list, relation list, counters, latest pointer hoặc
  mutable graph revision.
- Exact schema paths/hashes là canonical contract.
- Existing manifest/schema không được silently overwrite.
- Unknown predecessor/version/hash trả typed error và preserve files.
- `content_root` resolve trong repository, chống traversal/symlink escape.
- Bootstrap không tạo optional Markdown content hoặc empty tracked directory.
- ID allocation scan `entries/LRN-*.json` dưới repository fence và chọn
  `max + 1`; manifest không giữ mutable counter.
- Manual deletion/branch collision làm validation fail; ID không reuse trong
  normal API.

## Learning record contract

### Base record

```jsonc
{
  "schema_version": 1,
  "id": "LRN-001",
  "revision": 1,
  "title": "Token rotation requires atomic mutation",
  "status": "candidate",
  "kind": "failure_pattern",
  "severity": "high",
  "summary": "Concurrent refresh can issue invalid tokens when rotation uses check-then-act.",
  "guidance": {
    "do": ["Use an atomic state transition."],
    "avoid": ["Do not split rotation into an unguarded read then write."],
    "required_checks": ["Exercise concurrent refresh attempts."]
  },
  "applicability": {
    "domains": ["authentication"],
    "surfaces": ["api"],
    "paths": ["src/auth/**"],
    "symbols": ["rotateRefreshToken"],
    "work_kinds": ["ticket"],
    "work_labels": ["auth"],
    "technologies": ["postgresql"],
    "operations": ["token-rotation"],
    "risks": ["concurrency"],
    "signals": ["check-then-act"],
    "platforms": [],
    "configurations": [],
    "versions": [],
    "exclusions": ["stateless-access-token-verification"]
  },
  "provenance": {
    "relation_ids": [
      "derived-from--LRN-001--work--TK-031",
      "derived-from--LRN-001--receipt--rcpt_01J"
    ],
    "source_commits": ["0123456789abcdef0123456789abcdef01234567"]
  },
  "validation": {
    "confidence": "low",
    "validated_by": [],
    "validated_at": null,
    "reproduction_count": 1,
    "contradiction_status": "none"
  },
  "routing": {
    "audiences": ["planner", "implementer", "validator", "reviewer"],
    "moments": ["shape", "plan", "execute", "verify", "review"],
    "prompt_priority": "suggested",
    "max_summary_tokens": 90
  },
  "promotion": {
    "state": "unresolved",
    "rationale": null,
    "relation_ids": []
  },
  "freshness": {
    "review_after": null,
    "invalidated_by_paths": [],
    "version_constraints": [],
    "platform_constraints": []
  },
  "trust": {
    "source": "review_required",
    "contains_untrusted_text": false,
    "redaction_status": "caller_asserted"
  },
  "content": null,
  "created_at": "2026-07-24T01:00:00Z",
  "updated_at": "2026-07-24T01:00:00Z"
}
```

### One-learning-per-record

Một record phải có một title/summary/guidance/applicability scope thống nhất.
Kernel không thể chứng minh semantic unity hoàn toàn, nhưng enforce các signal cơ
học:

- một `kind` duy nhất;
- một concise summary trong configured length;
- bounded guidance arrays;
- không có generic `notes`, `transcript`, `raw_prompt`, `log` hoặc arbitrary
  payload field;
- ít nhất một actionable guidance item;
- ít nhất một concrete positive applicability dimension;
- ít nhất một provenance relation hoặc source commit;
- unknown fields bị reject;
- optional narrative detail đi qua typed `content` binding, không inline
  unbounded prose.

Semantic reviewer Phase 4 vẫn phải reject record gom nhiều insights không liên
quan dù schema pass.

### Kind

Initial closed enum theo owner contract:

```text
success_pattern
failure_pattern
correction
ratchet
decision_heuristic
debugging_technique
verification_technique
tooling_constraint
environment_constraint
integration_constraint
performance_insight
security_insight
process_insight
context_routing_insight
```

`non_durable` không phải searchable learning kind. Nó là compound disposition;
không tạo canonical entry mặc định.

### Status

Schema nhận lifecycle vocabulary:

```text
candidate
reviewed
validated
promoted
disputed
superseded
retired
```

Slice 6 mutation boundary:

- `knowledge create` luôn tạo `candidate`;
- `knowledge edit` không được đổi status;
- imports/fixtures có thể validate mọi status để schema ổn định;
- status transition commands và authority rules defer Phase 4;
- `superseded` record phải có exactly one outgoing `superseded_by` relation;
- non-superseded record không được có outgoing `superseded_by` relation;
- `promoted` phải có at least one `promoted_to` relation được liệt kê trong
  promotion relation IDs;
- `disputed` phải có contradiction status `suspected|confirmed` hoặc outgoing
  `contradicts` relation.

### Confidence và validation posture

```text
low
medium
high
enforced
```

Slice 6 rules:

- candidate create chỉ `low`;
- candidate edit không nâng confidence;
- `validated_by`, `validated_at`, reproduction count và enforced posture được
  schema/fixture validate nhưng mutation commands defer authority-bearing update;
- `enforced` chỉ được schema-reserve; Slice 6 không có resolver cho
  check/hook/policy/eval nên không có public path tạo enforced record;
- popularity/relation count không tự nâng confidence.

### Guidance

- `do`, `avoid`, `required_checks`: unique normalized non-empty strings;
- ít nhất một trong ba arrays non-empty;
- `ratchet` phải có `required_checks` non-empty;
- max proposal limits: 16 items/array, 500 UTF-8 chars/item;
- summary max 1,000 chars; title max 200 chars;
- no embedded base64/blob/log payload;
- secret scanner defer, nhưng obvious PEM/private-key markers hoặc NUL/control
  bytes bị reject tại storage boundary.

### Applicability

Typed dimensions:

```text
domains
surfaces
paths
symbols
work_kinds
work_labels
technologies
operations
risks
signals
platforms
configurations
versions
exclusions
```

Rules:

- arrays normalize/sort/deduplicate deterministic;
- ít nhất một positive dimension ngoài `exclusions` non-empty;
- broad-only applicability như chỉ `backend`, `frontend` hoặc `testing` là hard
  create/edit error `learning_applicability_too_broad`; candidate phải thêm ít
  nhất một concrete trigger như path, symbol, operation, risk, signal,
  technology/version hoặc explicit relation;
- paths là safe repository-relative glob subset, reject absolute/`..`/protected
  runtime/evidence/cache paths;
- symbols/signals/versions là bounded strings, không regex/query language;
- exclusions được preserve typed nhưng Slice 6 chưa evaluate against work;
- future typed matching must apply exclusions before lexical ranking.

### Routing

Audiences:

```text
shaper
planner
implementer
debugger
validator
reviewer
qa
orchestrator
```

Moments:

```text
shape
plan
execute
debug
verify
review
qa
reconcile
```

Prompt priority schema:

```text
suggested
recommended
required_when_applicable
```

Slice 6 restrictions:

- candidate create/edit chỉ `suggested`;
- `required_when_applicable` requires status `validated|promoted`, confidence
  `high|enforced` và future authority proof;
- foundation validator rejects an impossible combination but does not grant
  authority.

### Promotion posture

States:

```text
unresolved
none
proposed
promoted
deferred
```

Entry promotion block chỉ giữ disposition và relation IDs:

```jsonc
{
  "state": "proposed",
  "rationale": "Promote the atomicity invariant to domain docs and a replay eval.",
  "relation_ids": [
    "promoted-to--LRN-001--document--DOC-AUTH-DOMAIN",
    "promoted-to--LRN-001--eval--EVAL-AUTH-CONCURRENCY"
  ]
}
```

Target kind, identity, revision và content hash chỉ nằm trong canonical
`promoted_to` relation files. Entry không duplicate target detail.

Rules:

- candidate create defaults `unresolved`;
- Slice 6 edit có thể set `none` với rationale hoặc `proposed`; proposed targets
  được thêm qua relation command;
- `promoted` requires matching typed relations and target binding, nhưng command
  để claim it defer Phase 4;
- `deferred` requires owner, linked work và revisit trigger in future lifecycle
  payload; schema reserves typed object, public mutation defer;
- learning-only storage không thỏa documentation promotion gate.

### Freshness

- `review_after`: optional date;
- invalidated paths/globs safe and bounded;
- version/platform constraints are typed strings/ranges but no package resolver
  in Slice 6;
- expired review date creates warning/status finding, not auto-retire;
- missing promoted target or source path creates structural finding;
- history is preserved; validator never deletes stale entries.

### Trust posture

`trust` records storage-time posture only:

- `source`: `trusted_repository|review_required|untrusted_external`;
- `contains_untrusted_text`: caller declaration;
- `redaction_status`: `not_required|caller_asserted|review_required|verified`.

Slice 6 rejects automatic-routing claims when trust is unresolved. It does not
claim secret/prompt-injection review was performed merely because caller set a
field.

### Optional content binding

```jsonc
{
  "path": "knowledge/learnings/LRN-001.md",
  "content_hash": "sha256:..."
}
```

- path must live under configured `knowledge/learnings/` root;
- repository-relative, no symlink/path escape;
- when non-null, file must exist and exact bytes must match `content_hash`, kể cả
  candidate; không có not-yet-materialized content reference;
- content mutation phải đi qua typed `knowledge edit` binding update, bump entry
  revision và emit event; direct file change làm validation report
  `learning_content_hash_stale`;
- Markdown bytes are not parsed/indexed in Slice 6;
- entry summary/guidance remains sufficient for machine query without loading
  narrative.

## Typed knowledge relations

### Relation model

```jsonc
{
  "schema_version": 1,
  "id": "derived-from--LRN-001--receipt--rcpt_01J",
  "revision": 1,
  "type": "derived_from",
  "from": {"kind": "learning", "id": "LRN-001"},
  "to": {
    "kind": "receipt",
    "id": "rcpt_01J",
    "revision": null,
    "content_hash": "sha256:..."
  },
  "created_at": "2026-07-24T01:00:00Z",
  "created_by": "human:quannv"
}
```

### Relation types

Closed initial enum:

```text
derived_from
corroborates
contradicts
superseded_by
promoted_to
implemented_by
applied_to
caused_by
```

Direction rules:

| Type | From | Allowed target kinds |
|---|---|---|
| `derived_from` | learning | work, receipt, commit, document, decision |
| `corroborates` | learning | learning |
| `contradicts` | learning | learning, document, decision |
| `superseded_by` | learning | learning |
| `promoted_to` | learning | document, decision |
| `implemented_by` | learning | work |
| `applied_to` | learning | work |
| `caused_by` | learning | learning |

Rules:

- `from` luôn là existing learning trong Slice 6;
- learning target phải tồn tại;
- work/Decision target resolve qua work graph; Decision reference kind phải khớp;
- receipt target resolve qua evidence store và can bind canonical receipt hash;
- document target resolve qua docs registry và optional revision/content hash;
- commit target phải là full Git OID resolve được;
- target kinds without a canonical resolver in the current kernel are rejected;
  run/finding/skill/script/check/hook/policy/eval relation kinds require explicit
  schema evolution when their resolver plane exists;
- `superseded_by`, `corroborates` và `caused_by` reject self-edge;
- `superseded_by` không cycle và một old learning có tối đa một outgoing target;
- `corroborates` canonicalizes learning endpoints lexically để caller order không
  tạo duplicate;
- retry same semantic tuple returns `unchanged`, no event/revision bump;
- same deterministic ID with different payload is corruption/conflict;
- relation removal/tombstone defer cùng lifecycle design Phase 4.

### Deterministic identity

```text
<type-slug>--<from-id>--<target-kind>--<target-id>
```

Requirements:

- target IDs use a portable filename-safe grammar;
- caller cannot inject `/`, `\\`, `..`, control chars or Windows-reserved names;
- symmetric `corroborates` sorts learning endpoints before identity derivation;
- filename must equal object ID + `.json`;
- future endpoint kinds require schema/version change, not arbitrary string.

### Provenance consistency

Entry `provenance.relation_ids` is a bounded self-contained index to canonical
relation files, not a second source of target detail.

- each listed relation must exist, be `derived_from`, and originate at entry;
- each outgoing `derived_from` relation must be listed in entry provenance;
- create entry + initial provenance relations is one logical multi-target
  transaction;
- adding later provenance relation mutates relation file + entry revision/
  relation list atomically;
- relation target detail lives only in relation file;
- source commits can remain direct entry fields because they are immutable
  scalar source bindings and do not need reverse graph traversal.

This preserves self-contained entry inspection without allowing provenance
arrays and relation targets to drift independently.

## Create and edit mutation model

### Candidate create input

`knowledge create --file` consumes a typed candidate draft without kernel-owned
fields:

```jsonc
{
  "title": "...",
  "kind": "failure_pattern",
  "severity": "high",
  "summary": "...",
  "guidance": {...},
  "applicability": {...},
  "provenance_targets": [
    {"relation": "derived_from", "kind": "work", "id": "TK-031", "revision": 4},
    {"relation": "derived_from", "kind": "receipt", "id": "rcpt_01J"}
  ],
  "routing": {...},
  "promotion": {"state": "unresolved", "rationale": null, "relation_ids": []},
  "freshness": {...},
  "trust": {...},
  "content": null
}
```

Kernel assigns:

- `LRN-NNN` ID;
- revision `1`;
- status `candidate`;
- confidence `low`;
- timestamps;
- deterministic provenance relation IDs/files.

Create requires at least one resolvable current provenance source: work,
receipt, document, Decision or full Git commit. Unsupported future target kinds
are rejected before ID allocation.

### Typed edit

```text
pulse knowledge edit LRN-001
  --expected-revision 1
  --patch candidate-patch.json
  --actor human:quannv
```

Slice 6 patch may update:

- title, severity, summary;
- guidance;
- applicability;
- audiences/moments/max summary budget;
- promotion `none|proposed` posture and rationale;
- freshness;
- trust declaration;
- optional exact content binding.

Patch may not update:

- ID/schema version;
- status;
- confidence/validation authority fields;
- timestamps/revision;
- promoted/deferred lifecycle claims;
- provenance relation IDs directly.

Provenance changes use relation command so entry + relation stay atomic.

### Relation add

```text
pulse knowledge relation add LRN-001
  --type derived-from
  --to-kind receipt
  --to rcpt_01J
  [--target-revision 3]
  [--target-hash sha256:...]
  --expected-revision 1
  --actor human:quannv
```

When relation participates in entry snapshot (`derived_from`, `promoted_to`),
command uses entry expected revision and commits relation + updated entry in one
multi-target transaction. Pure learning relation that does not modify embedded
snapshot still validates under same repository fence and emits one event.

## CAS, transaction và recovery

### Entry create/edit

Entry + event uses existing prepared transaction protocol:

- create before state `absent` with commit-time create-new check;
- edit before `{hash, revision}` and after revision `+1`;
- prepared event payload durable before canonical write;
- crash after entry before event completes event on recovery;
- same ID/same bytes retry unchanged where operation identity supports it;
- same ID/different bytes conflicts; no overwrite.

### Entry + relation logical mutation

Create with provenance and later relation additions use ordered multi-target
roll-forward:

1. durable intent contains after bytes for entry and every new relation;
2. targets sort deterministic: entry, then relation IDs lexical;
3. all-before cleanup;
4. planned prefix-after rolls forward remaining targets;
5. all-after writes exactly one logical event;
6. manual/unplanned state stops and preserves intent/evidence;
7. reader holds repository guard through recovery + coherent load/validation.

No sequence of independent commits may create a visible entry whose required
provenance relation is absent.

### Repository fence

Slice 6 reuses current repository-scoped `workgraph.lock` because transactions,
events, work/evidence/docs references and cross-plane validation need one
coherent mutation snapshot. Lock filename is legacy naming debt; renaming it is
out of scope unless a separate migration changes all planes atomically.

Writers on distinct learning entries may serialize in v1 but never rewrite a
shared canonical entry list/counter file.

## Canonical JSON and knowledge fingerprint

Entries/relations use existing canonical serializer:

- recursively sorted object keys;
- semantic arrays normalized/sorted where order is not meaningful;
- exact UTF-8 bytes, LF and one trailing newline;
- floats rejected;
- exact bytes hashed with SHA-256.

Knowledge fingerprint derives from:

```text
knowledge fingerprint schema/version
+ manifest hash
+ learning schema hash
+ relation schema hash
+ sorted (entry path, entry content hash)
+ sorted (relation path, relation content hash)
```

Fingerprint excludes:

- events;
- runtime locks/transactions;
- cache/projection;
- optional narrative bytes themselves.

When materialized, entry `content.path` and exact `content.content_hash`
participate in canonical entry bytes and therefore knowledge fingerprint.
Changing narrative bytes without updating the binding makes validation fail; a
typed binding update bumps entry revision and changes fingerprint.

## Projection và disposable index boundary

### Snapshot

`pulse knowledge export --json` derives:

```jsonc
{
  "schema_version": 1,
  "knowledge_fingerprint": "sha256:...",
  "entries": [],
  "relations": [],
  "inverse": {
    "derived_from": {},
    "corroborated_by": {},
    "contradicted_by": {},
    "supersedes": {},
    "promotions": {},
    "implemented_by": {},
    "applied_to": {},
    "causes": {}
  },
  "eligibility": {
    "future_default_search": {
      "eligible": [],
      "excluded": [
        {"id": "LRN-001", "reason_codes": ["learning_candidate"]}
      ]
    }
  },
  "counts": {
    "entries": 1,
    "relations": 2,
    "by_status": {"candidate": 1},
    "by_kind": {"failure_pattern": 1}
  }
}
```

Rules:

- deterministic sort/order;
- validate canonical store before export success;
- eligibility projection is lifecycle/trust classification only, not work
  applicability or lexical ranking;
- candidate/disputed/superseded/retired excluded by default;
- reviewed/validated/promoted may be structurally eligible, but required routing
  still false without Phase 4 policy.

### Cache

```text
.pulse/cache/knowledge.snapshot.json
```

- keyed by knowledge fingerprint + projection schema version;
- missing/stale/corrupt cache discarded and rebuilt;
- cache cannot repair canonical entries/relations;
- delete/rebuild preserves fingerprint and equivalent projection;
- no `records.jsonl` or Tantivy index required in Slice 6;
- `.pulse/cache/knowledge-search/` remains a reserved empty boundary, not
  materialized as correctness dependency.

### Status

`pulse knowledge status --json` reports:

- manifest/schema state;
- canonical fingerprint;
- entry/relation counts;
- status/kind counts;
- structurally future-search eligible/excluded counts;
- stale content/provenance/promotion references;
- cache state `missing|current|stale|corrupt|incompatible`;
- unsupported future target kinds that require schema/resolver evolution.

Status is read-only and does not auto-edit entries or build Phase 4 index.

## Validation layers

`pulse knowledge validate` runs at least:

1. manifest/schema path/hash/version validation;
2. canonical JSON and filename/object ID consistency;
3. learning ID/kind/status/confidence enums;
4. one-learning structural guidance/applicability/provenance requirements;
5. field length/count/control-character limits;
6. path/glob/content-root safety;
7. trust/routing/status/confidence combination rules;
8. promotion state/relation-ID consistency;
9. freshness date/path/constraint syntax;
10. relation deterministic ID, endpoint kind and direction;
11. relation endpoint existence/resolver status;
12. provenance relation index ↔ canonical relation consistency;
13. supersession one-target/cycle/status invariants;
14. contradiction/disputed consistency;
15. promoted/enforced structural target requirements;
16. duplicate semantic tuple and relation retry conflicts;
17. pending transaction/orphan temp/event mismatch checks;
18. snapshot cache ignored for canonical correctness.

Validation findings classify:

- `error`: canonical integrity/contract violation;
- `warning`: structurally valid but stale or trust-unresolved state;
- `unavailable`: reserved for read-only status of future capabilities, never a
  relation accepted into the Slice 6 canonical store.

Validation never:

- decides a learning is semantically true;
- upgrades status/confidence;
- merges duplicates;
- resolves contradiction;
- mutates docs/Decision/policy;
- deletes stale or disputed history.

### Error/finding codes

Minimum stable codes:

```text
knowledge_manifest_missing
knowledge_manifest_schema_invalid
knowledge_manifest_repository_mismatch
knowledge_schema_hash_mismatch
learning_not_found
learning_id_invalid
learning_id_conflict
learning_schema_invalid
learning_revision_conflict
learning_guidance_missing
learning_applicability_missing
learning_applicability_too_broad
learning_provenance_missing
learning_provenance_mismatch
learning_content_path_unsafe
learning_content_missing
learning_content_hash_stale
learning_status_claim_unsupported
learning_confidence_claim_unsupported
learning_routing_invalid
learning_promotion_invalid
learning_freshness_stale
learning_trust_unresolved
knowledge_relation_not_found
knowledge_relation_id_invalid
knowledge_relation_direction_invalid
knowledge_relation_endpoint_missing
knowledge_relation_target_unsupported
knowledge_relation_cycle
knowledge_relation_conflict
knowledge_snapshot_missing
knowledge_snapshot_stale
knowledge_snapshot_corrupt
```

CAS error JSON follows existing contract and includes expected/current revision.

## CLI surface

```text
pulse knowledge create
  --file <candidate.json>
  --actor <actor>
  [--json]

pulse knowledge show <learning-id> [--json]

pulse knowledge list
  [--status <status>]
  [--kind <kind>]
  [--json]

pulse knowledge edit <learning-id>
  --expected-revision <n>
  --patch <candidate-patch.json>
  --actor <actor>
  [--json]

pulse knowledge relation add <learning-id>
  --type <relation-type>
  --to-kind <target-kind>
  --to <target-id>
  [--target-revision <n>]
  [--target-hash <sha256>]
  --expected-revision <n>
  --actor <actor>
  [--json]

pulse knowledge validate [--json]
pulse knowledge export [--json]
pulse knowledge status [--json]
```

`show` returns canonical JSON record metadata and relation summaries. It is not
Phase 4 progressive `knowledge get` with narrative/provenance expansion.

Deferred:

```text
pulse compound ...
pulse knowledge capture
pulse knowledge review
pulse knowledge validate <id> --evidence ...
pulse knowledge promote
pulse knowledge supersede
pulse knowledge retire
pulse knowledge search|get|applicable|index
pulse knowledge feedback
```

No aliases should imply these lifecycle/retrieval capabilities exist.

## CLI output contract

Create/edit outcome:

```jsonc
{
  "schema_version": 1,
  "code": "created",
  "status": "created",
  "knowledge_fingerprint": "sha256:...",
  "value": {"id": "LRN-001", "revision": 1, "status": "candidate"},
  "relations": ["derived-from--LRN-001--work--TK-031"]
}
```

Relation retry:

```jsonc
{
  "schema_version": 1,
  "code": "unchanged",
  "status": "unchanged",
  "relation_id": "derived-from--LRN-001--receipt--rcpt_01J"
}
```

Machine output always has `schema_version` and stable `code`. Invalid manifest,
schema, CAS, relation, unresolved transaction or unsupported status claim exits
non-zero.

## Immutable events

Event types:

```text
knowledge.learning.created
knowledge.learning.updated
knowledge.relation.added
```

Payloads contain:

- entry/relation IDs;
- before/after revisions/hashes;
- changed field names;
- target type/identity/hash summary;
- knowledge fingerprints before/after when deterministic;
- actor and operation context.

Events do not duplicate full guidance/applicability/narrative. Entry/relation is
canonical detail.

No lifecycle events are emitted in Slice 6 because authority-bearing lifecycle
mutations are deferred.

## Library/module layout đề xuất

```text
src/
  knowledge/
    mod.rs
    manifest.rs          # bootstrap, schema hashes, shared repository identity
    model.rs             # Learning, draft, patch, typed enums/refs
    relation.rs          # relation model, endpoint kinds, deterministic IDs
    validate.rs          # entry/relation/cross-plane structural validation
    store.rs             # KnowledgeStore lock/CAS/transaction/event adapter
    projection.rs        # fingerprint, inverse indexes, eligibility, cache

  schema/
    knowledge/
      learning.schema.json
      relation.schema.json

  bin/
    pulse.rs             # thin clap parsing/rendering only

tests/
  knowledge_store.rs
  knowledge_relations.rs
  knowledge_projection.rs
  knowledge_cli_contract.rs
  knowledge_process_concurrency.rs
  knowledge_crash_recovery.rs
```

Boundary rules:

- `KnowledgeStore` reuse `WriteGuard`, transaction and event primitives;
- no public generic `Store<T>` abstraction;
- relation resolver calls typed graph/evidence/docs/source interfaces, not raw
  JSON parsing duplicated in knowledge module;
- projection consumes validated typed snapshot;
- future lexical adapter consumes normalized knowledge records, not docs section
  schema;
- binary does not allocate IDs, read schemas, resolve references or calculate
  fingerprints itself.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| K1 | Bootstrap empty knowledge plane | Phase 1 | Manifest/schemas/dirs đúng; shared repository ID; no overwrite |
| K2 | Existing unknown manifest/schema | integrity | Preserve and reject clearly |
| K3 | Create candidate with work + receipt provenance | #53 subset | One entry, deterministic relations, one event, valid fingerprint |
| K4 | Create candidate without guidance | #53 | Reject before commit |
| K5 | Create candidate without concrete applicability | #53 | Reject; broad-only applicability is a hard error |
| K6 | Create candidate without provenance | #53 | Reject before ID/event commit |
| K7 | Candidate attempts validated/promoted/enforced claim | authority | Reject `learning_status_claim_unsupported` |
| K8 | One-learning field/size/unknown payload limits | #64 subset | Reject raw transcript/log/arbitrary fields/control bytes |
| K9 | Exact optional content binding | storage | Existing safe path/hash accepted; byte drift becomes stale error |
| K10 | Content path traversal/symlink escape | security | Reject |
| K11 | Show/list filters and deterministic order | query | Offline stable output |
| K12 | CAS edit current revision | storage | Revision +1, event and fingerprint update |
| K13 | CAS edit stale revision | concurrency | `learning_revision_conflict`, no mutation/event |
| K14 | Two processes edit same revision | concurrency | One success, one CAS conflict |
| K15 | Two processes create candidates | concurrency | Unique IDs; may serialize; no shared entry file conflict |
| K16 | Add same relation twice | #54 subset | First created, retry unchanged, no duplicate event |
| K17 | Relation deterministic ID payload mismatch | integrity | Hard conflict; preserve original |
| K18 | Invalid relation direction/kind | relations | Reject before commit |
| K19 | Missing work/receipt/document/learning endpoint | provenance | Reject with typed endpoint code |
| K20 | Commit reference full/resolving | provenance | Accept; abbreviated/unresolved reject |
| K21 | Unsupported future endpoint kind | boundary | Reject relation until schema/resolver evolution |
| K22 | Add derived provenance relation | atomicity | Entry revision/list + relation + one event logical commit |
| K23 | Crash after entry before relation | recovery | Roll forward relation/event; reader never sees invalid complete state |
| K24 | Crash after all targets before event | recovery | Exactly one event after recovery |
| K25 | Manual conflicting edit during recovery | recovery | Stop and preserve intent/evidence |
| K26 | Supersession self/cycle/multiple outgoing fixture | relation integrity | Validate fail |
| K27 | Promoted/enforced fixture missing targets | #60 subset | Validate fail; no authority inference |
| K28 | Candidate/disputed/superseded/retired eligibility | #55 subset | Excluded with reason codes |
| K29 | Reviewed/validated/promoted structural eligibility | #55 subset | Eligible label only; required routing false |
| K30 | Delete snapshot cache and rebuild | #61 subset | Same fingerprint and equivalent projection |
| K31 | Corrupt/stale snapshot cache | #61 subset | Discard/rebuild; canonical entries untouched |
| K32 | Canonical bytes and normalized arrays | determinism | Same semantic input gives same bytes/hash |
| K33 | Repository ID mismatch | security | Validate fail |
| K34 | JSON CLI contracts/errors | contract | Stable schema/code/order/non-zero exits |
| K35 | Full Rust validation | quality | fmt, Clippy, tests and CI matrix clean |

Crash tests cần failpoints ở entry, first relation, remaining relations và event
boundaries. Concurrency tests dùng processes thật. Cross-plane provenance tests
phải dùng real temporary Git repositories, work nodes, receipts và docs registry
records thay vì mock string existence.

## Definition of Done của slice

- [ ] Knowledge manifest/schema bootstrap idempotent, reuse shared repository ID
  và không overwrite unknown contract.
- [ ] Canonical layout dùng one-learning-per-record sharded JSON + typed relation
  files; không có tracked monolith/counter.
- [ ] Learning ID stable, deterministic allocator không reuse normal history.
- [ ] Record schema cover kind, status, severity, actionable guidance,
  applicability, provenance, confidence, routing, promotion, freshness, trust,
  exact content binding và timestamps.
- [ ] Candidate create always status candidate/confidence low/suggested routing;
  caller không tự claim validated/promoted/enforced.
- [ ] Unknown fields, raw payload bags, unsafe paths và unbounded text bị reject.
- [ ] At least one actionable guidance item, concrete applicability dimension và
  provenance source required.
- [ ] Work/receipt/document/Decision/commit references resolve và bind typed
  revisions/hashes khi available.
- [ ] Endpoint kinds without a current canonical resolver are rejected until
  explicit schema/resolver evolution; không có provisional canonical relation.
- [ ] Relation types/directions/endpoints closed and typed; IDs deterministic.
- [ ] Relation retry idempotent; payload conflict hard-fails.
- [ ] Provenance relation index và canonical relation files không drift.
- [ ] Entry create/edit/relation add use expected revision, canonical bytes,
  immutable event and recoverable transaction.
- [ ] Multi-target entry + relation mutation roll-forward/recovery and coherent
  reader contract pass process failpoints.
- [ ] Candidate/disputed/superseded/retired structural eligibility exclusion is
  deterministic and explainable.
- [ ] Promotion/confidence/status fixtures validate structural invariants without
  granting semantic authority.
- [ ] Knowledge fingerprint includes manifest/schema/entry/relation truth and
  excludes runtime/cache/events.
- [ ] `knowledge export/status` deterministic; delete/corrupt cache rebuild does
  not affect canonical truth.
- [ ] `pulse knowledge create|show|list|edit|relation add|validate|export|status`
  have stable human/JSON contracts.
- [ ] CLI remains thin; knowledge logic lives in typed Rust modules.
- [ ] No compound, BM25 search, applicable recall, prompt injection, usage
  feedback, semantic dedup or promotion workflow is smuggled into the slice.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --quiet -- -D warnings`
  and `cargo test --all-targets` pass.
- [ ] Platform CI matrix passes storage/concurrency/recovery tests on supported
  platforms before portable completion is claimed.

## Handoff sang Slice 7 — Shaping Contract + Readiness Projection

Sau Slice 6, Phase 1 còn shaping/readiness foundation:

```text
structural executability
+ implementation contract
+ shaping result and branch dispositions
+ authority/approval/source revision bindings
+ documentation impact/applicable docs
+ required Decision/content references
= dispatch readiness projection
```

Slice 7 nên sở hữu:

- implementation contract metadata/code anchors/invariants/mode;
- shaping-map reference/revision;
- destination, exit condition và out-of-scope boundary;
- critical branch dispositions;
- bounded `not_yet_specified` fog;
- canonical resolution pointers;
- shaping receipt/source/revision binding;
- ready gate explanation;
- decision frontier và execution frontier projections;
- readiness invalidation khi graph/docs/shaping inputs đổi.

Slice 7 không cần full conversational `pulse-shape` Agent capability hoặc
single-agent dispatch. Minimal semantic shaping path, reconciliation mutation,
work packet, runner, lease và resume tiếp tục sang Phase 2.

## Phase 4 follow-up

Phase 4 mở rộng foundation này với:

- continuous candidate capture;
- `pulse-compound` synthesis/dedup/disposition;
- lifecycle authority and promotion history;
- Tantivy knowledge index;
- `knowledge search|get|applicable|index|status` full contract;
- typed applicability filter before ranking;
- required/recommended/suggested/excluded buckets;
- role/moment-specific bounded prompt injection;
- usage feedback and reinforcement/noise classification;
- historical known-failure retrieval evals;
- contradiction reconciliation and doctor findings;
- promotion to docs/Decision/skill/check/hook/policy/eval.

Identity, record layout, relation IDs and canonical fingerprint from Slice 6 must
remain usable; Phase 4 may evolve schemas explicitly but must not replace the
plane with a transcript database or untyped vector memory.

## Quyết định đã khóa cho implementation

1. **Provenance index:** entry giữ bounded `provenance.relation_ids`; canonical
   target detail chỉ nằm trong relation files. Entry + relation updates là một
   logical multi-target transaction.
2. **Resolver policy:** Slice 6 chỉ record target kinds có canonical resolver
   hiện hữu: learning, work/Decision, receipt, document và full Git commit.
   Future run/finding/skill/script/check/hook/policy/eval targets cần explicit
   schema evolution; không có `provisional_external` canonical relation.
3. **Content binding:** materialized narrative luôn có exact path + SHA-256.
   Byte change không cập nhật binding làm validation fail; typed binding edit
   bump entry revision/event/fingerprint.
4. **Snapshot cache:** `.pulse/cache/knowledge.snapshot.json` là required
   disposable Phase 1 projection, không chỉ reserved optimization.
5. **Applicability minimum:** broad-only candidate là hard error. Create/edit cần
   ít nhất một concrete trigger dimension hoặc explicit provenance relation.
6. **Promotion representation:** entry chỉ giữ state/rationale/relation IDs;
   target identity/revision/hash chỉ nằm trong `promoted_to` relation files.
7. **Phase ordering:** Knowledge và shaping/readiness là sibling foundations.
   Slice numbering là delivery order, không phải technical dependency.

## Risks và open questions còn lại

1. **Numeric IDs:** `max + 1` matches work graph but branch-local creation can
   collide on merge. Need usage evidence before changing to ULID.
2. **Shared repository lock:** one lock simplifies cross-plane consistency but
   serializes independent knowledge/docs/graph writes. Benchmark before finer
   locks.
3. **Trust posture:** obvious secret marker rejection is not a scanner. Which
   redaction/review profile becomes required before reviewed/validated status?
4. **Schema lifecycle:** should reviewed/validated/promoted records be accepted
   only through future commands, or support controlled import fixtures now?
5. **Relation endpoint revision:** unrelated work/document revision changes may
   stale provenance even when observation remains historically valid. Need
   distinguish historical binding from current applicability later.
6. **Event envelope typing:** actor/subject remain strings. Avoid broad event
   migration unless knowledge use exposes a concrete need.
7. **Status fixtures:** schema validates lifecycle combinations before commands
   exist. Ensure no test helper becomes an accidental public bypass.
8. **Canonical duplicate detection:** filename/tuple duplicates are mechanical;
   semantic duplicate detection must not leak into kernel by title similarity.

## Không quyết định trong slice này

Slice này không chốt semantic compounding quality, learning authority thresholds,
applicability ranking weights, role prompt budgets, feedback reliability,
knowledge BM25 schema, semantic retrieval, promotion execution, doctor policy,
work-packet injection hoặc failure replay.

Nó chỉ chốt canonical, deterministic và recoverable foundation để những
capability đó có thể được implement sau mà không biến transcript, docs, evidence
hoặc work graph thành một memory store thứ hai.
