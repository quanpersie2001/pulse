# Phase 1 — Slice 3: Evidence Identity + Gate-Grade Receipt Foundation

> Trạng thái: **implemented và verified historical implementation record**.
> Current receipt/artifact contract được xác định bởi embedded schemas, source
> và active tests; checklist/proposed wording bên dưới không phải compatibility
> contract.
> Tiền đề: [`phase1-slice2-lifecycle-executability.md`](phase1-slice2-lifecycle-executability.md) đã hoàn thành và cung cấp lifecycle, supersession, structural executability, multi-target recovery và graph read consistency.
> Sở hữu: implementation strategy cho lát cắt Phase 1 tiếp theo: immutable receipt identity, content-addressed artifacts, source/content/work-revision binding, typed validation và integration đầu tiên với supersession.
> Tham chiếu normative: [`PULSE_REBOOT.md`](../PULSE_REBOOT.md), [`02-work-graph.md`](../pulse-reboot/02-work-graph.md), [`03-story-qa.md`](../pulse-reboot/03-story-qa.md), [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md), [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md), [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md), [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md), [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md).

## Vị trí của slice trong Pulse Reboot

Slice 1 đã chứng minh canonical sharded storage, CAS, repository write fence, atomic replace, crash recovery, immutable semantic events và deterministic graph projection. Slice 2 dùng nền đó để chứng minh lifecycle, supersession và explainable structural executability, nhưng cố ý giữ các gate sau ở trạng thái chưa có capability:

- shaping/approval;
- documentation impact và documentation review;
- verification/QA receipts;
- close gate;
- lease/run authority.

Slice 2 cũng phải dùng một `SupersessionAssertion` provisional nằm trực tiếp trong event. Assertion này giữ identity/revision/reference nhưng chưa có receipt identity riêng, chưa verify độc lập, chưa bind artifact/content theo một contract dùng lại được và chưa thể được các gate khác reference thống nhất.

Slice 3 đưa Pulse từ “event có chứa claim” sang “claim quan trọng có một immutable evidence identity và validator deterministic”. Nó tập trung vào bốn capability:

1. immutable one-receipt-per-file store;
2. content-addressed artifact store với hash verification;
3. typed receipt envelopes và validators cho work revision, source, content và artifact bindings;
4. integration đầu tiên: supersession dùng reconciliation receipt thay cho provisional inline assertion.

Slice 3 **không mở full readiness hoặc close gate**. Một receipt structurally valid không tự động có nghĩa actor có authority, semantic claim là đúng, hay mọi gate family cần thiết đã pass.

## Vì sao receipt foundation đi trước Document Registry

Thứ tự đề xuất sau Slice 2 là:

1. receipt/evidence identity;
2. Document Registry + applicable-doc projection;
3. section extraction + lexical retrieval;
4. shaping/readiness composition.

Lý do receipt foundation nên đi trước registry:

- Receipt identity, source binding, content hash và artifact verification là dependency chung của shaping, documentation, verification, QA và run lifecycle; Document Registry chỉ giải quyết một plane.
- Nếu làm registry trước, documentation validation vẫn phải phát minh proof/hash format tạm rồi migrate sang evidence store sau.
- Slice 1/2 đã có canonical JSON, immutable events, lock, single/multi-target recovery và graph revisions; đây là thời điểm rẻ nhất để reuse những primitive đó.
- Supersession đang có provisional assertion format. Chuyển nó sang receipt reference sớm tránh nhiều consumer phụ thuộc vào payload tạm.
- Document Registry ở slice kế tiếp có thể dùng receipt foundation để phân biệt path/content-valid với registry/owner/authority-valid thay vì trộn các khái niệm đó ngay từ đầu.

Receipt foundation vẫn phải nhỏ và typed. Slice này không xây một generic workflow database hoặc một “evidence JSON bag” nhận payload tùy ý.

## Nguyên tắc

- Receipt là immutable observation/assertion có provenance; nó không phải canonical work status, durable docs truth hay policy authority.
- Artifact bytes dùng content hash làm identity; receipt dùng stable receipt ID và có canonical receipt hash riêng.
- Receipt validity có nhiều chiều: schema/integrity, binding freshness và authorization không được gộp thành một boolean mơ hồ.
- Kernel validate mechanics và typed references. Agent/human/reviewer chịu trách nhiệm semantic judgment.
- Source/content/work bindings phải explicit. Không chấp nhận prose kiểu “reviewed current code” không có snapshot identity.
- Global graph fingerprint là audit context, không mặc định làm receipt stale vì một mutation không liên quan. Gate validity dựa trên các work revisions/content/source bindings cụ thể mà receipt khai báo.
- Một receipt không được sửa để khớp source mới. Khi source/content thay đổi, tạo receipt mới.
- One receipt per file và one artifact per digest tránh append hotspot và giữ Git/local inspection đơn giản.
- Receipt record + semantic event phải crash-recoverable qua transaction primitive hiện có.
- Evidence store là adapter riêng. Không tiếp tục biến `graph/store.rs` thành generic store cho mọi plane.
- Không mở transition `ready`, `active`, `verifying` hoặc `done` chỉ vì một receipt tồn tại.

## Mục tiêu

Triển khai evidence foundation để có thể:

- bootstrap `.pulse/evidence/` mà không overwrite contract hiện có;
- put và verify content-addressed artifacts;
- record/show/list/verify immutable typed receipts;
- detect tampering, missing artifacts, stale work revisions, stale content hashes và source mismatch;
- phân biệt structural validity, current-binding validity và authorization status;
- record receipt + immutable event với idempotency/recovery rõ;
- reference receipt từ event/gate mà không copy semantic payload;
- thay supersession inline assertion bằng `supersession_reconciliation` receipt cho mutation mới;
- giữ khả năng đọc/audit historical Slice 2 events mà không rewrite history;
- tạo extension points rõ cho Document Registry, shaping readiness, developer verification, documentation review và QA ở các slice sau.

## Acceptance scope

### Roadmap scenarios được slice này sở hữu

- **#13, primitive subset:** artifact/receipt hash mismatch bị validator từ chối; handoff semantics đầy đủ defer Phase 2.
- **#16, receipt subset:** supersession reconciliation có identity/provenance riêng thay vì inline claim; semantic coverage vẫn do reviewer/human chịu trách nhiệm.
- **#22, foundation:** documentation receipt invalid khi bound content hash đổi; registry/owner policy defer Slice 4.
- **#50, primitive subset:** source/content/work binding đổi làm receipt cũ không còn current; QA baseline/environment semantics defer Phase 3.

### Decisions liên quan

- D-02, D-06, D-07.
- D-17 đến D-22.
- D-29: documentation receipt source-bound và content-bound.
- D-43/D-44 ở schema/reference boundary cho shaping receipt; Slice 3 không tự đánh giá semantic shaping quality.
- D-47 đến D-50 ở reusable evidence boundary; full QA receipt contract vẫn thuộc Phase 3.

### Slice exit

Slice hoàn thành khi evidence identity, immutable storage, typed binding validation và supersession receipt integration deterministic/recoverable.

Slice exit **không** đồng nghĩa:

- actor đã được policy-authorize;
- shaping đã đủ để transition `draft -> shaped`;
- Ticket đã đủ để transition `shaped -> ready`;
- verification/QA/docs review đủ để close Ticket hoặc Story;
- dirty worktree snapshot đã có canonical algorithm;
- Phase 1 hoặc Core v1 hoàn thành.

## Non-goals

- Document Registry, document owner/scope/authority resolution hoặc `pulse docs applicable`.
- Docs section extraction, generated `_index.md`, Tantivy index hoặc `pulse docs search|get`.
- Ticket documentation-impact mutation/ready gate.
- Full shaping map, decision frontier, branch reconciliation hoặc semantic shaping reviewer.
- Full developer verification profile, process runner, handoff, close gate hoặc run lifecycle.
- Full QA receipt schema, baseline/case revision, environment, fixture, executor, retry/flaky/waiver policy.
- Assignment lease, Agent Registry, signature, cryptographic actor identity hoặc remote attestation.
- Dirty-worktree canonicalization. Slice 3 chỉ claim clean Git commit source binding; unsupported dirty binding fail rõ.
- Remote artifact store, retention/GC, compression, encryption-at-rest hoặc large-object transport.
- Receipt update/delete/revoke/supersede. Correction tạo receipt mới và relation/history semantics được thêm khi có consumer thật.
- Generic user-defined receipt kinds hoặc arbitrary unvalidated payload.
- Event migration/rewrite cho historical Slice 1/2 events.
- Một shared mutable evidence index. `list` scan sharded receipts; optimization/cache chỉ thêm sau benchmark.

## Repository layout

```text
.pulse/
  evidence/                           # tracked metadata/receipts; artifact policy có thể vary sau
    manifest.json
    schemas/
      receipt-envelope.v1.schema.json
      supersession-reconciliation.v1.schema.json
      shaping-validation.v1.schema.json
      documentation-validation.v1.schema.json
    receipts/
      rcpt_01J....json
    artifacts/
      sha256/
        ab/
          abcdef.../
            content
            metadata.json

  events/
    2026-07-22/
      evt_01J....json

  runtime/
    locks/
      workgraph.lock                  # Slice 3 reuse repository mutation fence hiện có
    transactions/
      txn_01J....json
```

Ownership:

- `manifest.json`, schemas và receipt files là canonical evidence metadata.
- Artifact `content` là canonical bytes theo digest; `metadata.json` mô tả size/media type/redaction posture nhưng không đổi identity của bytes.
- Event là immutable audit của receipt/artifact operation, không phải receipt truth thứ hai.
- Runtime intent chỉ phục vụ recovery, không phải durable evidence.
- Slice 3 mặc định giữ artifact trong repository-local evidence plane để chứng minh mechanics. Track/Git-ignore/retention policy cho artifact lớn phải được khóa trước public distribution; tests dùng artifact nhỏ.

## Evidence manifest

Ví dụ:

```jsonc
{
  "schema_version": 1,
  "receipt_schemas": {
    "1": {
      "schema": "schemas/receipt-envelope.v1.schema.json",
      "schema_hash": "sha256:..."
    }
  },
  "receipt_kinds": {
    "supersession_reconciliation": {
      "1": {
        "schema": "schemas/supersession-reconciliation.v1.schema.json",
        "schema_hash": "sha256:..."
      }
    },
    "shaping_validation": {
      "1": {
        "schema": "schemas/shaping-validation.v1.schema.json",
        "schema_hash": "sha256:..."
      }
    },
    "documentation_validation": {
      "1": {
        "schema": "schemas/documentation-validation.v1.schema.json",
        "schema_hash": "sha256:..."
      }
    }
  },
  "repository_id": "repo_01J...",
  "artifact_algorithm": "sha256",
  "max_inline_receipt_bytes": 262144,
  "max_artifact_bytes": 16777216
}
```

Rules:

- Manifest không chứa receipt list, artifact counters hoặc mutable latest pointers.
- Existing manifest/schema không được silently overwrite.
- `repository_id` là generated stable ULID/UUID được tạo một lần khi bootstrap evidence plane; repository copy giữ identity, explicit re-home/fork migration mới đổi identity. Không derive từ root path hoặc mutable remote URL.
- Mỗi `(kind, payload_version)` trỏ tới một immutable schema path + exact schema hash. Schema version cũ được giữ để verify historical receipts; upgrade chỉ append version mapping mới, không replace bytes cũ.
- Binary embed exact bootstrap schemas nhưng repository-owned schema là canonical sau bootstrap. Unknown predecessor/hash làm migration refuse thay vì overwrite.
- Unknown schema/kind version fail rõ; không deserialize thành generic `serde_json::Value` rồi coi là valid.
- Size defaults là proposal/prototype values, không phải public compatibility contract.

## Receipt identity và envelope

### Stable ID và hash

- Receipt ID dùng prefix `rcpt_` + ULID.
- Filename là `<receipt-id>.json`.
- `receipt_hash` là SHA-256 của exact canonical receipt JSON bytes; hash không nằm trong receipt để tránh self-reference.
- Immutable `evidence.receipt.recorded` event là external hash anchor và phải chứa receipt ID/hash. Integrity verification resolve đúng một matching recording event; missing hoặc ambiguous anchor là invalid, không chỉ advisory.
- ID không derive từ content hash vì hai independent observations có thể có cùng semantic payload nhưng khác actor/time/provenance.
- Same ID + same canonical bytes là idempotent `unchanged`.
- Same ID + different bytes là `receipt_id_conflict`; không overwrite.

### Common envelope

```jsonc
{
  "schema_version": 1,
  "receipt_version": 1,
  "id": "rcpt_01J...",
  "kind": "supersession_reconciliation",
  "result": "passed",
  "actor": {
    "kind": "human",
    "id": "quannv"
  },
  "recorded_at": "2026-07-22T03:00:00Z",
  "subject": {
    "kind": "work",
    "id": "TK-031"
  },
  "bindings": {
    "work": [
      {"id": "TK-031", "revision": 4},
      {"id": "ST-014", "revision": 7}
    ],
    "source": {
      "kind": "git_commit",
      "commit": "7d31c2a...",
      "repository_id": "repo_01J..."
    },
    "content": [
      {
        "path": "works/TK-031/ticket.md",
        "sha256": "sha256:..."
      }
    ],
    "artifacts": [
      {
        "sha256": "sha256:...",
        "role": "review_notes"
      }
    ],
    "graph_fingerprint_observed": "sha256:..."
  },
  "payload": {
    "payload_version": 1
  }
}
```

### Common field rules

- `actor.kind`: Slice 3 support `human`, `agent`, `system`; ID non-empty và bounded.
- `result`: typed enum theo kind. Initial common vocabulary là `passed`, `failed`, `inconclusive`; mỗi kind có thể restrict subset.
- `subject`: stable typed reference; không dùng display title làm identity.
- `bindings.work`: unique IDs, exact revisions, deterministic sort.
- `bindings.source`: optional ở envelope nhưng required theo receipt kind khi contract yêu cầu.
- `bindings.content`: repository-relative path, no traversal/symlink escape, exact SHA-256.
- `bindings.artifacts`: digest phải resolve artifact hiện hữu khi record/verify.
- `graph_fingerprint_observed`: audit context của lúc review; validator không làm receipt stale chỉ vì unrelated graph mutation đổi global fingerprint.
- Work prose/acceptance được review phải có matching `bindings.content`; node revision một mình không chứng minh `works/**` chưa đổi.
- Receipt không embed raw prompt, full logs hoặc large diff. Các bytes đó đi qua artifact reference sau redaction/size policy.
- Canonical serializer sort object keys; arrays có semantic order phải được kind validator normalize hoặc validate deterministic ordering.

## Ba chiều validation

CLI/library không trả một `valid: true` duy nhất. Kết quả tối thiểu:

```jsonc
{
  "schema_version": 1,
  "receipt_id": "rcpt_01J...",
  "receipt_hash": "sha256:...",
  "integrity": {
    "status": "valid",
    "reason_codes": []
  },
  "bindings": {
    "status": "current",
    "reason_codes": []
  },
  "authorization": {
    "status": "not_evaluated",
    "reason_codes": ["authority_resolver_unavailable"]
  },
  "gate_eligible": false
}
```

### Integrity status

- `valid`: canonical bytes, schema, typed payload, references và artifact hashes hợp lệ.
- `invalid`: tampering, malformed schema, missing artifact, unsupported enum/reference hoặc kind invariant fail.
- `unsupported_version`: envelope/payload/schema version binary không hiểu.

### Binding status

- `current`: mọi required current bindings khớp.
- `stale`: work revision, content hash hoặc source snapshot không còn khớp target hiện tại.
- `not_checked`: caller không yêu cầu current validation hoặc repository context thiếu.
- `unsupported`: binding mode như dirty source chưa được Slice 3 hỗ trợ.

### Authorization status

- `not_evaluated`: mặc định Slice 3; actor/assertion được preserve nhưng chưa có repository policy resolver.
- `structurally_declared`: optional label khi receipt có required approval fields, nhưng không đồng nghĩa authorized.
- `authorized` **không được trả trong Slice 3** trừ khi một authority resolver riêng được proposal/review chấp nhận sau này.

`gate_eligible` chỉ true khi gate consumer có policy rõ cho phép combination hiện có. Với Slice 3 standalone, output mặc định false.

Supersession integration không cấp authority mới. Nó tiếp tục mutation boundary đã được Slice 2 mở cho control-plane caller, nhưng thay inline assertion bằng immutable receipt. Receipt không biến Worker thành conductor, không cho actor tự đổi acceptance và không được dùng để suy ra quyền từ `actor.kind`. Cho tới khi authority resolver tồn tại, orchestration/CLI policy phải giữ supersession command ngoài Worker execution scope; Slice 3 chỉ enforce integrity/current bindings, không claim authorization.

## Source identity

### Clean Git commit binding

Slice 3 support source binding tối thiểu:

```jsonc
{
  "kind": "git_commit",
  "commit": "full-40-hex-oid",
  "repository_id": "repo_01J..."
}
```

Rules:

- Commit phải resolve trong repository hiện tại.
- `repository_id` phải khớp stable ID trong evidence manifest; receipt từ repository identity khác bị reject.
- `--source <commit>` là target source identity chính. `--current` mặc định dùng current `HEAD` nhưng cho phép evidence-only descendants: nếu commits sau bound source chỉ thay `.pulse/evidence/**` và `.pulse/events/**`, source binding vẫn current cho source/content được review.
- Receipt kind yêu cầu source binding phải reject abbreviated/non-resolving commit.
- Clean-commit receipt được record từ source commit đã tồn tại. Việc tạo tracked receipt/event sau đó không tự làm proof stale nhờ evidence-only-descendant rule.
- Dirty check chỉ áp dụng các source/content paths thuộc receipt scope; dirty changes chỉ trong evidence/event plane không làm source dirty. Dirty source/work/content ngoài supported snapshot làm `dirty_source_unsupported`.
- Slice 3 không hash dirty diff ad hoc.

### Vì sao defer dirty diff hash

Dirty snapshot canonicalization phải quyết định untracked files, file mode, rename, submodule, symlink, ignored/generated files và line-ending normalization. Một `git diff | sha256` đơn giản không đủ. Slice 3 defer algorithm này tới run/source-snapshot slice và không tạo compatibility debt.

## Content binding

Content binding dùng khi proof phụ thuộc exact file bytes, đặc biệt documentation và shaping artifacts.

Rules:

- Hash exact bytes của canonical repository file; không normalize Markdown/line endings sau khi đọc.
- Path phải repository-relative, không escape root và không resolve vào migration backup/protected path trái policy hiện có.
- Record mặc định kiểm tra file tồn tại và hash khớp payload trước commit.
- `verify --current` đọc canonical file hiện tại và phát hiện `content_binding_stale`.
- Path rename làm old binding stale; stable document identity/replacement handling thuộc Document Registry slice.
- Content hash không chứng minh semantic correctness; nó chỉ bind observation với bytes.

## Artifact store

### Artifact record

```jsonc
{
  "schema_version": 1,
  "algorithm": "sha256",
  "digest": "sha256:abcdef...",
  "size_bytes": 4812,
  "media_type": "text/plain",
  "kind": "review_notes",
  "original_name": "supersession-review.txt",
  "redaction": {
    "status": "caller_asserted",
    "notes": null
  },
  "created_at": "2026-07-22T02:55:00Z"
}
```

### Put protocol

```text
1. validate source path and size limit
2. stream bytes and calculate SHA-256
3. derive digest directory
4. acquire repository WriteGuard
5. recover/refuse unresolved prior transaction
6. if same digest content exists, verify bytes/hash and return unchanged
7. write content + metadata through ordered multi-target transaction
8. emit one evidence.artifact.recorded event
9. release guard
```

Requirements:

- Content bytes are identity; metadata conflict không được rewrite existing artifact silently.
- Same bytes với harmless new original filename có thể trả existing artifact; filename không thuộc digest identity.
- Digest collision/hash mismatch là hard corruption.
- Artifact verify rehashes actual bytes, không tin metadata.
- Symlink input được reject mặc định để tránh accidental secret/path escape.
- Empty artifact allowed only khi kind schema cho phép; default reject.
- No directory/archive ingestion trong Slice 3.

Artifact event không chứa artifact bytes, chỉ digest, size, kind và metadata hash.

## Typed receipt kinds

Slice 3 chỉ support ba kind có consumer rõ trong Phase 1.

### 1. `supersession_reconciliation`

Mục đích: thay `SupersessionAssertion` provisional của Slice 2 bằng immutable independently-verifiable identity.

Payload:

```jsonc
{
  "payload_version": 1,
  "old": {"id": "TK-031", "revision": 4},
  "target": {
    "kind": "replacement",
    "id": "ST-014",
    "revision": 7
  },
  "claim": "absorbed",
  "follow_up_work": [],
  "review_summary": "Acceptance has been absorbed by ST-014.",
  "reviewed_references": ["TK-031", "ST-014"]
}
```

Alternative Decision target:

```jsonc
{
  "target": {
    "kind": "decision_explanation",
    "id": "DEC-006",
    "revision": 2
  }
}
```

Rules cơ học:

- `old` và target phải xuất hiện trong `bindings.work` cùng revision.
- Exact owning acceptance/work prose đã review cho old/target/Decision phải xuất hiện trong `bindings.content`; `passed` receipt thiếu content bindings bị reject.
- Source commit binding là required để giữ review trên một repository snapshot cụ thể.
- `old` tồn tại và status tại thời điểm record thuộc supersedable set.
- Replacement/Decision kind phải đúng với graph hiện tại.
- `claim` là `absorbed` hoặc `follow_up_required`.
- `follow_up_required` phải có ít nhất một referenced work item tồn tại và bound revision.
- `absorbed` có thể có follow-up advisory nhưng không bắt buộc.
- Receipt validator không đọc prose để chứng minh acceptance thật sự được hấp thụ.
- `result` phải là `passed` để supersession command dùng.
- Authorization vẫn `not_evaluated` trong Slice 3.

### 2. `shaping_validation`

Mục đích: khóa schema/reference foundation để slice readiness sau có thể bind semantic shaping result với exact work/content revisions.

Payload tối thiểu:

```jsonc
{
  "payload_version": 1,
  "owning_work": {"id": "TK-031", "revision": 4},
  "risk": "R1",
  "destination": null,
  "branch_summary": {
    "resolved": ["error_mapping"],
    "rejected": [],
    "delegated": ["internal_helper_name"],
    "deferred": [],
    "blocking": []
  },
  "remaining_uncertainty": [],
  "approval_assertion": {
    "required": false,
    "reference": null
  }
}
```

Rules cơ học:

- Owning work ID/revision phải bound.
- `blocking` non-empty làm receipt không gate-eligible cho future ready composer, dù receipt integrity vẫn valid.
- `delegated`/`deferred` entries cần structured references/rationale trong schema thật; ví dụ trên rút gọn.
- Persisted destination/map khi có phải content-bound.
- Kernel không tự đánh giá branch list đầy đủ, recommendation đúng hay ambiguity semantic đã resolve.
- Slice 3 record/verify kind này nhưng **không** mở `draft -> shaped` hoặc `shaped -> ready`.

### 3. `documentation_validation`

Mục đích: chứng minh exact document bytes đã qua declared checks trên source snapshot; owner/registry/policy validity defer Slice 4.

Payload:

```jsonc
{
  "payload_version": 1,
  "documents": [
    {
      "proposed_document_id": "DOC-AUTH-DOMAIN",
      "path": "docs/domain/token-lifecycle.md",
      "content_hash": "sha256:...",
      "result": "passed"
    }
  ],
  "checks": [
    {"kind": "link_check", "result": "passed", "artifact": null},
    {
      "kind": "semantic_review",
      "result": "passed",
      "artifact": "sha256:..."
    }
  ]
}
```

Rules cơ học:

- Mỗi document path/hash phải có matching content binding.
- Source binding và document content bindings là required cho mọi `documentation_validation` result, kể cả `failed` hoặc `inconclusive`; attempt không có snapshot phải được ghi như finding/event khác, không phải documentation receipt.
- Required check set chưa được kernel tự invent; receipt chỉ validate declared typed checks trong Slice 3.
- `proposed_document_id` là optional/untrusted cho tới khi registry tồn tại.
- Slice 4 sẽ resolve stable document ID, owner, authority, scope, review policy và current/superseded lifecycle.
- Thay đổi file bytes làm receipt `stale`, không auto-update.

## Receipt record protocol

```text
1. parse typed envelope + payload
2. acquire repository WriteGuard
3. bootstrap evidence plane và recover pending transactions
4. load graph/source/content/artifact references
5. validate schema, kind invariants và current bindings required at record time
6. canonicalize envelope and calculate receipt hash
7. prepare evidence.receipt.recorded event
8. commit receipt file + event qua existing single-target transaction primitive
9. release guard
```

Receipt file là create-new. Transaction primitive phải bổ sung commit-time target mode `create_new`: ngay trước canonical write, target vẫn phải absent; nếu file xuất hiện sau prepare thì compare bytes để trả `unchanged` hoặc `receipt_id_conflict`, tuyệt đối không `atomic_replace` đè file ngoài plan. Không dùng graph fingerprint làm receipt filename hoặc mutation counter.

### ID generation và retry

CLI proposal:

- Receipt file input có thể chứa explicit `id`; đây là path khuyến nghị cho replay/idempotent automation.
- Nếu thiếu ID, kernel generate ID và trả canonical recorded receipt. Caller muốn retry sau timeout phải dùng returned ID/operation receipt hoặc kiểm tra event; public automation nên pre-allocate ID qua library helper.
- Không dùng semantic dedup để tự merge hai receipts khác ID. Hai independent observations có thể cùng payload nhưng vẫn là hai evidence records hợp lệ.

### Receipt event

Event type:

```text
evidence.receipt.recorded
```

Payload tối thiểu:

```jsonc
{
  "receipt_id": "rcpt_01J...",
  "receipt_kind": "documentation_validation",
  "receipt_hash": "sha256:...",
  "subject": "TK-031",
  "result": "passed"
}
```

Event không duplicate full payload, actor claim hoặc artifacts list. Receipt là canonical source cho detail.

## Supersession integration

### CLI evolution

Slice 2:

```text
pulse work supersede ... --assertion <versioned-json-file>
```

Slice 3 proposal:

```text
pulse work supersede <old-id>
  (--by <replacement-id> | --decision <decision-id>)
  --expected-revision <n>
  --reason <text>
  --reconciliation-receipt <receipt-id>
  --actor <actor>
  [--json]
```

`--assertion` không còn được quảng bá cho mutation mới. Vì reboot CLI vẫn pre-contract, proposal chọn deliberate breaking change thay vì duy trì hai canonical forms vô hạn.

### Preconditions

Trước graph mutation, store phải verify:

- receipt tồn tại và integrity valid;
- kind là `supersession_reconciliation`;
- result là `passed`;
- receipt old ID/revision khớp command old/expected revision;
- receipt target form/ID/revision khớp replacement hoặc Decision hiện tại;
- follow-up references tồn tại và revisions khớp nếu claim yêu cầu;
- receipt hash khớp canonical bytes và matching recording event anchor;
- receipt không stale đối với pre-mutation work/content/source bindings liên quan.

Global graph fingerprint đổi do unrelated mutation không tự làm receipt invalid. Nếu graph mutation ảnh hưởng old, target, follow-up hoặc supersession path, explicit revisions/graph preconditions làm operation fail.

Retry là trường hợp đặc biệt: sau supersession thành công, old revision tăng nên receipt không còn `current` theo pre-mutation view. Store phải kiểm tra existing supersession event + receipt ID/hash + target + pre-mutation revision trước current-binding validation. Nếu tất cả khớp, trả `unchanged`; receipt vẫn là valid historical basis của operation dù không còn current cho một mutation mới.

### Supersession event/output

Event `work.node.superseded` chỉ giữ:

```jsonc
{
  "reconciliation_receipt": {
    "id": "rcpt_01J...",
    "hash": "sha256:..."
  }
}
```

Nó không copy payload claim lần nữa.

`SupersededWork` output thay field `assertion` bằng receipt summary/reference. Idempotent retry so sánh canonical target + receipt ID/hash. Cùng target nhưng receipt khác phải conflict rõ, trừ future correction policy được thiết kế riêng.

### Historical compatibility boundary

- Historical Slice 2 events có inline `assertion` vẫn parse/audit được bằng versioned event reader hoặc raw event inspection.
- Không rewrite event cũ thành receipt giả.
- Existing superseded nodes không bắt buộc backfill receipt để graph validate.
- New mutation path chỉ tạo receipt-reference event.
- Nếu implementation cần bump event payload schema, bump explicit version/decoder; không infer bằng missing field im lặng.

## CLI surface của slice

```text
pulse evidence artifact put <path>
  --kind <kind>
  [--media-type <type>]
  [--original-name <name>]
  [--json]

pulse evidence artifact show <sha256-digest> [--json]
pulse evidence artifact verify <sha256-digest> [--json]

pulse evidence receipt record --file <receipt.json> [--json]
pulse evidence receipt show <receipt-id> [--json]
pulse evidence receipt list
  [--kind <kind>]
  [--subject <work-id>]
  [--result <result>]
  [--json]
pulse evidence receipt verify <receipt-id>
  [--current]
  [--source <commit>]
  [--json]

pulse work supersede ... --reconciliation-receipt <receipt-id>
```

Public root docs hiện minh họa `pulse evidence show|verify`; proposal dùng explicit `artifact`/`receipt` subcommands để tránh ambiguity. Sau khi review có thể thêm aliases ngắn, nhưng typed namespace là canonical CLI contract.

Deferred:

```text
pulse evidence receipt revoke|supersede|delete
pulse evidence gc|push|pull
pulse verify
pulse qa receipt verify
pulse work ready|close|packet
pulse docs validate
```

## Library/module layout đề xuất

```text
src/
  evidence/
    mod.rs
    manifest.rs            # evidence layout/schema bootstrap
    model.rs               # envelope, bindings, typed payload enum
    artifact.rs            # digest layout, put/show/verify
    receipt.rs             # immutable receipt CRUD/read projections
    validate.rs            # integrity/current-binding validation
    store.rs               # EvidenceStore; lock/recovery orchestration
  source/
    mod.rs                 # minimal repository identity + clean commit resolver
  schema/
    evidence/
      receipt-envelope.v1.schema.json
      supersession-reconciliation.v1.schema.json
      shaping-validation.v1.schema.json
      documentation-validation.v1.schema.json
  graph/
    store.rs               # only thin receipt-reference integration for supersession
  bin/
    pulse.rs

tests/
  evidence_artifacts.rs
  evidence_receipts.rs
  evidence_validation.rs
  evidence_cli_contract.rs
  supersession_receipt_integration.rs
  evidence_crash_recovery.rs
```

Boundary rules:

- `EvidenceStore` reuse `WriteGuard`, canonical JSON, atomic replace và transaction primitives.
- Không tạo public `Store<T>` abstraction chỉ vì graph/evidence đều là JSON.
- Shared transaction primitive có thể đổi tên/module để bỏ graph-specific assumption, nhưng API chỉ generalize đến mức hai concrete adapters cần.
- Typed payload dùng Rust enum (`#[serde(tag = "kind")]` hoặc envelope dispatch equivalent), không expose arbitrary map.
- Binary chỉ parse CLI, call typed API và render stable JSON/human output.

## Transaction và recovery

### Receipt write

Receipt + event là single canonical target + immutable event, reuse Slice 1 transaction protocol:

- before: absent với commit-time `create_new` precondition;
- after: receipt hash;
- event payload prepared/durable trước canonical write;
- crash sau receipt trước event được recovery hoàn tất event;
- all-before cleanup;
- unexpected bytes/event mismatch stop và preserve evidence.

### Artifact write

Artifact content + metadata + event cần multi-target ordered roll-forward từ Slice 2:

1. content target;
2. metadata target;
3. event.

After payload phải durable trước target đầu tiên. Recovery chỉ roll forward planned prefix. Nếu content đã tồn tại đúng digest, operation có thể treat content target as verified pre-existing và only create missing compatible metadata/event qua explicit idempotent plan; không giả overwrite.

Current multi-target intent embeds payload bytes, nên Slice 3 giới hạn artifact ở bounded small files và không claim streaming end-to-end. Hash calculation có thể stream input, nhưng commit/recovery có thể buffer payload trong configured 16 MiB ceiling. File-backed staged payload/content-addressed recovery blob là follow-up trước khi nâng size limit.

### Read consistency

- Receipt/artifact `show`, `verify` và list giữ repository guard xuyên recovery + read theo read-consistency contract Slice 2, hoặc dùng snapshot mechanism nếu sau này thay đổi.
- Readers không quan sát metadata without content hoặc receipt without deterministically recoverable event trong supported crash model.
- Support boundary về `.pulse/runtime/` intent preservation từ Slice 1/2 vẫn áp dụng; proposal không claim unconditional audit completeness nếu runtime intent bị xóa sau canonical write.

## Validation error contract

Stable error/reason codes tối thiểu:

```text
receipt_not_found
receipt_id_conflict
receipt_schema_invalid
receipt_kind_unsupported
receipt_version_unsupported
receipt_hash_mismatch
receipt_recording_event_missing
receipt_recording_event_ambiguous
receipt_subject_mismatch
receipt_result_ineligible
work_binding_missing
work_binding_stale
content_binding_missing
content_binding_stale
source_binding_missing
source_binding_stale
dirty_source_unsupported
repository_identity_mismatch
artifact_not_found
artifact_hash_mismatch
artifact_metadata_conflict
artifact_too_large
artifact_path_unsafe
authority_resolver_unavailable
supersession_receipt_mismatch
```

Errors parse/schema/integrity trả non-zero. `verify` có thể trả structured report với non-zero khi integrity invalid hoặc required current binding stale. Authorization `not_evaluated` không tự là process error cho plain verify, nhưng gate consumer phải coi nó theo policy và Slice 3 không được render thành “approved”.

## Listing và projection

`receipt list` scan `receipts/*.json`, validate filename/basic envelope và deterministic sort theo `(recorded_at, id)` hoặc explicit order contract.

Output không phải writable index:

```jsonc
{
  "schema_version": 1,
  "receipts": [
    {
      "id": "rcpt_01J...",
      "kind": "shaping_validation",
      "subject": {"kind": "work", "id": "TK-031"},
      "result": "passed",
      "recorded_at": "...",
      "receipt_hash": "sha256:..."
    }
  ]
}
```

Nếu benchmark cần cache, cache phải disposable/fingerprinted và không được trở thành gate truth. Slice 3 chưa cần cache.

## Test matrix

| ID | Scenario | Roadmap | Kỳ vọng |
| --- | --- | ---: | --- |
| E1 | Bootstrap evidence plane trống | prerequisite | Layout/schema đúng, không overwrite existing contract |
| E2 | Put artifact nhỏ | evidence | Digest/path/metadata deterministic; event đúng |
| E3 | Put cùng bytes lần hai | idempotency | `unchanged`, không duplicate content/event |
| E4 | Tamper artifact bytes | integrity | `artifact_hash_mismatch` |
| E5 | Unsafe/symlink/oversize artifact | security | Reject trước commit |
| E6 | Record typed receipt hợp lệ | Phase 1 receipt | One file + one event, stable hash/output |
| E7 | Same receipt ID/same bytes retry | idempotency | `unchanged`, không duplicate event |
| E8 | Same receipt ID/different bytes | integrity | `receipt_id_conflict`, preserve original |
| E9 | Unknown kind/version | schema | Fail rõ, không generic-accept payload |
| E10 | Missing/tampered artifact reference | integrity | Receipt record/verify fail |
| E11 | Work revision đổi sau record | binding | `work_binding_stale` với expected/current revision |
| E12 | Documentation bytes đổi sau review | #22 | `content_binding_stale` |
| E13 | Clean commit source khớp | source | Current source binding pass |
| E14 | Evidence-only commit sau bound source | source | Vẫn current; receipt commit không tự invalidate proof |
| E15 | Source/content path dirty với source-required receipt | source boundary | `dirty_source_unsupported`, không hash ad hoc |
| E16 | Receipt từ `repository_id` khác | security | `repository_identity_mismatch` |
| E17 | Global graph fingerprint đổi do unrelated node | invalidation | Receipt vẫn current nếu explicit bindings không đổi |
| E18 | Supersession receipt wrong kind | #16 | Reject, graph không mutate |
| E19 | Supersession receipt stale old/target revision | #16 | Reject, graph không mutate |
| E20 | Valid replacement reconciliation receipt | #16 | Atomic node+edge mutation; event reference receipt ID/hash |
| E21 | Valid Decision explanation receipt | #16 | Node mutation; event reference receipt ID/hash |
| E22 | Supersession retry same target/receipt sau old revision đã tăng | idempotency | Match historical event/receipt basis và trả `unchanged` trước new-mutation freshness check |
| E23 | Supersession retry same target/different receipt | history | Conflict rõ, không silently rewrite provenance |
| E24 | Historical Slice 2 assertion event | compatibility | Vẫn inspect/parse; không backfill/rewrite |
| E25 | Shaping receipt có blocking branches | D-43/D-44 | Integrity valid, future gate eligibility false |
| E26 | Documentation receipt bất kỳ result thiếu source/content binding | D-29 | Reject receipt; snapshotless attempt dùng finding/event khác |
| E27 | Receipt approval claim không có authority resolver | authority boundary | `authorization=not_evaluated`, không report approved |
| E28 | Crash after receipt before event | recovery | Recovery emit đúng một event |
| E29 | Crash after artifact content before metadata | recovery | Ordered roll-forward hoàn tất metadata/event |
| E30 | Manual conflicting edit during recovery | recovery | Stop, preserve intent/evidence |
| E31 | Concurrent writers record different receipts | concurrency | Cả hai thành công; có thể serialize, không shared canonical file conflict |
| E32 | Concurrent same-ID receipt writes | concurrency | Một created; bên kia unchanged hoặc conflict theo bytes |
| E33 | JSON CLI contracts | contract | Stable schema/code/non-zero exits |

Crash tests phải có failpoint tại intent/content/metadata/receipt/event boundaries và ít nhất một kill-process integration test. Artifact hash calculation nên stream input, nhưng Slice 3 chấp nhận bounded buffering trong transaction/recovery theo max artifact size đã cấu hình.

## Definition of Done của slice

- [ ] Evidence manifest có stable generated `repository_id`; typed schemas được version/hash-map immutable và bootstrap idempotent, không overwrite unknown contract.
- [ ] Receipt envelope có stable ID, canonical hash, typed subject/actor/result/bindings và deny unknown fields ở nơi contract yêu cầu.
- [ ] Chỉ ba receipt kinds trong scope được accept; arbitrary payload bị reject.
- [ ] One receipt per file, commit-time `create_new`, same-ID idempotency/conflict semantics rõ; transaction không overwrite file xuất hiện sau prepare.
- [ ] Receipt + event recovery pass mọi supported failpoint trong crash model đã khóa.
- [ ] Artifact store content-addressed theo SHA-256, verifies actual bytes và rejects unsafe/oversize inputs.
- [ ] Artifact content + metadata + event dùng ordered multi-target recovery, không sequential independent writes giả atomic.
- [ ] Work revision, clean Git source, stable repository ID, work-prose/content hash và artifact bindings có typed validators; evidence-only descendants không tự invalidate source proof.
- [ ] Global graph fingerprint chỉ là observed context; unrelated graph mutation không invalid receipt mặc định.
- [ ] Dirty worktree source binding trả unsupported rõ, không dùng unstable shortcut.
- [ ] Validation output tách integrity, binding freshness và authorization.
- [ ] Slice 3 không trả `authorized` nếu chưa có authority resolver.
- [ ] Supersession mutation mới yêu cầu `supersession_reconciliation` receipt integrity-valid và current theo pre-mutation bindings; retry cùng operation resolve qua historical event/receipt basis.
- [ ] Supersession event reference receipt ID/hash và không duplicate full claim payload.
- [ ] Historical Slice 2 inline assertions vẫn inspect được mà không rewrite.
- [ ] Shaping receipt schema giữ branch summary, remaining uncertainty, work/content revisions nhưng không tự đánh giá semantic completeness.
- [ ] Documentation receipt bắt source/content binding nhưng không giả registry owner/authority.
- [ ] CLI/library boundary vẫn thin CLI + typed APIs.
- [ ] Process concurrency, crash recovery, tamper và JSON contract tests pass.
- [ ] Rust format, clippy và full test suite sạch theo repository policy.
- [ ] Không mở full `ready`, `active`, `verifying`, `done`, work packet hoặc docs registry trong slice này.

## Handoff sang các slice tiếp theo

### Slice 4 — Document Registry + Applicable-Doc Projection

Dùng evidence foundation để thêm:

- `.pulse/docs/registry.json` và document schema;
- stable document ID/path/kind/owner/authority/scope;
- current/draft/stale/retired/superseded routing;
- `pulse docs list|show|applicable`;
- Ticket documentation impact posture;
- exclusion migration backup/generated navigation;
- nâng documentation receipt từ content-valid lên registry/policy-aware validation.

### Slice 5 — Docs Section Extraction + Lexical Retrieval

Sau registry identity ổn định:

- comrak heading-aware sections;
- generated `_index.md`;
- Tantivy BM25 cache;
- `pulse docs index|status|search|get|tree`;
- retrieval fingerprints/evals/context budgets.

### Slice 6 — Knowledge Store Foundation

Đóng requirement Phase 1 còn thiếu về canonical learning identity:

- one-learning-per-record sharded JSON;
- typed provenance/relations;
- revision CAS và immutable events;
- status/confidence/applicability/promotion/freshness schema;
- deterministic fingerprint và disposable snapshot boundary.

### Slice 7 — Shaping State + Readiness/Frontier Projections

Dùng typed shaping receipt + docs applicability + structural executability để
compose:

```text
implementation contract
+ critical branch dispositions
+ work/content-bound shaping receipt
+ documentation impact/applicable docs
+ required Decisions/content references
+ graph/dependency validity
= dispatch readiness
```

Slice 7 mới mở gate `draft -> shaped`, `shaped -> ready`, decision/execution
frontier và `pulse work ready`. Final `pulse work packet`, runner, lease và
reconciliation execution thuộc Phase 2; QA impact/baseline thuộc Phase 3.

## Risks và open questions cho review

1. **Repository fork/re-home semantics:** stable generated `repository_id` được copy cùng repo; explicit fork muốn identity mới cần command/migration nào, và receipt cũ được import/reference ra sao?
2. **Tracked artifact policy:** artifact nhỏ có track mặc định không, hay mọi artifact gitignored còn receipt metadata tracked? Product direction nói receipt metadata có thể track và artifact theo retention policy; Slice 3 cần chọn fixture/default rõ nhưng không claim universal policy.
3. **Runtime intent loss:** receipt/event atomicity vẫn phụ thuộc preserved `.pulse/runtime/` trong crash model hiện tại. Có cần tracked pending marker trước Core v1 public claim không?
4. **Authority enforcement surface:** Slice 3 không suy ra authority từ receipt hay `actor.kind`; command phải tiếp tục chỉ khả dụng ở control-plane scope. Trước public multi-actor usage, policy/authority resolver nào sẽ enforce human/Orchestrator grants và audit override?
5. **Receipt correction:** typo trong immutable receipt tạo receipt mới nhưng relation nào đánh dấu replacement? Slice 3 defer; list consumer phải tránh “latest wins” heuristic.
6. **Global graph fingerprint:** giữ field observed có hữu ích đủ để justify không? Nếu có, validators phải tuyệt đối không over-invalidate unrelated work.
7. **Content rename:** trước registry, path-bound documentation receipt stale khi rename dù bytes giữ nguyên. Đây là expected conservative behavior; Slice 4 mới resolve stable document ID.
8. **Artifact metadata:** metadata event/time khác nhau cho cùng digest có conflict hay chỉ preserve first observation? Proposal ưu tiên first canonical artifact metadata + receipt-specific role ở receipt binding.
9. **Receipt ID generation:** preallocated caller ID giúp retry nhưng tăng malformed-ID input. Cần helper/CLI UX nào để automation an toàn?
10. **Evidence-only ancestry:** exact path allowlist và merge-commit handling nào đủ để một receipt/event commit không tự invalidate bound source mà vẫn không bỏ sót source change?
11. **Event envelope typing:** current event actor/subject là strings. Evidence events có thúc đẩy migration sang typed actor/subject ngay Slice 3 hay chỉ payload typed? Không nên mở migration lớn nếu không cần acceptance.
12. **Schema validation engine:** code hiện chủ yếu typed-deserialize/manual validation. Có cần JSON Schema runtime validation ngay slice này hay embedded schema + typed Rust validator đủ cho acceptance? Repository schema drift vẫn phải validate exact.
13. **Source-required kinds:** shaping receipt cho content-only planning có luôn cần Git commit không, hay work revisions + content hashes đủ? Proposal yêu cầu source khi claim liên quan code state; kind validator cần explicit matrix.
14. **Receipt result vocabulary:** `passed|failed|inconclusive` có đủ cho shaping/docs foundation không, hay shaping nên dùng `accepted|rejected`? Tránh một enum quá generic làm mất semantics.
15. **Artifact redaction:** `caller_asserted` chưa chứng minh no-secret. Slice 3 nên size/path guard và warning; blocking secret scanner thuộc harness/policy slice sau.
16. **Supersession receipt replacement:** cùng target nhưng reviewer tạo receipt mới trước mutation nên dùng receipt nào? Command chọn explicit one; retry khác receipt conflict để provenance không đổi silently.

## Không quyết định trong slice này

Slice này không chốt document ownership/applicability, semantic shaping quality, QA environment/executor, verification profile, actor authorization, assignment ownership, close gate, retention/remote storage hoặc dirty snapshot algorithm. Nó chỉ chốt nền identity và proof mechanics để các capability đó có thể reference immutable evidence, phát hiện stale/tamper deterministic và không tiếp tục nhét semantic claims trực tiếp vào event payload.
