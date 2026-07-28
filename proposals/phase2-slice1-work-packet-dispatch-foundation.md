# Phase 2 — Slice 1: Bounded Work Packet + Dispatch Preparation Foundation

> Trạng thái: **implemented and verified** for Phase 2 Slice 1. Implementation
> landed across commits `ed68c08`, `823607e`, `e47f90d`, `cf65ec4`,
> `d236774`, `7f62545`, `e28c5c0`, `ddef30a` and verification hardening commit
> `6d3076b`; supporting fix/verification commits are listed in the completion
> evidence below. This remains a pre-Core-v1 current baseline, not a released
> compatibility contract.
> Tiền đề:
> [`phase1-slice7-shaping-readiness-frontier.md`](phase1-slice7-shaping-readiness-frontier.md)
> đã được implement và verify tại commit `677c593`.
> Sở hữu: implemented Slice 1 behavior for the first Phase 2 slice: packet thực
> thi versioned/bounded, coherent packet snapshot, exact source-base identity,
> workspace/lease/capability requirements và projection chuẩn bị dispatch không
> tự nhận đã reserve hoặc chạy Agent.
> Tham chiếu normative:
> [`PULSE_REBOOT.md`](../PULSE_REBOOT.md),
> [`02-work-graph.md`](../pulse-reboot/02-work-graph.md),
> [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md),
> [`06-priority-reconciliation.md`](../pulse-reboot/06-priority-reconciliation.md),
> [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md),
> [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md),
> [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md),
> [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md),
> [`11-documentation-retrieval.md`](../pulse-reboot/11-documentation-retrieval.md),
> [`12-knowledge-compounding.md`](../pulse-reboot/12-knowledge-compounding.md).

## Tóm tắt quyết định

Slice này triển khai đúng một public query mới:

```bash
pulse --repo-root <repo> work packet <ticket-id> --json
```

Command tạo một packet deterministic và bounded cho **implementation Ticket** đã
có lifecycle `ready` và current Phase 1 readiness pass. Packet bind exact:

- Ticket `revision` và `contract_revision`;
- readiness profile/fingerprint;
- coherent graph fingerprint;
- shaping, Decision, content và authority identities đã tham gia readiness;
- current docs registry/applicability và ranked lexical section suggestions;
- exact clean Git `HEAD` dùng làm source base;
- stable repository identity;
- workspace strategy requirement, lease requirement và capability requirement;
- các gate Phase 2/3/4 chưa được cài, ở trạng thái typed
  `not_evaluated|not_installed`, không giả empty/pass.

Slice này **không acquire lease, không tạo worktree, không launch Agent và không
mở `ready -> active`**. Vì vậy mọi packet của Slice này giữ:

```json
{
  "dispatch": {
    "reservation_candidate": true,
    "dispatch_authorized": false,
    "authorization_status": "not_reserved"
  }
}
```

`reservation_candidate=true` chỉ có nghĩa mọi pre-reservation input do Slice
này sở hữu đã current và packet có thể được đưa cho Slice 2 để atomically
revalidate + acquire lease + materialize workspace. Nó không đồng nghĩa
`unclaimed`, `assigned`, `active`, có Agent phù hợp hoặc có quyền chạy.

Quyết định này giải circular ordering giữa packet, lease và workspace bằng hai
artifact/stage rõ ràng:

```text
Slice 1: preview packet
  = coherent canonical/source context + runtime requirements
  = read-only, no reservation, dispatch_authorized=false

Slice 2: prepared assignment
  = revalidate packet preconditions
  + acquire exclusive lease
  + materialize/bind workspace
  + evaluate concrete capability inventory
  + emit final assignment packet/record
  + permit gated ready -> active
```

Preview packet không được dùng trực tiếp làm bằng chứng rằng Agent được phép
start. Slice 2 phải revalidate exact preconditions; không được chỉ tin
`reservation_candidate` từ bytes cũ.

---

## Baseline đã implement trước Slice 1

Repository hiện đã có:

- sharded work graph, canonical JSON, repository write fence, CAS, recoverable
  transactions, immutable semantic events;
- typed implementation/decision-work contracts và separate
  `contract_revision`;
- structural executability, lifecycle, supersession, traversal và deterministic
  graph projection/fingerprint;
- completed shaping receipt v1, current shaping pointer/map, immutable Decision
  acceptance receipt;
- default-deny authority policy và deterministic policy fingerprint;
- Phase 1 readiness profile/fingerprint, stale-ready semantics và gated
  `draft -> shaped -> ready`;
- deterministic decision/execution frontier với
  `claim_state=not_evaluated`;
- docs registry/applicability gồm `required|optional|write_candidates|excluded`;
- section extraction, generated navigation, disposable Tantivy index,
  `docs search|get|tree` và bounded snippets;
- clean Git commit source binding/currentness cho evidence receipts;
- canonical knowledge store foundation nhưng chưa có applicable retrieval.

Current boundary được giữ nguyên:

- `ReadinessReport.dispatch_authorized` là `false`;
- execution frontier không persist claim state;
- `lease`, `source_workspace` và Phase 3 `qa_baseline_and_cases` là future gate
  families;
- lifecycle `ready` chỉ diễn tả current executable contract, không diễn tả
  assignment.

Slice 1 compose foundation trên; không sửa nghĩa Phase 1 readiness để nhét
runtime semantics.

---

## Vị trí của slice trong Phase 2

Phase 1 trả lời:

> Ticket có current executable contract đủ rõ và current để nằm trong execution
> frontier không?

Slice này trả lời:

> Với một exact coherent repository snapshot và clean source base hiện tại,
> bounded context nào phải được giao cho execution adapter, pre-reservation gate
> nào đã pass, và runtime requirements nào Slice 2 phải satisfy trước dispatch?

Slice tiếp theo mới trả lời:

> Có thể atomically reserve Ticket này cho assignee cụ thể và bind workspace
> cụ thể để mở `ready -> active` không?

Pipeline:

```text
ready implementation Ticket
  + current Phase 1 readiness snapshot
  + graph context
  + shaping/Decision/content bindings
  + docs applicability
  + docs lexical suggestions
  + authority policy
  + clean Git HEAD + repository identity
  -> coherent bounded WorkPacketV1
  -> reservation_candidate=true|false
  -> Slice 2 revalidation/reservation/workspace binding
```

---

## Mục tiêu

Triển khai để một caller có thể:

1. gọi `pulse work packet TK-... --json` trên target repository đã bootstrap;
2. nhận packet schema v1 có stable field names và deterministic ordering;
3. biết chính xác Ticket/revisions/readiness/graph/docs/source nào packet bind;
4. đọc objective, target, anchors, invariants, acceptance, expected proof và
   handoff mà không scan raw workgraph;
5. đọc parent/Decision/shaping/edge context cần thiết mà không reconstruct graph;
6. đọc required docs và bounded suggested sections mà không scan toàn docs tree;
7. biết context nào required, optional, excluded hoặc chưa được evaluator cài;
8. biết packet có phải reservation candidate hay không và vì gate family nào;
9. biết workspace mode/capability/lease nào Slice 2 phải materialize;
10. detect stale packet bằng explicit preconditions/fingerprint trước reservation;
11. xóa/rebuild disposable docs cache mà không đổi canonical packet semantics,
    trừ lexical scores/generation metadata được khai báo là advisory projection;
12. không tạo lease/workspace/run/Agent/canonical mutation như side effect.

---

## Non-goals

Slice này không triển khai:

- `pulse work claim|release`;
- assignment lease store, TTL, heartbeat, acknowledgement hoặc recovery;
- workspace record/store, worktree create/cleanup/adoption;
- concrete Worker/Agent capability inventory hoặc capability matcher;
- `dispatch_authorized=true`;
- `ready -> active` lifecycle gate;
- `pulse run`, Codex adapter, prompt transport, stream, timeout, cancel/resume;
- handoff receipt, verification runner, review, `active -> verifying`;
- close gate hoặc `done|rework|blocked` proof transition;
- full conversational `pulse-shape` hay automatic reconciliation;
- Story QA baseline/case resolver, QA executor hoặc QA receipt;
- `knowledge applicable`, knowledge BM25, packet learning injection;
- Agent Registry, typed mailbox, peer-agent orchestration;
- semantic priority scoring hoặc automatic work selection;
- dirty-worktree canonicalization;
- enforced filesystem sandbox/ACL từ free-text scope;
- vector/embedding/semantic retrieval;
- SQLite, daemon hoặc provider-neutral runtime abstraction;
- schema predecessor/migration chỉ vì đổi Phase/Slice.

---

## Các quyết định khóa cho proposal

### P2S1-D1 — Packet là preview artifact, không phải reservation

`work packet` là read-oriented preview query. Nó không acquire lease hoặc tạo
workspace. Output có:

- `reservation_candidate`: pre-reservation gates do Slice 1 sở hữu có pass hay
  không;
- `dispatch_authorized`: luôn `false` trong packet profile v1;
- `authorization_status`: luôn `not_reserved` khi packet build thành công.

Packet không dùng field `unclaimed=true`; lease state là `not_evaluated` vì
resolver chưa tồn tại.

Lý do:

- giữ query idempotent và không tạo ghost lease từ một context-preview command;
- tránh transaction giả xuyên graph/runtime/Git worktree;
- cho Slice 2 một precondition contract rõ để revalidate atomically;
- không nhập Phase 5 Agent Registry/ack semantics quá sớm.

### P2S1-D2 — Chỉ implementation Ticket lifecycle `ready`

Command chỉ build packet executable khi subject:

- tồn tại;
- kind `ticket`;
- role `implementation`;
- lifecycle `ready`;
- current readiness report status `ready`;
- `transition_eligible=true` dưới `phase1_contract_readiness_v1`.

Decision-work Ticket dùng decision frontier/shaping workflow, không dùng
implementation packet profile này. Ticket `draft|shaped|blocked|rework` nhận
non-zero error thay vì một packet giả. Ticket `ready` nhưng current readiness
stale nhận `work_packet_readiness_stale`.

### P2S1-D3 — Packet có fingerprint riêng, không thay readiness fingerprint

`readiness_fingerprint` tiếp tục là narrow semantic readiness identity.

Packet thêm `packet_fingerprint`, hash một **fingerprint projection riêng** gồm:

- packet profile/schema;
- subject ID, role, `revision`, `contract_revision`, status;
- readiness profile/fingerprint;
- coherent graph fingerprint;
- canonical execution contract projection;
- selected parent/relation/shaping/Decision projections;
- authority policy revision/fingerprint;
- docs registry revision/fingerprint;
- docs applicability projection;
- docs suggestion query and selected section identities/hashes/ranks;
- docs index content fingerprint, nhưng không generation ID/path/timestamp;
- repository ID;
- exact source base commit and cleanliness status;
- workspace strategy requirement;
- lease/capability/QA/knowledge evaluation statuses;
- static budget limits và truncation findings.

Fingerprint projection **không chứa**:

- `packet_fingerprint` của chính nó;
- `dispatch.revalidation_preconditions.packet_fingerprint`;
- `budget.actual_canonical_json_bytes`;
- CLI rendering;
- absolute repository path;
- cache generation ID;
- wall-clock packet generation time.

Packet JSON không chứa floating-point number. Lexical scores được quantize thành
integer micro-score theo P2S1-D13. Fingerprint giữ `rank`, `section_ref`,
document/section hashes, integer scores và reason codes vì những fields này mô
tả context thực sự caller đã nhận. Nếu selected order đổi thì fingerprint phải
đổi; proposal không claim fingerprint giống nhau giữa hai Tantivy/dependency
versions có ranking khác nhau.

### P2S1-D4 — Clean committed source only

Slice này chỉ tạo reservation candidate khi:

- target là Git repository;
- `HEAD` resolve thành full 40-hex commit;
- worktree dùng để query sạch với mọi tracked/untracked non-ignored path;
- repository identity đã tồn tại và match manifests;
- không có in-progress merge/rebase/cherry-pick/bisect.

Không dùng `git diff | sha256`; không invent dirty snapshot identity.

Low-risk in-place execution vẫn là direction Phase 2, nhưng Slice 1 chỉ output
`workspace.strategy=in_place_allowed` khi current workspace sạch. Slice 2 phải
revalidate cleanliness ngay trước lease/workspace commit. Medium/high/critical
output `isolated_worktree_required`.

### P2S1-D5 — Workspace là requirement, chưa phải allocated identity

Do Slice này không create workspace, packet không fabricate `workspace_id`.
Output tách:

```json
{
  "workspace": {
    "binding_status": "not_allocated",
    "required_strategy": "isolated_worktree",
    "base_source": {"commit": "..."},
    "workspace_id": null
  }
}
```

Slice 2 sở hữu workspace ID, record path, lifecycle và cleanup. Packet chỉ khóa
strategy và preconditions.

### P2S1-D6 — Risk-to-workspace mapping hiện tại

Mapping deterministic:

| Ticket risk | Workspace requirement |
|---|---|
| `low` | `in_place_allowed` |
| `medium` | `isolated_worktree_required` |
| `high` | `isolated_worktree_required` |
| `critical` | `isolated_worktree_required` |
| `unassessed` | packet reject; Phase 1 ready không hợp lệ |

`in_place_allowed` không bắt buộc Slice 2 dùng in-place; caller có thể nâng lên
isolated. Downgrade từ required isolated sang in-place không được phép. Packet
không tự dựa vào materialization để hạ workspace requirement.

### P2S1-D7 — Capability requirement là typed requirement, không phải inventory result

Packet derive required capability names từ typed contract:

- mọi implementation Ticket: `source.read`, `repository.inspect`;
- surface `code|configuration|data`: `source.write`;
- surface `documentation`: `docs.write`;
- non-empty expected `focused_test_output`: `test.run`;
- plan policy `required_before_execution`: `plan.materialize`;
- isolated workspace requirement: `workspace.worktree`;
- expected `documentation_diff`: `docs.write`;
- expected `decision_record`: `decision.propose`, không phải accept authority.

Vocab trên là current packet requirement vocabulary v1. Requirements được sort,
dedup và fingerprint. `capability_evaluation.status=not_evaluated` vì chưa có
concrete assignee inventory. `reservation_candidate` không fail chỉ vì concrete
inventory chưa tồn tại; Slice 2 phải match toàn bộ required capabilities trước
`dispatch_authorized=true`.

Authority grants và capability availability là hai plane riêng. Grant không
chứng minh runtime có tool; capability không cấp business authority.

### P2S1-D8 — Không fabricate enforced writable scope

Current contract có typed anchors nhưng chưa có canonical path ACL. Vì vậy
packet v1 output hai khái niệm:

- `scope_hints`: deterministic source/docs/config/data anchor paths và free-text
  included/excluded scope để Agent định hướng;
- `enforcement.status=not_installed`.

Field không được gọi `allowed_paths` hoặc `writable_scope`. Slice 2/3 phải thêm
canonical policy nếu muốn enforce path ACL. Packet vẫn mang hard authority stops
và implementation freedom; possession of packet không cấp quyền đổi acceptance,
approved Decisions hoặc contract docs.

### P2S1-D9 — Packet dùng cache-only docs refresh

Existing `docs index` vừa publish `.pulse/cache/docs-search/**` vừa materialize
tracked/generated `docs/**/_index.md`. Packet source base lại yêu cầu clean Git
HEAD, nên `work packet` **không được gọi nguyên xi** path đó: cache miss có thể tự
làm worktree dirty và invalidate packet.

Slice này phải refactor docs indexing thành hai internal operations dùng chung
capture/extraction/Tantivy code:

```text
build_search_cache(repo_root, options)
  -> publish cache generation + CURRENT only
  -> never write docs/**/_index.md

build_index(repo_root, options)
  -> build_search_cache internals
  -> write/validate generated projections for explicit docs index command
```

`work packet` chỉ được gọi `build_search_cache`. Allowed writes:

- `.pulse/cache/docs-search/**`;
- docs-search cache lock/files already owned by that cache subsystem.

Nó không được write generated navigation, canonical docs registry, evidence
manifest, graph, knowledge hoặc lease/workspace/run state. Nếu generated
projection đang stale, packet report advisory exclusion/finding metadata nếu
available nhưng không repair projection; generated freshness enforcement thuộc
`docs validate`/later close gate.

Nếu cache-only refresh cannot run theo policy, packet fail với packet-level docs
index error. `--no-refresh` chưa expose; packet luôn yêu cầu current search cache.
Cache generation ID không thuộc packet fingerprint. Content/index fingerprint
và selected section hashes thuộc fingerprint.

### P2S1-D10 — Knowledge là `not_installed`, không phải empty applicability

Packet luôn có knowledge section để future schema composition rõ:

```json
{
  "knowledge": {
    "status": "not_installed",
    "owner_phase": 4,
    "knowledge_fingerprint": null,
    "required": [],
    "recommended": [],
    "suggested": [],
    "excluded": []
  }
}
```

Empty arrays không mang nghĩa “đã search và không có hit”; `status` là source of
truth.

### P2S1-D11 — QA required vẫn không dispatchable

Current Phase 1 readiness đã block `qa.impact=required`. Slice này không tạo
profile bypass. Nếu malformed/manual state có lifecycle `ready` với required QA,
packet reject bằng `work_packet_qa_resolver_unavailable`.

`none` và `covered_by_story_close` chỉ packet được nếu current readiness đã pass
đúng authority/rationale gate.

### P2S1-D12 — Packet không quyết định priority

Packet không chứa hidden priority score và command không chọn Ticket. Nó chỉ
build context cho ID caller đưa vào. Stable output ordering dùng ID/type/rank;
soft preferences được report như context, không trở thành blocker.

### P2S1-D13 — Packet JSON không có float

Existing canonical JSON rejects floating-point values. `WorkPacketV1` vì vậy
không copy `f64 score` trực tiếp từ `SearchReport`.

Mapping bắt buộc:

```text
score_micros = round_nonnegative(score * 1_000_000)
lexical_score_micros = round_nonnegative(lexical_score * 1_000_000)
```

Rules:

- input phải finite và `>= 0`; violation => `work_packet_docs_score_invalid`;
- rounding dùng Rust `f64::round`, sau đó checked convert sang `u64`;
- integer scores chỉ để explain/rank trong current dependency baseline, không là
  public semantic relevance probability;
- packet schema chỉ có integer micro-score fields;
- canonical packet serialization, fingerprint và byte budget đều dùng existing
  float-rejecting canonical serializer.

### P2S1-D14 — Docs registry/index là packet prerequisite

Mọi executable work packet phải có existing current docs manifest/registry, kể
cả Ticket có documentation posture `none`. Lý do: work packet contract luôn cần
bounded repository knowledge routing và lexical suggestions; `none` chỉ nói
implementation không cần update durable docs, không nói repository không có docs
context.

Missing docs manifest/registry => `work_packet_docs_registry_missing`, không
bootstrap và không trả typed absent docs section. Empty current registry hợp lệ:
applicability buckets và suggestions empty, index vẫn có deterministic empty
corpus fingerprint nếu existing docs index policy supports it.

### P2S1-D15 — Repository fence lock file là allowed operational side effect

Existing `WriteGuard::acquire` có thể tạo:

```text
.pulse/runtime/locks/workgraph.lock
```

Packet cho phép side effect hẹp này và parent directory cần cho lock. Nó vẫn
không được tạo lease/workspace/run/presence state. Trên enrolled repository,
missing lock directory/file được create như generic coordination primitive; trên
non-enrolled repository command phải fail enrollment validation **trước** lock
acquisition để không bootstrap `.pulse/runtime`.

---

## Public CLI contract

### Command

```bash
pulse --repo-root <repo> work packet <ticket-id> --json
```

Human rendering:

```text
TK-031 packet: reservation candidate
source: <40-char HEAD> (clean)
readiness: current (<fingerprint>)
workspace: isolated worktree required
required docs: 2
suggested sections: 4
required capabilities: repository.inspect, source.read, source.write,
                       test.run, workspace.worktree
dispatch authorized: no (lease/workspace/capability assignment not evaluated)
packet fingerprint: sha256:...
```

Human output là convenience, JSON là machine contract.

### Arguments

- `<ticket-id>` required.
- `--json` existing global/domain rendering convention.
- Không có `--force`.
- Không có `--allow-dirty`.
- Không có `--include-not-ready`.
- Không có `--full-docs`.
- Không có `--claim` hay implicit mutation.

### Exit behavior

Exit `0` chỉ khi packet được tạo hoàn chỉnh và
`reservation_candidate=true`.

Non-zero khi subject/input invalid, stale, unavailable hoặc packet budget không
thể giữ required context. Không trả exit `0` với một partial packet mà caller có
thể nhầm là dispatchable.

Optional lexical search không có hit vẫn exit `0` với `suggested=[]`.

---

## WorkPacketV1 JSON contract

Current packet family dùng một baseline `schema_version: 1`; không tạo v2 cho
các internal Slice sau nếu pre-release contract được amended in place theo D-68.
Nếu sau release cần compatibility family mới mới xem xét version bump.

```json
{
  "schema_version": 1,
  "profile": "phase2_work_packet_preview_v1",
  "code": "reservation_candidate",
  "subject": {},
  "snapshot": {},
  "contract": {},
  "context": {},
  "shaping": {},
  "graph": {},
  "documentation": {},
  "knowledge": {},
  "source": {},
  "workspace": {},
  "capabilities": {},
  "scope": {},
  "assurance": {},
  "dispatch": {},
  "budget": {},
  "packet_fingerprint": "sha256:...",
  "reason_codes": []
}
```

Mọi Rust packet type dùng `#[serde(deny_unknown_fields)]` khi deserialize trong
tests/internal round-trip. Collections được normalize trước canonical hash.

### `subject`

```json
{
  "id": "TK-031",
  "kind": "ticket",
  "role": "implementation",
  "title": "Rotate refresh tokens atomically",
  "revision": 7,
  "contract_revision": 4,
  "status": "ready",
  "risk": "medium",
  "materialization": "R1",
  "content_dir": "works/TK-031"
}
```

`content_dir` là reference, không inline raw work directory.

### `snapshot`

```json
{
  "graph_fingerprint": "sha256:...",
  "readiness_profile": "phase1_contract_readiness_v1",
  "readiness_fingerprint": "sha256:...",
  "readiness_status": "ready",
  "authority_policy_revision": 1,
  "authority_policy_fingerprint": "sha256:...",
  "docs_registry_revision": 3,
  "docs_registry_fingerprint": "sha256:...",
  "docs_index_fingerprint": "sha256:...",
  "source_commit": "0123456789abcdef0123456789abcdef01234567"
}
```

Đây là precondition set Slice 2 phải revalidate. Không có wall-clock timestamp.

### `contract`

Packet dùng một explicit `PacketImplementationContractV1` DTO, không serialize
thẳng `ImplementationContract`. DTO map one-to-one từ current normalized model,
không parse `ticket.md` để reconstruct semantics. Mọi field dưới đây là
**required trong packet JSON**; absent/default canonical model values được emit
thành empty object/array hoặc `null` đúng schema, nên packet shape không phụ thuộc
`skip_serializing_if` của node model:

```json
{
  "mode": "guided",
  "work_surface": "code",
  "plan_policy": "worker_optional",
  "semantic_impact": "behavior_or_public_risk_change",
  "effort": {},
  "verification_profile": "service-change",
  "brief": {"path": "works/TK-031/ticket.md", "content_hash": "sha256:..."},
  "objective": "...",
  "current_behavior": "...",
  "target_behavior": "...",
  "code_anchors": [],
  "documentation_anchors": [],
  "configuration_anchors": [],
  "data_anchors": [],
  "research_refs": [],
  "required_changes": [],
  "invariants": [],
  "acceptance": [],
  "scope": {"included": [], "excluded": []},
  "implementation_freedom": [],
  "required_decisions": [],
  "shared_approach_refs": [],
  "expected_evidence": [],
  "expected_handoff": []
}
```

Brief/content refs đã được readiness content-integrity check. Packet không inline
full brief; Agent dùng path/ref khi cần prose detail.

### `context.parents`

```json
{
  "parents": [
    {
      "relation": "parent_of",
      "id": "ST-004",
      "kind": "story",
      "revision": 5,
      "contract_revision": 3,
      "status": "active",
      "title": "Reliable token lifecycle",
      "content_dir": "works/ST-004"
    }
  ]
}
```

Current node schema không có machine summary riêng; v1 dùng bounded `title` làm
summary. Không đọc/LLM-summarize parent prose. Parent sort theo
`relation,id` và chỉ include direct hierarchy parents; Epic ancestor include qua
bounded hierarchy chain tối đa 2 edges từ Ticket. Invalid/multiple hierarchy
parent rules tiếp tục do graph validator sở hữu.

### `context.decisions`

Mỗi required Decision:

```json
{
  "id": "DEC-007",
  "revision": 4,
  "contract_revision": 2,
  "status": "done",
  "title": "Use single-use refresh rotation",
  "acceptance_receipt": {
    "id": "rcpt_...",
    "hash": "sha256:..."
  },
  "content_refs": [
    {"path": "works/DEC-007/decision.md", "content_hash": "sha256:..."}
  ]
}
```

Chỉ include Decisions explicit trong implementation contract hoặc current
shaping payload; không lexical-discover arbitrary Decisions.

### `shaping`

Packet projection map losslessly từ current `ShapingValidationPayload`:

```json
{
  "status": "current",
  "receipt_id": "rcpt_...",
  "receipt_hash": "sha256:...",
  "owning_work": {
    "id": "ST-004",
    "revision_observed": 5,
    "contract_revision": 3
  },
  "shape_mode": "focused_branches",
  "destination": {
    "summary": "Deliver reliable refresh-token rotation",
    "scope_boundary": ["No session UI redesign"],
    "exit_conditions": ["Concurrent rotation acceptance passes"]
  },
  "map": {
    "path": "works/ST-004/shaping.md",
    "revision": 2,
    "content_hash": "sha256:..."
  },
  "critical_branches": [
    {
      "id": "BR-AUTH-1",
      "question": "How is concurrent rotation serialized?",
      "gap_kind": "tradeoff_gap",
      "affected_work": ["TK-031"],
      "disposition": {
        "kind": "resolved",
        "resolution": {
          "kind": "decision",
          "id": "DEC-007",
          "revision": 2,
          "gist": "Single-use atomic rotation"
        }
      }
    }
  ],
  "bounded_fog": [
    {
      "id": "FOG-AUTH-TELEMETRY",
      "statement": "Telemetry names may change",
      "bounds": ["No acceptance impact"],
      "why_not_precise": "Instrumentation not selected",
      "trigger": "Telemetry implementation starts",
      "affected_work": ["TK-031"]
    }
  ],
  "remaining_uncertainty": [
    {
      "summary": "Telemetry naming remains open",
      "trigger": "Telemetry implementation starts"
    }
  ],
  "decision_frontier": {
    "status": "evaluated",
    "items": []
  }
}
```

Exact mapping:

- `owning_work`, `shape_mode`, optional `destination`, optional `map` copy current
  typed receipt fields unchanged;
- `destination.exit_conditions` remains an ordered array; không collapse thành
  một string và không invent destination owner;
- `critical_branches` includes payload branches where
  `criticality=critical` **and** (`affected_work` contains subject ID or subject
  is the shaping `owning_work`); branch `question`, `gap_kind`, `affected_work`
  and tagged `disposition` copy losslessly;
- `bounded_fog` includes current `fog` entries where `affected_work` contains
  subject ID or subject is owning work; giữ ID và all bounded fields;
- `remaining_uncertainty` copies current receipt entries `{summary,trigger}`;
  không gọi chúng là fog IDs;
- all nested set-like arrays normalize như receipt validator; branches/fog by ID;
- không inline complete shaping-map file content;
- decision frontier scope theo `owning_work.id`, rồi chỉ include decision-work
  items whose `branch_id` matches an included critical branch; maximum 16;
- nếu >16, fail `work_packet_decision_frontier_overflow`, không truncate.

### `graph`

```json
{
  "structural_state": "executable",
  "hard_blockers": [],
  "soft_preferences": [],
  "supersession": null,
  "relations": {
    "outgoing": [],
    "incoming": []
  }
}
```

Relation projection:

- include every direct edge incident to subject;
- fields: edge ID/type/from/to/revision plus opposite endpoint ID/kind/status/
  revision/title;
- sort by `type,from,to,id`;
- max 128 incident edges;
- overflow là `work_packet_relation_overflow` vì silent omission có thể che
  blocker/context;
- hard blocker và supersession detail reuse structural report;
- soft preference không affect `reservation_candidate`.

### `documentation`

```json
{
  "applicability": {
    "status": "complete",
    "required": [],
    "optional": [],
    "write_candidates": [],
    "excluded": []
  },
  "suggestion_query": {
    "text": "...",
    "normalized_terms": []
  },
  "suggested_sections": [],
  "read_budget": {
    "required_sections": 0,
    "recommended_initial_sections": 4,
    "max_initial_lines": 240,
    "suggestion_limit": 8,
    "snippet_max_bytes_each": 500
  },
  "index": {
    "state": "current",
    "fingerprint": "sha256:...",
    "mode": "lexical"
  }
}
```

#### Required/optional documents

Reuse `ApplicableDocsReport` fields:

- stable ID/path/kind/authority/owner/summary;
- document revision/content hash;
- applicability reasons.

`write_candidates` phải join lại current applicable/registry record để packet
có ID/path/authority/owner/content hash/reasons; không chỉ trả bare ID.

`excluded` giữ path nếu known, replacement và reason codes.

Required applicability gate incomplete làm packet fail. Retired, superseded,
stale, migration/generated exclusions không bị route như current truth.

#### Deterministic suggestion query

Không dùng LLM. Query fragments theo thứ tự:

1. Ticket title;
2. `objective`;
3. `target_behavior`;
4. từng acceptance `summary`, theo acceptance ID;
5. basename và optional symbol của code/config/data/documentation anchors, sort
   theo normalized path/symbol;
6. documentation routing domains, rồi labels, sort lexical;
7. required Decision titles, sort Decision ID;
8. shaping destination `summary`, rồi từng `exit_conditions` theo receipt order;
9. critical branch `question`, sort branch ID.

Normalize:

- trim/collapse whitespace;
- concatenate bằng `" | "`;
- remove duplicate exact fragments preserving first occurrence;
- cap mỗi fragment 280 Unicode scalar values;
- feed lexical tokenizer;
- iterate unique normalized terms in tokenizer order and append a whole term only
  if both constraints remain true: at most 32 terms and reconstructed UTF-8 query
  byte length at most 256 when joined by one ASCII space;
- never cut a term; stop at the first term that would exceed 256 bytes because
  preserving later terms while skipping an earlier term would change priority;
- `suggestion_query.normalized_terms` is exactly the accepted prefix and
  `suggestion_query.text` is those terms joined by one ASCII space;
- nếu không còn term, packet fail `work_packet_docs_query_empty` vì ready
  implementation contract lẽ ra luôn có title/objective/target.

Search options:

- `limit=8`;
- `work=current WorkDocumentationContext`;
- `explain=true`;
- `include_draft=false`;
- `include_stale=false`;
- `no_refresh=false`;
- không kind/domain/authority override ngoài applicability context.

#### Required section routing

Current registry/applicability identifies documents, chưa identify exact required
sections. V1 không invent section semantics. Mỗi required document có:

- `required_section_refs=[]` nếu contract chỉ references document ID;
- `section_resolution_status=document_level_only`;
- Agent phải đọc document summary và dùng `docs tree/get/search` để resolve detail
  trong read budget;
- nếu explicit content/section ref đã tồn tại trong typed contract/shaping, include
  exact ref after hash validation.

V1 không silently label top lexical hit của required document là “required
section”. Phase sau có thể add typed section refs to canonical contracts.

#### Suggestions

- exclude sections belonging to docs already hard-excluded;
- required docs may also appear in suggested sections if section helps route;
- include rank, `score_micros`, `lexical_score_micros`, section ref, heading path,
  line range, document/section hashes, summary, bounded snippet,
  authority/owner/kind, matched fields/applicability reasons;
- no hit is valid and not a packet failure.

### `knowledge`

As fixed in P2S1-D10: typed `not_installed`, owner phase 4, empty buckets, null
fingerprint.

### `source`

```json
{
  "repository_id": "repo_...",
  "kind": "git_commit",
  "commit": "0123456789abcdef0123456789abcdef01234567",
  "head_ref": "refs/heads/main",
  "worktree_root_kind": "primary_or_existing_worktree",
  "cleanliness": "clean",
  "operation_state": "normal",
  "currentness": "current"
}
```

Rules:

- `repository_id` load qua preserve/no-bootstrap manifest path;
- evidence/docs/knowledge manifests hiện có phải cùng repository ID; missing
  optional manifest được report, mismatch fails;
- detached HEAD hợp lệ: `head_ref=null`;
- full commit required;
- clean check dùng `git status --porcelain=v1 --untracked-files=all`; ignored
  files không block;
- any output entry => `work_packet_dirty_source_unsupported`;
- reject merge/rebase/cherry-pick/revert/bisect state bằng
  `work_packet_source_operation_in_progress`;
- packet source identity khác receipt descendant-currentness: packet luôn bind
  exact current `HEAD`, không dùng evidence-only descendant exception.

### `workspace`

```json
{
  "binding_status": "not_allocated",
  "workspace_id": null,
  "required_strategy": "isolated_worktree_required",
  "base_repository_id": "repo_...",
  "base_commit": "...",
  "requirements": [
    "same_repository_identity",
    "exact_base_commit",
    "clean_at_reservation",
    "scope_policy_revalidation"
  ]
}
```

### `capabilities`

```json
{
  "evaluation_status": "not_evaluated",
  "required": ["repository.inspect", "source.read", "source.write"],
  "optional": [],
  "missing": [],
  "inventory_identity": null
}
```

`missing=[]` không có nghĩa none missing; status controls interpretation.

### `scope`

```json
{
  "scope_hints": {
    "source_paths": ["src/token.mjs"],
    "documentation_paths": ["docs/product/authentication.md"],
    "configuration_paths": [],
    "data_paths": [],
    "included": [],
    "excluded": []
  },
  "implementation_freedom": [],
  "hard_stops": [
    "do_not_change_acceptance_without_authority",
    "do_not_override_accepted_decision",
    "stop_on_objective_or_invariant_ambiguity",
    "stop_on_source_or_contract_drift"
  ],
  "enforcement": {
    "status": "not_installed",
    "owner_phase": 2
  }
}
```

Anchor paths use safe repository-relative validation and sort/dedup. A symbol
anchor contributes its path once to scope hints.

### `assurance`

```json
{
  "verification_profile": "service-change",
  "expected_evidence": [],
  "expected_handoff": [],
  "documentation_impact": {},
  "qa": {
    "posture": "none",
    "status": "ready_gate_satisfied",
    "affected_case_ids": []
  },
  "promotion_policy": {
    "status": "not_installed",
    "owner_phase": 2
  },
  "close_gate": {
    "status": "not_installed",
    "owner_phase": 2
  }
}
```

Không claim verification profile command tồn tại/chạy được; đây là declared
profile requirement từ contract. Profile execution belongs later Phase 2.

### `dispatch`

```json
{
  "reservation_candidate": true,
  "dispatch_authorized": false,
  "authorization_status": "not_reserved",
  "gate_families": [
    {"family": "readiness", "status": "passed", "reason_codes": []},
    {"family": "packet_completeness", "status": "passed", "reason_codes": []},
    {"family": "source_base", "status": "passed", "reason_codes": []},
    {"family": "documentation_context", "status": "passed", "reason_codes": []},
    {"family": "qa_baseline_and_cases", "status": "not_applicable", "reason_codes": []},
    {"family": "lease", "status": "not_evaluated", "reason_codes": ["lease_resolver_not_installed"]},
    {"family": "workspace_binding", "status": "not_evaluated", "reason_codes": ["workspace_not_allocated"]},
    {"family": "capability_match", "status": "not_evaluated", "reason_codes": ["capability_inventory_not_bound"]}
  ],
  "revalidation_preconditions": []
}
```

Command success contract means every returned packet has
`reservation_candidate=true`. Pre-reservation family failure returns non-zero
packet error and no partial packet; therefore schema constrains this field to
`true` in preview profile v1. Field tồn tại để Slice 2 final assignment wrapper
có thể preserve explicit stage semantics, không phải để represent a false
preview result.

Active pre-reservation families:

- readiness;
- packet completeness/budget;
- source base;
- documentation context;
- QA dispatchability under installed Phase 1 capability.

Lease/workspace/capability families stay `not_evaluated` and therefore force
`dispatch_authorized=false`.

`revalidation_preconditions` includes stable code/value pairs for every field in
`snapshot` plus source cleanliness. Nó **không include `packet_fingerprint`** để
tránh self-reference; top-level `packet_fingerprint` là precondition riêng mà
Slice 2 compares after recomputing projection.

### `budget`

```json
{
  "profile": "phase2_work_packet_preview_budget_v1",
  "max_canonical_json_bytes": 131072,
  "max_incident_relations": 128,
  "max_decision_frontier_items": 16,
  "max_suggested_sections": 8,
  "max_snippet_bytes_each": 500,
  "recommended_initial_sections": 4,
  "max_initial_lines": 240,
  "actual_canonical_json_bytes": 42710,
  "truncations": []
}
```

128 KiB là hard ceiling cho canonical packet JSON v1. Lý do:

- current typed contract collections đã bounded;
- 8 snippets x 500 bytes giữ lexical context nhỏ;
- đủ chỗ cho 128 direct edges và typed metadata;
- thấp hơn existing 16 MiB evidence artifact ceiling nhiều bậc;
- cho test fixture một deterministic context budget.

Construction order giải self-reference:

1. Build normalized packet body with `packet_fingerprint=null` and
   `actual_canonical_json_bytes=0` in an internal builder type.
2. Build fingerprint projection, which excludes both fields and excludes any
   fingerprint precondition; canonicalize/hash it.
3. Set final `packet_fingerprint`.
4. Serialize final packet once with `actual_canonical_json_bytes=0`; let length
   be `L0`.
5. Set `actual_canonical_json_bytes=L0`, serialize again to `L1`.
6. Repeat step 5 until value equals serialized length; decimal digit growth
   converges in at most 3 passes. More than 3 => internal
   `work_packet_size_fixpoint_failed`.
7. Enforce final fixed-point length <=131072.

`actual_canonical_json_bytes` không tham gia fingerprint nhưng đúng bằng length
của final canonical bytes.

Truncation policy:

- không truncate required contract, required docs, blockers, Decisions,
  acceptance, invariants, expected proof hoặc hard stops;
- snippets đã bounded 500 bytes bởi search layer;
- suggestions đã bounded 8;
- optional docs metadata không truncate vì applicability max phải được contract
  limits/registry validation giữ hợp lý; nếu aggregate vượt ceiling thì fail;
- output >128 KiB => `work_packet_budget_exceeded` với size và dominant sections;
- `truncations` v1 luôn empty; field reserved để future explicitly safe advisory
  truncation, không dùng silent truncation.

---

## Coherent snapshot và locking algorithm

Packet build phải tránh nhiều public read calls quan sát revisions khác nhau.
Implementation flow:

```text
1. Validate repo root is an already-enrolled/bootstrapped target by checking
   existing graph manifest/schema and repository identity with preserve loaders;
   do this before any lock acquisition.
2. Acquire repository write fence exclusively. Creating only
   `.pulse/runtime/locks/workgraph.lock` is allowed operational side effect.
3. Run transaction recovery using existing store protocol.
4. Load graph canonical projection once; validate graph.
5. Load subject and verify implementation/ready shape.
6. Build current readiness snapshot/report using the same loaded graph and
   preserve/no-bootstrap docs/evidence/policy loaders.
7. Load parent/incident relation/opposite-node context from same graph projection.
8. Load current shaping/Decision/content projections already assembled for
   readiness; do not reread through unrelated public commands.
9. Load docs registry/applicability and exact required content hashes.
10. Load stable repository identity without creating manifests.
11. Capture exact Git HEAD, operation state and cleanliness while fence held.
12. Construct deterministic docs query terms.
13. Release repository fence before potentially expensive docs index refresh.
14. Refresh/search **cache-only** disposable docs index; do not write generated
    `docs/**/_index.md` projections.
15. Reacquire repository fence.
16. Recover if needed, reload all revalidation preconditions:
    - graph fingerprint, subject revisions/status, readiness fingerprint;
    - authority/docs registry fingerprints;
    - docs-search input/content fingerprint;
    - content hash of every required, optional and write-candidate document
      represented in applicability;
    - document and section hash of every selected suggestion;
    - source HEAD, cleanliness and repository operation state.
17. If graph/docs/policy/content precondition changed, discard result and return
    `work_packet_snapshot_changed`. If source HEAD, cleanliness or operation
    state changed, return `work_packet_source_changed`. Caller retries; do not
    caller retries. Do not loop internally more than once.
18. Construct normalized packet, compute canonical bytes/fingerprint/size.
19. Enforce hard budget.
20. Release fence and return packet.
```

Why two fences:

- Tantivy refresh may be expensive and uses its own cache publication lock;
- holding global repository fence across index build would unnecessarily block
  mutations;
- exact precondition revalidation closes the observation window;
- packet remains read-only over canonical state.

No canonical graph/docs/evidence/knowledge or lease/workspace/run file is
written. Allowed operational writes are repository lock file, disposable
workgraph snapshot cache, and cache-only docs search generation/lock.

Step 4 uses existing `export_unlocked/export_with_cache`; therefore
`.pulse/cache/workgraph.snapshot.json` is explicitly allowed. Packet correctness
does not depend on it: stale/corrupt/missing cache rebuilds from canonical graph.
Do not introduce a second graph projection algorithm solely to avoid this
cache.

If repository changes continuously, command fails `work_packet_snapshot_changed`
instead of starving writers or returning mixed context.

---

## Repository identity loading

Current evidence manifest `load()` may bootstrap. Slice này phải thêm preserve
loader, ví dụ:

```rust
pub fn load_existing(repo_root: &Path) -> PulseResult<Option<EvidenceManifest>>
```

Rules:

- no file => `None`, no creation;
- malformed/unknown manifest => typed error;
- packet requires existing repository identity; `None` =>
  `work_packet_repository_identity_missing`;
- if docs/knowledge manifests exist, repository IDs must match;
- missing knowledge manifest does not block packet because knowledge injection
  is not installed;
- missing docs manifest/registry always blocks with
  `work_packet_docs_registry_missing`; packet command does not bootstrap it.

Do not create a second repository ID owner. Evidence manifest remains stable
owner; source/workspace modules reference it.

---

## Source snapshot implementation contract

Extend public `pulse::source` owner rather than creating duplicate Git logic.
Proposed types:

```rust
pub struct PacketSourceSnapshot {
    pub repository_id: String,
    pub kind: SourceKind,
    pub commit: String,
    pub head_ref: Option<String>,
    pub worktree_root_kind: WorktreeRootKind,
    pub cleanliness: SourceCleanliness,
    pub operation_state: RepositoryOperationState,
    pub currentness: SourceCurrentness,
}
```

Add focused functions:

```rust
pub fn packet_base_snapshot(
    repo_root: &Path,
    repository_id: &str,
) -> PulseResult<PacketSourceSnapshot>;

pub fn revalidate_packet_base(
    repo_root: &Path,
    expected: &PacketSourceSnapshot,
) -> PulseResult<()>;
```

Do not change receipt `current_status` semantics. Packet exact-HEAD identity and
receipt ancestor/evidence-only currentness are different named operations.

Operation detection checks Git administrative paths via `git rev-parse
--git-path` rather than assuming `.git/` is a directory, so linked worktrees are
supported. Detect at minimum:

- `MERGE_HEAD`;
- `rebase-merge` or `rebase-apply`;
- `CHERRY_PICK_HEAD`;
- `REVERT_HEAD`;
- `BISECT_LOG`.

---

## Packet model ownership và source layout

New modules:

```text
src/work_packet.rs             public packet DTOs, normalization, schema/fingerprint projection
src/kernel/packet.rs           cross-domain coherent composition
src/kernel/dispatch.rs         pre-reservation gate report over packet inputs
src/source.rs                  extend exact packet source snapshot
src/docs/index.rs              extract cache-only search publication primitive
src/schema/work-packet.schema.json
```

Ownership khóa:

- `src/work_packet.rs` là public neutral value owner; export `pub mod
  work_packet` từ `src/lib.rs` và add public API compile guard;
- `src/kernel/packet.rs` owns I/O/composition and returns
  `work_packet::WorkPacketV1`;
- do not place packet types in `graph::model`, because packet composes docs,
  source and future runtime planes;
- do not place source/workspace/lease state in node schema;
- do not place packet semantics in CLI handler;
- CLI variant/handler remain thin;
- `graph::read` remains pure graph-only;
- `JsonGraphStore` entrypoint delegates to kernel composition following
  readiness/frontier pattern.

Recommended concrete API:

```rust
impl JsonGraphStore {
    pub fn work_packet(&self, id: &str) -> PulseResult<WorkPacketV1>;
}
```

`src/kernel/mod.rs` exports `packet`; CLI calls `store.work_packet(id)` and
renders.

Schema file is embedded/tested as current packet output contract, but packet is
read projection, not canonical persisted file.

---

## Stable error codes

### Subject/readiness

| Code | Meaning |
|---|---|
| `work_packet_subject_not_found` | ID missing |
| `work_packet_subject_not_ticket` | Subject kind is not Ticket |
| `work_packet_role_unsupported` | Role is not implementation |
| `work_packet_status_not_ready` | Lifecycle not `ready` |
| `work_packet_readiness_stale` | Ready lifecycle but current readiness stale |
| `work_packet_readiness_failed` | Current readiness not pass |
| `work_packet_qa_resolver_unavailable` | Required QA cannot dispatch before Phase 3 |

### Graph/content/docs

| Code | Meaning |
|---|---|
| `work_packet_graph_invalid` | Graph/reference validation failed |
| `work_packet_required_content_missing` | Required bound content missing |
| `work_packet_required_content_stale` | Bound content hash stale |
| `work_packet_docs_registry_missing` | Existing docs manifest/registry absent |
| `work_packet_docs_context_incomplete` | Required applicability gate incomplete |
| `work_packet_docs_query_empty` | Deterministic query has no terms |
| `work_packet_docs_index_unavailable` | Current lexical cache cannot be built/read |
| `work_packet_docs_score_invalid` | Search score cannot quantize to integer micros |
| `work_packet_relation_overflow` | More than 128 incident edges |
| `work_packet_decision_frontier_overflow` | More than 16 relevant decision items |

### Lower-layer to public mapping

| Lower-layer family/code | Packet top-level code |
|---|---|
| graph validate/reference/cycle error | `work_packet_graph_invalid` |
| readiness status stale or `ready_state_stale` | `work_packet_readiness_stale` |
| readiness failed/unavailable other than QA | `work_packet_readiness_failed` |
| required content missing/stale codes | matching packet content code |
| docs manifest/registry not found | `work_packet_docs_registry_missing` |
| docs applicability incomplete | `work_packet_docs_context_incomplete` |
| docs index/cache policy, corrupt, incompatible, build error | `work_packet_docs_index_unavailable` |
| docs index inputs changed during cache build | `work_packet_snapshot_changed` |
| repository lock timeout | `work_packet_lock_timeout` |
| transaction recovery ambiguous/event mismatch | `work_packet_recovery_failed` |
| malformed evidence/docs/knowledge manifest | `work_packet_manifest_invalid` |
| repository IDs disagree | `work_packet_repository_identity_mismatch` |
| canonical serializer/schema invariant | matching packet schema/fingerprint code |

JSON error rendering keeps existing global envelope with packet top-level `code`;
message must append `cause_code=<lower-code>` when mapping a lower-layer error.
Tests assert both process exit non-zero and JSON `code`; `cause_code` is a
structured optional field if existing renderer is extended, otherwise stable
message token until a global error-envelope proposal changes it.

### Source/repository

| Code | Meaning |
|---|---|
| `work_packet_repository_identity_missing` | Existing repository identity absent |
| `work_packet_repository_identity_mismatch` | Plane manifests disagree |
| `work_packet_source_unavailable` | Not a usable Git repository/HEAD |
| `work_packet_dirty_source_unsupported` | Tracked/untracked non-ignored changes exist |
| `work_packet_source_operation_in_progress` | Merge/rebase/etc. active |
| `work_packet_source_changed` | HEAD/cleanliness/operation state changed during build |
| `work_packet_snapshot_changed` | Graph/docs/policy/content precondition changed during search |
| `work_packet_lock_timeout` | Repository fence could not be acquired |
| `work_packet_recovery_failed` | Existing transaction recovery failed |
| `work_packet_manifest_invalid` | Existing plane manifest malformed/unsupported |

### Packet

| Code | Meaning |
|---|---|
| `work_packet_budget_exceeded` | Canonical JSON exceeds 128 KiB |
| `work_packet_schema_invalid` | Internal output does not validate current schema |
| `work_packet_fingerprint_failed` | Canonicalization/hash failure |
| `work_packet_size_fixpoint_failed` | Final canonical size did not converge |

No generic success packet with `reason_codes` substitutes for non-zero errors
above.

---

## Determinism rules

- canonical JSON through existing float-rejecting canonical serializer;
- packet JSON and fingerprint projection contain no floats;
- lexical scores use checked integer micro-score mapping;
- every set-like collection sort/dedup;
- parent chain ordered nearest-first then ID;
- Decisions by ID;
- branches/fog/frontier by ID;
- relations by type/from/to/ID;
- docs required/optional/write/excluded by document ID;
- suggestions by rank then section ref;
- capabilities lexical;
- reason codes lexical;
- no current timestamp, process ID, absolute root or cache generation path;
- determinism guarantee is within the pinned current Tantivy/tokenizer/dependency
  baseline on supported platforms;
- if dependency/platform ranking changes selected order or integer score, packet
  fingerprint changes truthfully;
- tests use fixture queries with non-tied stable ordering and additionally verify
  exact section-ref tie-break when quantized scores are equal;
- full canonical JSON and fingerprint are expected stable for same pinned
  implementation and same canonical/source/docs inputs.

---

## Security và authority boundaries

- Packet is context, not capability token.
- `dispatch_authorized=false` cannot be overridden by CLI flag.
- Actor possession of packet grants no mutation authority.
- Required Decision receipt proves accepted direction, not identity auth.
- Repository actor IDs remain local policy principals, not cryptographic
  signatures.
- Source path/scope hints use safe repository-relative validation and reject
  traversal/symlink escape where content is opened.
- Snippets come only from current eligible docs index; retired/migration backup
  docs are excluded.
- Packet does not inline raw prompts, secrets, environment variables or full
  logs.
- Suggested docs text remains repository content and should be treated as
  untrusted context by future prompt builder; Slice 1 does not claim prompt
  injection sanitization.

---

## Read-only và mutation boundary

Allowed operational side effects after enrollment validation:

- create/acquire `.pulse/runtime/locks/workgraph.lock` via existing repository
  fence;
- create/refresh disposable `.pulse/cache/workgraph.snapshot.json` through the
  existing coherent graph projection path;
- create/refresh cache-only docs-search generations/locks under `.pulse/cache/`.

Forbidden side effects:

- bootstrap `.pulse/workgraph`, `.pulse/evidence`, `.pulse/docs` or
  `.pulse/knowledge`;
- create runtime state other than the generic repository lock file;
- create events;
- edit graph/docs/knowledge manifests;
- acquire lease;
- create workspace/worktree/branch;
- transition status;
- write packet to canonical repository state.

Tests must assert these absences.

---

## Implementation sequence

### P2S1-I1 — Lock packet contract và schema

- Add Rust packet types and JSON Schema.
- Implement normalization and fingerprint projection.
- Add schema round-trip and unknown-field rejection tests.
- Add budget constants/profile.

### P2S1-I2 — Preserve repository identity + exact source base

- Add preserve/no-bootstrap manifest loaders.
- Extend `pulse::source` exact packet snapshot/currentness.
- Detect clean status and Git operations.
- Add primary repo/detached HEAD/existing worktree tests.

### P2S1-I3 — Coherent canonical packet snapshot

- Add kernel packet builder under repository fence.
- Reuse readiness snapshot without changing readiness contract.
- Extract parent/relation/Decision/shaping/scope/capability requirements.
- Add overflow and stable ordering tests.

### P2S1-I4 — Documentation integration

- Refactor a cache-only docs search publication primitive that never writes
  generated navigation.
- Join applicability records for write candidates.
- Implement deterministic query builder and integer micro-score mapper.
- Refresh/search disposable current cache.
- Add two-fence revalidation and snapshot-changed behavior.
- Add no-hit/exclusion/current-hash tests.

### P2S1-I5 — Dispatch preparation projection

- Implement active pre-reservation gate families.
- Emit runtime future gates as typed not-evaluated/not-installed.
- Guarantee `dispatch_authorized=false`.
- Add `reservation_candidate` and revalidation preconditions.

### P2S1-I6 — CLI và contract tests

- Add `WorkCommand::Packet`.
- Thin handler + human/JSON render.
- Stable error/exit tests.
- Target-repo fixture integration.

### P2S1-I7 — Concurrency/currentness/budget hardening

- Concurrent mutation during docs search returns snapshot-changed.
- No mixed graph/docs/source packet.
- 128 KiB/edge/frontier overflow tests.
- Cross-platform source status tests where supported.

### P2S1-I8 — Owner-doc and completion update

After implementation/verification only:

- mark proposal implemented with commit/test count;
- update roadmap Phase 2 status without claiming full Phase 2 complete;
- update root next-step summary if still stale;
- preserve Slice 2 handoff below.

---

## Test layout

Use existing domain integration convention.

Suggested files:

```text
tests/graph.rs
  #[path = "graph/work_packet.rs"]
  #[path = "graph/work_packet_cli_contract.rs"]

 tests/graph/work_packet.rs
 tests/graph/work_packet_cli_contract.rs

 tests/process.rs
  #[path = "process/work_packet_snapshot_concurrency.rs"]

 tests/process/work_packet_snapshot_concurrency.rs

 tests/target_repo.rs
  #[path = "target_repo/work_packet_target_repo.rs"]

 tests/target_repo/work_packet_target_repo.rs
```

Extend `tests/common/fixture_repo.rs` for Git operation/worktree helpers instead
of reimplementing unsafe copy logic.

Never run Pulse against this development repository or tracked fixture in place.
Every integration scenario uses `TestRepo::from_fixture` or an external
`TempDir` initialized with deterministic Git baseline.

---

## Acceptance matrix

### A. Happy path

1. Bootstrapped temp copy with ready implementation Ticket produces schema v1.
2. Output has exact subject revision/contract revision/readiness fingerprint.
3. Output binds full clean Git HEAD and repository ID.
4. Required docs/applicable docs and top lexical sections are routed.
5. Packet stays under 128 KiB.
6. `reservation_candidate=true`.
7. `dispatch_authorized=false`.
8. lease/workspace/capability remain typed not evaluated.

### B. Subject/readiness

1. Missing ID -> `work_packet_subject_not_found`.
2. Story/Epic/Decision -> subject-not-ticket.
3. Decision-work Ticket -> role unsupported.
4. Draft/shaped/blocked -> status-not-ready.
5. Ready node with changed contract/docs/shaping/content -> readiness stale.
6. Hard dependency newly blocks -> readiness stale/fail; no packet.
7. Soft preference does not block packet and appears as context.
8. Superseded/terminal Ticket rejected.

### C. Docs

1. Required document included with ID/path/revision/hash/owner/authority/reasons.
2. Optional docs included separately.
3. Write candidate joined to full current metadata.
4. Retired/stale/migration/superseded doc visible in excluded, not current.
5. Missing required doc -> docs context incomplete.
6. Suggested search no hit -> success, empty suggestions.
7. Suggested results max 8, snippet each <=500 bytes.
8. Read budget is 4 sections/240 lines.
9. Cache missing builds disposable search cache only and does not write
   `docs/**/_index.md`.
10. Cache corrupt rebuilds or returns packet-level stable error; no canonical
    docs change.
11. Explicit `docs index` still writes/validates generated navigation after
    internal refactor; packet cache-only path does not.
12. Required document is not silently mapped to arbitrary top section.
13. Missing docs manifest/registry rejects even for documentation posture `none`.
14. Search scores serialize only as non-negative integer micros.

### D. Source

1. Clean full HEAD succeeds.
2. Dirty tracked file rejects.
3. Untracked non-ignored file rejects.
4. Ignored cache/runtime file does not dirty source.
5. Detached HEAD succeeds with `head_ref=null`.
6. Merge/rebase/cherry-pick/revert/bisect rejects.
7. Missing repository ID rejects without bootstrap.
8. Manifest repository ID mismatch rejects.
9. HEAD changes during packet build rejects snapshot/source changed.

### E. Packet coherence

1. Graph mutation during docs index/search never returns mixed revisions.
2. Docs registry/content mutation during search causes revalidation failure.
3. Authority policy change during search causes revalidation failure.
4. Source becomes dirty during search causes revalidation failure.
5. Retry after stable state succeeds.
6. Packet query creates no event/lease/workspace/run state.
7. Packet query may create only repository lock file, disposable workgraph
   snapshot cache and docs search cache files after enrollment validation.
8. Packet query does not change node status/revision.
9. Non-enrolled path rejects before creating `.pulse/runtime`.

### F. Determinism/budget

1. Same inputs produce same packet fingerprint.
2. Cache generation rebuild with same content produces same packet fingerprint.
3. Different required section hash changes fingerprint.
4. Optional selected suggestion identity/hash change changes fingerprint.
5. Float search scores quantize deterministically to integer micros; no float
   reaches canonical packet JSON.
6. Same selected sections with different rank/quantized score produce a changed
   fingerprint; exact tie uses section-ref ordering.
7. Fingerprint projection has no self-reference.
8. `actual_canonical_json_bytes` reaches fixed point and equals final bytes.
9. >128 edges rejects overflow.
10. >16 relevant decision-frontier items rejects overflow.
11. >128 KiB canonical packet rejects budget; no required context truncation.
12. Ordering stable despite shuffled input files/registry records.

### G. Safety/architecture

1. Graph pure read layer gains no docs/source/runtime imports.
2. CLI handler owns no packet business logic.
3. Node/schema gains no lease/workspace/packet fields.
4. `pulse::source` public path remains valid.
5. Packet query on non-enrolled fixture path does not bootstrap.
6. Tracked fixture remains immutable and free of `.pulse`/`.git`.

---

## Roadmap scenarios được slice này sở hữu

Slice này hoàn thành foundation subset của:

- **#8:** work packet trả Ticket, parents, Decisions, docs, edges và gates mà
  Agent không scan raw graph/docs; source/workspace final binding remains Slice 2.
- **#20:** route current applicable docs, exclude retired/migration docs.
- **#27:** section refs/ranges/hashes/snippets bounded.
- **#28:** docs cache rebuild giữ expected semantic routing/fingerprint.
- **#30:** required/suggested refs + read budget, không inline full docs.
- **#31:** corrupt/stale cache discard/rebuild.
- **#32:** retrieval tokenizer/eval foundation reused; Slice này thêm deterministic
  packet query fixture.
- **#40:** execution frontier identity remains unchanged; claim remains not
  evaluated.

Slice này không claim full scenario #8 vì final workspace/lease-bound assignment
packet belongs Slice 2, và không claim run/close scenarios.

---

## Definition of Done

- [x] Public `pulse::work_packet::WorkPacketV1` schema/types, explicit packet
      contract DTO, deny-unknown round-trip và non-self-referential canonical
      fingerprint implemented.
- [x] `pulse work packet <ticket-id> --json` stable command implemented.
- [x] Only current ready implementation Ticket can produce packet.
- [x] Packet contains typed contract, parent, Decision, shaping, relation,
      blocker, docs, source, assurance and runtime-requirement context.
- [x] Existing docs manifest/registry is required; packet contains current
      required/optional/write/excluded docs and max 8 lexical section suggestions.
- [x] Deterministic docs query algorithm implemented exactly as specified.
- [x] Packet binds existing stable repository ID and exact clean Git HEAD.
- [x] Dirty source and Git operation in progress reject.
- [x] Workspace requirement mapping implemented; no workspace ID fabricated.
- [x] Capability requirements derive exactly from D7 vocabulary/rules.
- [x] Knowledge/promotion/close/runtime families report typed not installed or
      not evaluated, not fabricated empty/pass.
- [x] Every successful preview packet has schema-constant
      `reservation_candidate=true`; failed pre-reservation gates return stable
      non-zero packet errors.
- [x] `dispatch_authorized` always false in this profile.
- [x] Packet builder observes coherent canonical/source state and revalidates
      after cache-only docs search.
- [x] Packet JSON contains no floats; search scores use integer micros.
- [x] 128 KiB hard budget, size fixed point and overflow rules enforced without
      required-context truncation.
- [x] Read query writes only generic repository lock, disposable workgraph
      snapshot cache and disposable docs search cache.
- [x] No node/edge/manifest/event/lease/workspace/run mutation from packet query.
- [x] Focused schema/unit/integration/concurrency/source tests pass.
- [x] `cargo fmt --check` passes.
- [x] `cargo clippy --all-targets --quiet -- -D warnings` passes.
- [x] `cargo test --all-targets` passes under default threading.
- [x] `git diff --check` passes.
- [x] Proposal and roadmap completion status updated only after verified commit.

### Completion evidence

Verified implementation commits:

- `ed68c08` — packet contract/schema/types, normalization, canonical
  fingerprinting and schema tests.
- `5478234` — schema verification hardening for the packet contract.
- `823607e` — preserve/no-bootstrap repository identity and exact source-base
  snapshot support.
- `067533a` — source identity edge-case verification.
- `e47f90d` — coherent packet snapshot kernel builder and context projection.
- `7e89033` — packet snapshot verification hardening.
- `cf65ec4` — documentation applicability/search integration, deterministic
  docs query and cache-only search path.
- `a6d3125` — documentation packet integration hardening.
- `d236774` — pre-reservation dispatch preparation projection.
- `c306a22` — dispatch preview projection hardening.
- `7f62545` — CLI and contract/integration tests.
- `e28c5c0` — read-only side-effect contract fix.
- `ddef30a` — coherence, concurrency and budget hardening.
- `6d3076b` — final P2S1-I7 verification of side effects, fingerprint and
  deterministic hardening invariants.

Final P2S1-I8 verification evidence before marking complete:

- `cargo fmt --check` — pass.
- `cargo clippy --all-targets --quiet -- -D warnings` — pass.
- `cargo test --all-targets` under default threading — pass: 458 tests across
  library, binary and integration crates; the docs retrieval bench harness
  executable also passed.
- `git diff --check` — pass.

This completes only the Phase 2 Slice 1 preview `WorkPacketV1` foundation. It
still does not acquire leases, allocate workspaces, emit `PreparedAssignmentV1`,
open `ready -> active`, run agents, perform handoff/verification, or close work;
those remain the Slice 2+ responsibilities below.

---

## Failure/recovery behavior

Packet command itself persists no canonical transaction. Recovery concerns:

- at command start, run existing graph transaction recovery before snapshot;
- docs cache index uses existing atomic generation publication/recovery;
- interruption before cache publication leaves last complete generation usable;
- interruption after cache publication but before packet return leaves only
  disposable cache, safe to retry;
- no lease/workspace cleanup needed because this Slice creates none;
- source/graph/docs drift between phases causes typed snapshot-changed, never a
  partially current packet.

---

## Performance target

Reference target on `minimal-service` and generated fixtures:

- packet without index rebuild: p95 < 250 ms at 1,000 registered docs and 5,000
  graph nodes on supported developer hardware;
- packet with required incremental docs refresh: p95 < 2 s at 1,000 docs;
- peak packet JSON <=128 KiB by contract;
- no full docs corpus loaded into packet output;
- no persistent full graph cache required for correctness.

These are acceptance benchmark targets, not compatibility SLA. Add ignored
benchmark/smoke fixture if normal integration timing would be flaky; correctness
tests must not assert wall-clock thresholds.

---

## Risks và mitigations

### Packet thành một graph/docs dump

Mitigation: direct relations only, bounded parent chain/frontier/suggestions,
refs/hashes over full prose, 128 KiB hard ceiling.

### Preview packet bị caller hiểu nhầm là assignment

Mitigation: explicit profile name, `dispatch_authorized=false`,
`authorization_status=not_reserved`, no lease/workspace ID, non-overridable.

### TOCTOU giữa preview và reservation

Mitigation: packet carries complete preconditions; Slice 2 must atomically
revalidate under fence before lease. Preview is never durable authorization.

### Docs index refresh tạo mixed snapshot

Mitigation: two-fence algorithm and exact revalidation; no unbounded retry.

### Source identity quá strict cho low-risk work

Mitigation: strict clean commit now; dirty canonicalization remains explicit
future Decision. Correctness over convenience.

### Anchor paths bị hiểu nhầm là permissions

Mitigation: name `scope_hints`, enforcement typed not installed; do not expose
`allowed_paths`.

### Capability vocabulary khóa v2 quá sớm

Mitigation: small v1 requirement vocabulary, no Agent Registry/inventory schema;
future inventory maps to stable requirement strings.

### Packet fingerprint thay đổi vì Tantivy float/cache generation

Mitigation: packet quantizes scores to integer micros and fingerprints selected
section identities/hashes/ranks/integer scores, never cache generation ID or
floating number.

### Required docs chưa có section refs

Mitigation: truthful `document_level_only`; do not invent required section from
ranking. Future canonical section refs can refine without lying now.

---

## Handoff bắt buộc cho Phase 2 Slice 2

Slice 2 phải triển khai **atomic reservation + workspace binding**, sử dụng
packet preconditions này. Minimum follow-up contract:

1. `pulse work claim <ticket-id>` hoặc một explicit `work prepare` mutation;
2. runtime assignment lease with ID, assignee/run principal, Ticket revision,
   readiness fingerprint, packet fingerprint, TTL and exclusive state;
3. workspace record with ID, mode, canonical path, repository ID, base commit,
   clean/current state and owning lease;
4. concrete capability inventory identity and full required-capability match;
5. revalidate graph/readiness/docs/source/policy under repository fence before
   committing lease;
6. materialize isolated worktree when required, with rollback on failure;
7. final `PreparedAssignmentV1` wrapper, owned by a new neutral runtime value
   module, containing:
   - `schema_version: 1`;
   - exact preview `packet_fingerprint` and revalidated snapshot;
   - lease ID/state/assignee principal;
   - allocated workspace identity/binding;
   - concrete capability inventory/match report;
   - `dispatch_authorized=true` only after all gates pass;
   - reference to WorkPacketV1 context rather than mutating preview semantics;
8. gated `ready -> active`, no `--force`;
9. claim/release concurrency, TTL/ghost lease recovery and process crash tests;
10. frontier claim-state composition from runtime without persisting claim into
    graph.

Slice 2 proposal must lock transaction ordering between runtime lease file, Git
worktree creation, event and lifecycle transition before implementation. Slice
2 không được mutate preview `WorkPacketV1` thành post-lease shape, weaken packet
preconditions hoặc coi preview packet như bearer authorization.

---

## Câu hỏi không còn mở trong slice này

Proposal này đã khóa:

- packet là preview hay post-lease: **preview**;
- lease/workspace mutation có thuộc Slice 1: **không**;
- `dispatch_authorized`: **luôn false**;
- supported source: **exact clean Git HEAD only**;
- dirty hash: **không hỗ trợ**;
- workspace ID: **không fabricate; Slice 2 sở hữu**;
- risk/workspace mapping: **low may in-place, medium+ isolated required**;
- capability handling: **requirements only, concrete match not evaluated**;
- writable paths: **scope hints only, no enforced ACL claim**;
- docs cache refresh: **cache-only, không write generated navigation, rồi
  revalidate**;
- docs query: **deterministic typed-fragment algorithm**;
- knowledge: **typed not installed**;
- packet score representation: **integer micros, không float**;
- packet budget: **128 KiB hard ceiling with fixed-point actual size**;
- required context overflow: **fail, never silently truncate**;
- fingerprint: **separate aggregate packet fingerprint**;
- schema strategy: **single current v1 baseline pre-release**;
- final Slice 2 artifact family: **`PreparedAssignmentV1` wrapper, không mutate
  preview packet**.

## Những gì cố ý để Slice 2/3/4 quyết định

- lease file schema/path/state machine/TTL;
- assignee identity before full Agent Registry;
- workspace ID generation and runtime record lifecycle;
- worktree branch/ref naming and cleanup/adoption;
- exact transaction protocol spanning lease/worktree/event/status (artifact
  family đã khóa là `PreparedAssignmentV1`, transaction ordering chưa thuộc
  Slice 1);
- concrete capability inventory source;
- enforceable writable path policy;
- prompt builder rendering/token budget;
- dirty source canonicalization;
- verification/handoff/close receipt payloads;
- Story QA baseline/case schema;
- applicable knowledge retrieval/injection;
- peer-agent transport/mailbox/ack.

Những mục này không cần đoán để implement Slice 1 vì output đã biểu diễn chúng
rõ là requirement hoặc not-installed state, còn handoff đã khóa artifact boundary
để Slice 2 viết proposal transaction riêng trước implementation.
