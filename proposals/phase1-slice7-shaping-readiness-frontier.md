# Phase 1 — Slice 7: Shaping Contract, Readiness Projection + Decision/Execution Frontier

> Trạng thái: **proposal để review**, chưa phải work contract hay compatibility
> contract.
> Tiền đề:
> [`phase1-slice6-knowledge-store-foundation.md`](phase1-slice6-knowledge-store-foundation.md)
> đã được implement và verify.
> Sở hữu: implementation strategy cho lát cắt Phase 1 cuối cùng: typed
> implementation/shaping contract, immutable shaping result, readiness
> composition, lifecycle gate `shaped`/`ready`, shaping-map binding và hai
> projection decision/execution frontier.
> Tham chiếu normative:
> [`PULSE_REBOOT.md`](../PULSE_REBOOT.md),
> [`02-work-graph.md`](../pulse-reboot/02-work-graph.md),
> [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md),
> [`06-priority-reconciliation.md`](../pulse-reboot/06-priority-reconciliation.md),
> [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md),
> [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md),
> [`10-documentation-system.md`](../pulse-reboot/10-documentation-system.md) và
> [`11-documentation-retrieval.md`](../pulse-reboot/11-documentation-retrieval.md).

## Trạng thái đã verify trước proposal này

Repository hiện đã implement Slice 1–6:

- sharded work graph, lifecycle, supersession, structural executability,
  traversal, roll-up, CAS, crash recovery và immutable events;
- immutable evidence receipts, source/content/work-revision bindings và
  content-addressed artifacts;
- canonical docs registry, documentation impact và document-level
  applicability;
- section extraction, generated `_index.md`, Tantivy lexical retrieval,
  bounded `search|get|tree`, cache generations và retrieval evals;
- canonical knowledge entry/relation store, revision CAS, provenance,
  applicability, promotion/freshness/trust schema và disposable projection.

Tại thời điểm viết proposal:

```text
cargo fmt --check                                      pass
cargo clippy --all-targets --quiet -- -D warnings     pass
cargo test --all-targets                               pass
cargo bench --bench docs_retrieval -- --smoke         pass
```

Sau single-baseline cleanup, current node schema/Rust implementation đã có:

- `contract_revision` tách khỏi normal CAS `revision`;
- Ticket role `implementation|decision_work`;
- assessed risk/materialization trên public create và explicit `unassessed` cho
  canonical draft/bootstrap state;
- typed implementation/decision-work contracts, minimal QA-impact metadata và
  current shaping pointer/map reference;
- current node `schema_version: 1`, không có predecessor model hoặc migration
  path cho internal Slice state.

CLI hiện có `work transition`, `work executability`, `docs applicable` và
`evidence receipt verify`, nhưng chưa có contract/QA setters, shaping
apply/show/invalidate, readiness-policy query, `work ready`, hoặc decision/
execution frontier commands. Lifecycle source cũng chưa mở typed gated path
`draft -> shaped -> ready` theo owner contract.

`shaping_validation` receipt hiện chỉ có skeletal payload v1 với
`owning_work`, `risk`, một `destination` string, summary arrays và
`approval_assertion`; JSON Schema trên disk chỉ enforce `payload_version`.
Slice 7 hoàn thiện chính current payload v1 này. Internal placeholder bytes are
not a historical compatibility family.

Vì vậy Slice 7 là lát cắt đúng tiếp theo và là foundation còn thiếu để đóng
Phase 1 theo roadmap. Nó không phải Phase 2 runner và không phải full
conversational `pulse-shape` capability.

## Vị trí của slice trong Pulse Reboot

Slice 2 chỉ trả lời câu hỏi hẹp:

> Graph/lifecycle hiện tại có cho Ticket tiếp tục về mặt cơ học hay không?

Slice 4–5 trả lời:

> Durable docs nào current/applicable và section nào có thể retrieve bounded?

Slice 3 đã tạo identity cho shaping receipt, nhưng chưa đủ schema để gate.
Slice 7 compose các foundation đó thành câu hỏi:

> Với exact work/docs/Decision/shaping inputs hiện tại, Ticket có một executable
> contract đủ rõ để trở thành current `ready` candidate hay chưa?

Pipeline của slice:

```text
Ticket/Story/Epic work revisions
  + implementation hoặc decision-work contract
  + structural executability
  + current shaping receipt + optional shaping map
  + branch dispositions + bounded fog
  + required Decisions/content references
  + documentation impact + applicable docs
  + local authority policy
  -> deterministic readiness report
  -> guarded shaped/ready transition
  -> decision frontier / execution frontier projection
```

Slice 7 hoàn thiện **kernel contract và projection**. Judgment vẫn thuộc
Agent/human:

- kernel không tự phát hiện mọi ambiguity;
- kernel không tự chọn direction;
- kernel không tự graduate fog;
- kernel không tự viết Decision hoặc Ticket;
- kernel không tự đánh giá recommendation semantic có tốt hay không.

`pulse-shape` ở Phase 2/3 sẽ tạo/review inputs bằng repo-grounded,
one-question-at-a-time grilling. Slice 7 chỉ cho capability đó một contract
local, typed, auditable và recoverable để publish kết quả.

## Các quyết định khóa cho proposal

### S7-D1 — Tách structural executability khỏi readiness

`src/graph/executability.rs` tiếp tục chỉ sở hữu lifecycle, hard dependency,
soft preference và supersession. Không nhét docs, receipt, branch hoặc authority
vào structural executability.

Readiness là module riêng compose nhiều gate families. Điều này giữ output
explainable và tránh một boolean `is_ready` khó audit.

### S7-D2 — Current shaping receipt payload v1 là typed immutable result; map là index

- Canonical answers vẫn sống ở Decision, Ticket/Story contract, research hoặc
  prototype evidence tương ứng.
- Current `shaping_validation` payload v1 được hoàn thiện thành immutable
  observation/assertion bind exact revisions và content bytes.
- Optional `shaping.md`/`approach.md` map là human-facing low-resolution index,
  bind bằng path/revision/content hash.
- Node chỉ giữ pointer tới **current accepted shaping receipt** và optional map;
  không embed writable branch/frontier list.
- Không parse free-form Markdown để reconstruct readiness semantics.

### S7-D3 — Receipt-first apply thay vì transaction receipt + node cùng lúc

Workflow:

```text
1. pulse evidence receipt record --file shaping-receipt.json
2. pulse work shaping apply <owner-id> --receipt <receipt-id> --expected-revision <n>
```

Receipt record là immutable operation đã có crash recovery. `shaping apply` chỉ
validate receipt rồi CAS-update owner node pointer + event. Crash sau bước 1 tạo
orphan historical receipt an toàn; không tạo half-valid current shaping pointer.
Cách này reuse receipt-first boundary của supersession và tránh một transaction
cross-plane lớn không cần thiết.

### S7-D4 — Readiness là projection hiện tại, lifecycle `ready` có thể stale

Node status ghi lifecycle transition đã xảy ra. Current readiness luôn được
recompute từ exact inputs. Nếu docs, Decision, map, receipt, blocker hoặc node
revision liên quan đổi:

- node có thể vẫn mang status `ready` để giữ audit history;
- `pulse work ready` trả `stale|not_ready`;
- execution frontier exclude node đó;
- không có hidden background mutation tự hạ status;
- caller phải re-shape/re-apply receipt hoặc transition có audit phù hợp.

### S7-D5 — Dùng narrow readiness fingerprint

Global graph fingerprint được report làm audit context nhưng không là freshness
key duy nhất. Unrelated node mutation không được làm receipt/readiness stale.
Readiness fingerprint chỉ hash exact inputs liên quan đến subject.

### S7-D6 — Frontier là membership projection, không phải priority ranking

- Decision frontier và execution frontier dùng cùng graph snapshot nhưng khác
  eligibility semantics.
- Priority/foundation/cost-of-delay thuộc semantic reconciliation trong
  `06-priority-reconciliation.md`, không được kernel biến thành một score ngầm.
- Initial ordering deterministic theo ID và reason class.
- Claim/lease không persist vào graph. Slice 7 chưa có runtime resolver nên
  output phải ghi `claim_state=not_evaluated`, không được giả `unclaimed=true`.

### S7-D7 — Authority là local policy, không suy ra từ `actor.kind`

Receipt actor hoặc CLI `--actor` chỉ là declared identity, không tự tạo quyền.
Slice 7 cần một narrow machine-readable resolver để thật sự mở transition có
approval gate.

Proposal dùng tracked local policy:

```text
.pulse/policy/authority.json
```

`PULSE.md` vẫn sở hữu human-readable intent và authority boundaries;
`authority.json` chỉ là enforceable principal-to-grant mapping, tương tự docs
registry giữ machine routing metadata mà không thay durable prose.

Slice 7 chỉ load/validate file này. Public grant/revoke UX defer `pulse init` và
policy capability Phase 3; bootstrap fixture hoặc human maintainer tạo policy
explicitly. Không có permissive default.

**Đã khóa:** canonical enforceable authority registry của Slice 7 nằm tại
`.pulse/policy/authority.json`. `PULSE.md` tiếp tục sở hữu human-readable intent;
`.pulse/config.yaml` không chứa principal/grant truth.

### S7-D8 — Ready profile có minimal QA-impact gate, không giả full QA

Owner ready gate yêu cầu QA impact không còn `unknown`, trong khi Story baseline,
case selection, executor và QA receipts thuộc Phase 3. Slice 7 không được bỏ qua
requirement này chỉ bằng cách đổi tên profile.

Proposal kéo **minimal readiness-only QA impact declaration** vào node schema:

```text
unknown
required
covered_by_story_close
none
```

- `unknown` luôn chặn ready;
- `none` chỉ pass với rationale cho internal/non-behavior work và approver có
  grant `qa.none.approve`;
- `covered_by_story_close` cần behavioral owner Story + rationale giải thích vì
  sao targeted checkpoint chưa economic/runnable, và approver có grant
  `qa.defer_to_story_close`; full Story qualification vẫn là future close gate;
- `required` cần Story baseline/case references. Cho tới Phase 3 resolver tồn
  tại, gate status là `unavailable` và Ticket không transition `ready`.

Vì vậy Slice 7 có thể mở ready trung thực cho internal/no-checkpoint work mà
không dispatch behavior-affecting work thiếu QA contract. Initial profile:

```text
phase1_contract_readiness_v1
```

Output luôn giữ:

```json
{
  "dispatch_authorized": false,
  "future_gate_families": [
    "qa_baseline_and_cases",
    "lease",
    "source_workspace"
  ]
}
```

Phase 3 mở rộng cùng QA-impact metadata bằng baseline/case resolver và bump
profile khi cần. `ready` không bao giờ có nghĩa “QA unknown nhưng sẽ xử lý sau”.

## Mục tiêu

Triển khai shaping/readiness foundation để có thể:

- hoàn thiện current node schema v1 với typed work role, risk/materialization,
  implementation contract và shaping pointer;
- phân biệt implementation Ticket với decision-work Ticket mà không thêm node
  kind hoặc hierarchy thứ hai;
- hoàn thiện current `shaping_validation` payload v1 với contract revision bindings, destination, exit
  condition, critical branches, dispositions, bounded fog, out-of-scope,
  canonical resolution pointers, approvals và reconciliation provenance;
- record minimal immutable Decision acceptance proof cho hard-to-reverse
  references;
- store minimal QA-impact posture so `unknown` cannot pass ready;
- update development receipts/fixtures to the completed current payload v1; older
  internal placeholder bytes are regenerated or rejected as drift, not migrated;
- apply một current shaping receipt vào owning work bằng expected-revision CAS;
- validate map path/revision/content hash và stale bindings;
- query readiness với gate-family statuses/reason codes đầy đủ;
- mở `draft -> shaped` và `shaped -> ready` bằng fresh gate evaluation,
  không có `--force`; blocked work đi `blocked -> shaped -> ready` để giữ
  structural executability contract hiện tại;
- invalidate current readiness khi relevant work/docs/Decision/content/policy
  input đổi, không over-invalidate mutation không liên quan;
- derive decision frontier cho typed decision work phục vụ một destination;
- derive execution frontier cho implementation Tickets status `ready` và
  current readiness pass;
- rebuild projection/cache mà không đổi semantics;
- cung cấp stable extension points cho minimal `pulse-shape`, work packet,
  prompt builder, lease và reconciliation ở Phase 2.

## Acceptance scope

### Roadmap scenarios được slice sở hữu

- **#9:** implementation Ticket thiếu code anchors/invariants/mode không ready;
  typed decision/discovery work có contract riêng.
- **#10:** R0 rõ/low-risk không bị ép plan/design/map/human interview.
- **#19:** documentation impact `unknown` chặn ready; internal refactor có thể
  dùng `none` + rationale.
- **#34:** critical branch chưa disposition chặn ready; invalid delegation hoặc
  deferral bị reject.
- **#35:** R0 concise contract + ambiguity self-check đủ khi policy cho phép.
- **#37:** multi-session shaping khóa destination/exit condition, map chỉ giữ
  gist/link và decision frontier derive từ graph.
- **#38, storage/projection subset:** bounded fog được giữ typed; precise
  decision work có provenance. Semantic graduation defer Phase 2.
- **#39, mutation/projection subset:** shaping receipt replacement/reconciliation
  cập nhật pointers và affected readiness; semantic invalidation/graduation do
  capability quyết định.
- **#40:** decision/execution frontiers khác nhau, deterministic và không persist
  claim/lease.

### Scenario chỉ chuẩn bị boundary

- **#33:** Slice 7 cung cấp output contract cho `pulse-shape`; conversational
  grounding/question flow defer Phase 2/3.
- **#36:** Slice 7 encode implementation freedom và blocking ambiguity;
  Worker `decision_request` defer Phase 2.
- **#42, minimal posture subset:** QA impact `unknown` chặn ready;
  `none|covered_by_story_close` cần rationale/owner phù hợp; required baseline,
  case selection và execution receipts defer Phase 3.

### Decisions liên quan

- D-02 đến D-07.
- D-17 đến D-25.
- D-26 đến D-29.
- D-35 đến D-40 ở docs section/read-budget input boundary.
- D-43/D-44 trực tiếp cho shaping, destination, frontier và fog.
- D-45 đến D-51 chỉ ở explicit future QA boundary.
- D-61 ở future work-packet aggregation boundary; Slice 7 không merge docs và
  knowledge thành untyped context.

### Slice exit

Slice hoàn thành khi Phase 1 có typed, deterministic và recoverable path từ
work contract + shaping evidence tới readiness/frontier projections.

Slice exit **không** đồng nghĩa:

- `pulse-shape` conversational capability đã hoàn chỉnh;
- work packet/prompt builder đã tồn tại;
- Worker đã được dispatch;
- lease/source workspace đã được acquire;
- full QA baseline/case execution đã được resolve;
- Ticket có thể transition `active|verifying|done`;
- Phase 2 hoặc Core v1 đã hoàn thành.

## Non-goals

- Tự động đọc prose và suy ra implementation contract bằng heuristic/LLM.
- Interactive one-question-at-a-time interview.
- Tự tạo recommendation, Decision hoặc discovery Ticket.
- Tự quyết định ambiguity nào critical.
- Tự graduate fog hoặc cancel/supersede invalidated branch.
- Full `pulse work packet` và prompt rendering.
- Source writable scope, workspace identity, lease hoặc Agent Registry.
- Runner, cancellation, resume, handoff, verification hoặc close gate.
- Story QA baseline, QA case parsing/selection, executor hoặc QA receipt.
  Slice 7 chỉ thêm minimal readiness-only QA impact posture mutation.
- Knowledge search/applicable injection.
- Priority scoring hoặc automatic dispatch ordering.
- Generic policy engine, cryptographic identity/signature hoặc remote
  authorization.
- Dirty-worktree snapshot hashing.
- Editing `ticket.md`, `approach.md` hoặc `shaping.md` through kernel.
- Persisting decision/execution frontier lists hoặc runtime claim state.
- Generic arbitrary JSON patch cho contract/shaping fields.

## Repository layout

```text
PULSE.md

.pulse/
  policy/
    authority.json                  # tracked enforceable local grants

  workgraph/
    schemas/
      node.schema.json              # current schema v1 amended in place
    nodes/
      ST-014.json                   # current shaping pointer
      TK-031.json                   # implementation contract
      TK-032.json                   # decision-work contract

  evidence/
    schemas/
      shaping-validation.v1.schema.json
      decision-acceptance.v1.schema.json
    receipts/
      rcpt_01J....json

  events/
    2026-07-25/
      evt_01J....json

  cache/
    workgraph.snapshot.json
    readiness.snapshot.json         # optional disposable projection

  runtime/
    locks/
      workgraph.lock
    transactions/
      txn_01J....json

works/
  ST-014/
    approach.md
    shaping.md                      # optional persisted map/index
  TK-031/
    ticket.md                       # human-facing implementation contract
```

Ownership:

- node JSON giữ concise machine-readable contract và current receipt pointer;
- work Markdown giữ human-facing outcome, rationale và detailed prose;
- receipt giữ immutable reviewed snapshot/provenance;
- Decision/work artifacts giữ canonical answers; Decision acceptance receipt
  giữ immutable approval proof;
- authority registry giữ machine grants, không thay `PULSE.md` intent;
- readiness/frontier cache là disposable;
- map không phải canonical decision database hoặc writable frontier store.

## Current node schema v1 completion

### Baseline rule

Current repository node schema remains `schema_version: 1`. Slice 7 amends that
current pre-release baseline in place across `node.schema.json`, Rust types,
tests and fixtures. It does not add a predecessor model, schema-upgrade event,
migrate-on-load path or migration command for internal Slice state.

Current development repositories/fixtures are regenerated or updated to the
completed v1 shape. Unknown or manually drifted repository schemas remain
default-deny and are rejected rather than guessed or silently rewritten. Public
Ticket creation requires explicit assessed role/risk/materialization; canonical
draft/bootstrap state may retain the explicit `unassessed` domain value without
fabricating an implementation contract, shaping receipt or Markdown content.

### Semantic contract revision

The current node v1 baseline includes:

```jsonc
{
  "revision": 9,
  "contract_revision": 4
}
```

`revision` remains CAS revision for every node mutation. `contract_revision`
tracks only semantic inputs that shaping/readiness reviews:

- implementation/decision-work contract;
- risk/materialization/work surface/plan policy;
- documentation impact/routing;
- minimal QA impact;
- required Decision/shared approach references;
- shaping destination-owner contract when stored on the node.

Mutations that **do not** bump `contract_revision`:

- applying/invalidating current shaping pointer;
- lifecycle-only transition/status reason;
- timestamps/audit-only metadata.

Rules:

- contract mutation bumps both revisions;
- pointer/status mutation bumps only normal revision;
- shaping receipt payload v1 binds `contract_revision` as freshness boundary and records
  normal node revision only as observed audit context;
- generic evidence `bindings.work` may become historical after apply/transition,
  but shaping-specific currentness remains valid while exact contract revision
  and bound content hashes remain current;
- this prevents shaping apply and successful ready transition from invalidating
  their own proof.

### Common risk/materialization metadata

The current Ticket baseline already includes:

```jsonc
{
  "risk": "unassessed|low|medium|high|critical",
  "materialization": "unassessed|R0|R1|R2|R3"
}
```

Rules:

- both are already required on normal public Ticket create and cannot be
  `unassessed` there;
- canonical draft/bootstrap Tickets may carry explicit `risk=unassessed` and
  `materialization=unassessed`; kernel does not invent semantic risk;
- any `unassessed` value blocks shaped/ready and creates a review finding;
- first explicit classification from `unassessed` is not a downgrade and does
  not require downgrade authority;
- risk and materialization are related but separate: risk describes exposure;
  materialization describes required shaping/artifact depth;
- later downgrade from assessed value is typed mutation with reason/authority
  event; initial Slice 7 contract setter may keep/raise freely but downgrade
  requires `work.materialization.downgrade` grant.

### Minimal QA-impact metadata

The current Ticket baseline also includes:

```jsonc
{
  "qa": {
    "impact": {
      "posture": "unknown|required|covered_by_story_close|none",
      "rationale": null,
      "behavioral_owner": null,
      "affected_case_ids": []
    }
  }
}
```

- canonical draft/bootstrap or missing metadata uses `unknown` without inventing rationale;
- mutation uses Ticket expected revision, bumps contract revision and emits
  `work.qa_impact.updated`;
- structural rules are defined in the readiness QA gate below;
- baseline/case existence resolver remains Phase 3.

### Ticket role

```jsonc
{
  "role": "implementation|decision_work"
}
```

Only Ticket may carry `role`.

Existing node kinds remain exactly:

```text
epic, story, ticket, decision
```

Không thêm `Discovery`, `Spike` hoặc `Question` top-level kind. Decision work là
Ticket role, đúng D-03 và D-44.

### Implementation Ticket contract

```jsonc
{
  "role": "implementation",
  "implementation": {
    "mode": "guided",
    "work_surface": "code",
    "plan_policy": "worker_optional",
    "verification_profile": "service-change",
    "brief": {
      "path": "works/TK-031/ticket.md",
      "content_hash": "sha256:..."
    },
    "objective": "Distinguish expired and invalid refresh tokens.",
    "current_behavior": "Both failures map to InvalidToken.",
    "target_behavior": "Expired maps to TokenExpired while invalid stays InvalidToken.",
    "code_anchors": [
      "src/auth/RefreshTokenHandler.ts",
      "src/auth/errors.ts"
    ],
    "required_changes": [
      {
        "id": "CHG-ERROR-TAXONOMY",
        "summary": "Introduce the expired-token domain error."
      }
    ],
    "invariants": [
      {
        "id": "INV-NO-SECRET-LEAK",
        "summary": "Do not expose sensitive authentication details."
      }
    ],
    "acceptance": [
      {
        "id": "AC-EXPIRED",
        "summary": "Expired token produces stable TokenExpired semantics."
      }
    ],
    "scope": {
      "included": ["Domain error, HTTP mapping and contract tests"],
      "excluded": ["Login UI"]
    },
    "implementation_freedom": [
      {
        "id": "FREE-HELPER-STRUCTURE",
        "summary": "Worker may choose internal helper structure."
      }
    ],
    "required_decisions": [
      {
        "id": "DEC-006",
        "contract_revision": 2,
        "acceptance_receipt": {"id": "rcpt_DEC_01J...", "hash": "sha256:..."}
      }
    ],
    "shared_approach_refs": [
      {
        "owner": {"id": "ST-014", "contract_revision": 3},
        "path": "works/ST-014/approach.md",
        "content_hash": "sha256:..."
      }
    ],
    "expected_evidence": [
      "focused_test_output",
      "acceptance_mapping"
    ],
    "expected_handoff": [
      "source_snapshot",
      "acceptance_to_evidence",
      "remaining_risks",
      "documentation_findings"
    ]
  }
}
```

#### Implementation modes

```text
locked
  Must follow referenced accepted direction. Deviation requires re-shape or
  Decision; implementation_freedom may only cover local mechanics.

guided
  Objective, anchors, required changes and invariants are fixed; Worker may
  choose internal structure within declared freedom.

open
  Worker may select a reversible approach inside explicit boundary. Objective,
  acceptance, invariants, public contract and irreversible direction remain
  outside implementation freedom.
```

#### Mechanical contract rules

- objective/current/target behavior are non-empty and bounded;
- at least one acceptance item;
- `work_surface` closed enum starts with `code|documentation|configuration|data|research`;
- `guided|locked` require at least one code anchor when `work_surface=code`;
  other surfaces require equivalent typed anchor/reference appropriate to the
  surface;
- `plan_policy=none|worker_optional|required_before_execution`; Slice 7 only
  validates the policy declaration, not `plan.md` existence because execution
  materialization belongs Phase 2;
- R1–R3 require at least one invariant;
- every item has unique stable ID and bounded summary;
- brief path must equal or live below `content_dir`, be a regular file and exact
  hash must match;
- code anchors are repository-relative path or typed `path#symbol` references;
- required Decisions exist, kind `decision`, contract revision matches, are not
  cancelled/superseded without replacement reconciliation and carry a typed
  accepted-Decision proof; plain node existence is not approval;
- `shared_approach_refs` bind exact owner `contract_revision`/path/hash and are
  the only typed parent-approach alternative to required Decisions for `locked`
  work;
- `locked` requires at least one accepted Decision proof or current approved
  shared approach reference;
- implementation freedom IDs are the only valid targets for delegated branch
  claims;
- plan steps are forbidden from machine contract; `plan.md` remains Phase 2
  execution materialization;
- expected evidence and expected handoff are closed initial enums/profile refs,
  not arbitrary shell output or free-form “done” claims.

### Accepted Decision proof

Slice 7 introduces a small immutable receipt kind:

```text
decision_acceptance
```

It binds Decision ID/contract revision/content hash, accepted outcome summary,
approver principal and source/content snapshot. Current Decision reference in an
implementation/shaping contract points to receipt ID/hash. The local authority
resolver derives the required grant (for example `decision.accept`) from the
operation/risk; receipt does not choose it.

This closes the authority gap for hard-to-reverse branches without pretending
Decision node existence means acceptance. Historical Decision prose remains the
canonical rationale; receipt is approval proof. Full Decision lifecycle UX may
be expanded later.

### Decision-work Ticket contract

```jsonc
{
  "role": "decision_work",
  "decision_work": {
    "destination_owner": {"id": "ST-014", "contract_revision": 3},
    "branch_id": "BR-TOKEN-COMPAT",
    "gap_kind": "tradeoff_gap",
    "question": "Must legacy clients retain the generic invalid-token path?",
    "expected_output": "A Decision with compatibility direction and consequences.",
    "expected_evidence": ["client_contract_inventory"],
    "resolution_target": {
      "kind": "decision",
      "id": "DEC-006"
    },
    "provenance": {
      "shaping_receipt": "rcpt_01J...",
      "fog_id": null
    }
  }
}
```

`gap_kind` closed enum:

```text
fact_gap
intent_gap
tradeoff_gap
fidelity_gap
prerequisite_gap
```

Rules:

- `destination_owner` must be Epic hoặc Story;
- Ticket must have `parent` or `related` relation to destination owner;
- branch ID points to current or historical shaping receipt;
- decision work requires a precise question; vague statement belongs in fog;
- `resolution_target` optional because fact/prototype output can update an
  owning contract without creating Decision;
- hard-to-reverse accepted direction must resolve to a Decision;
- decision work uses normal blockers/preferences/lifecycle and does not create
  a parallel shaping graph;
- implementation fields forbidden on decision-work Ticket.

### Current shaping pointer

Epic/Story/Ticket may add:

```jsonc
{
  "shaping": {
    "receipt": {
      "id": "rcpt_01J...",
      "hash": "sha256:..."
    },
    "map": {
      "path": "works/ST-014/shaping.md",
      "revision": 3,
      "content_hash": "sha256:..."
    },
    "applied_at": "2026-07-25T02:00:00Z",
    "applied_by": "human:quannv"
  }
}
```

Rules:

- receipt ID/hash resolve exact immutable `shaping_validation` receipt;
- receipt subject/owning work matches node contract revision declared by the
  receipt; applying the pointer itself does not retroactively stale that proof;
- map always optional for R0/R1, policy/effort-dependent for R2 and required for
  R3; R2 requires map only when multi-session, multi-decision or explicit
  resume/audit condition is declared;
- map path inside owner `content_dir`, regular file, no symlink escape;
- logical map revision `>=1`; map mutation is external content edit followed by
  a new receipt and shaping apply;
- changing current receipt pointer bumps node revision and emits event;
- old receipt remains historical;
- no branch/fog/frontier array stored in node.

## Current shaping receipt payload v1

### Baseline completion

The current `shaping_validation` payload remains `payload_version: 1`. Slice 7
completes the existing placeholder schema/model in place and updates the evidence
manifest hash for newly bootstrapped target repositories. Internal placeholder
receipts and fixtures are regenerated or rejected as drift; they are not a
supported predecessor family.

Unknown payload versions still fail clearly. Read-only readiness/frontier queries
never bootstrap or rewrite the evidence plane. Typed Rust dispatch remains
explicit by receipt kind and current payload version.

### Envelope requirements

A readiness-eligible current payload v1 receipt requires:

- `kind=shaping_validation`;
- `result=passed`;
- subject is owning work;
- exact owning work `contract_revision` binding plus normal revision observed
  for audit;
- exact `contract_revision` for every directly affected
  implementation/decision-work Ticket;
- generic envelope work bindings still name observed node revisions, but the
  shaping validator does not make later pointer/status-only bumps stale;
- exact content binding for Ticket brief/Story approach reviewed;
- exact map content binding when map exists;
- exact content binding for referenced Decision prose when semantic resolution
  depends on it;
- source binding requirement derives from reviewed surfaces: code/config/data
  claims require clean Git commit; content-only non-code shaping may use
  `source_posture=not_required_content_bound` when policy allows;
- matching recording event anchor and canonical receipt hash.

### Payload model

```jsonc
{
  "payload_version": 1,
  "owning_work": {"id": "ST-014", "revision_observed": 7, "contract_revision": 3},
  "materialization": "R2",
  "shape_mode": "persisted_map",
  "source_posture": "clean_git_commit",
  "destination": {
    "summary": "Deliver stable refresh-token failure semantics.",
    "scope_boundary": [
      "Refresh-token domain classification",
      "HTTP error mapping"
    ],
    "exit_conditions": [
      "All critical branches affecting TK-031 have non-blocking dispositions",
      "Implementation contract has stable acceptance and invariants"
    ]
  },
  "map": {
    "path": "works/ST-014/shaping.md",
    "revision": 3,
    "content_hash": "sha256:..."
  },
  "affected_work": [
    {"id": "TK-031", "revision_observed": 4, "contract_revision": 2}
  ],
  "branches": [
    {
      "id": "BR-TOKEN-COMPAT",
      "question": "Must legacy clients retain the generic invalid-token path?",
      "gap_kind": "tradeoff_gap",
      "criticality": "critical",
      "affected_work": ["TK-031"],
      "disposition": {
        "kind": "resolved",
        "resolution": {
          "kind": "decision",
          "id": "DEC-006",
          "revision": 2,
          "gist": "Preserve compatibility while splitting domain semantics."
        }
      }
    },
    {
      "id": "BR-HELPER-STRUCTURE",
      "question": "Which internal helper layout should be used?",
      "gap_kind": "fidelity_gap",
      "criticality": "non_critical",
      "affected_work": ["TK-031"],
      "disposition": {
        "kind": "delegated",
        "freedom_id": "FREE-HELPER-STRUCTURE",
        "reason": "The choice cannot alter acceptance, invariant or public contract."
      }
    }
  ],
  "fog": [
    {
      "id": "FOG-AUTH-TELEMETRY",
      "statement": "Telemetry implications may become visible after the prototype.",
      "bounds": [
        "Authentication telemetry only",
        "No current acceptance depends on this area"
      ],
      "why_not_precise": "The emitted event shapes are not yet known.",
      "review": "bounded_non_blocking",
      "trigger": "Reconcile after TK-030 prototype evidence.",
      "affected_work": ["TK-031"]
    }
  ],
  "out_of_scope": [
    {
      "id": "OOS-LOGIN-UI",
      "statement": "Login UI recovery changes.",
      "reason": "Outside the approved destination."
    }
  ],
  "resolution_pointers": [
    {
      "kind": "decision",
      "id": "DEC-006",
      "revision": 2,
      "gist": "Compatibility direction for token errors."
    }
  ],
  "approval": {
    "approved_by": {"kind": "human", "id": "quannv"},
    "reference": "PULSE.md#human-judgment-boundaries"
  },
  "reconciliation": {
    "supersedes_receipt": "rcpt_01H...",
    "surfaced_branch_ids": ["BR-TOKEN-COMPAT"],
    "invalidated_branch_ids": [],
    "graduated_fog_ids": [],
    "affected_work": ["TK-031"]
  },
  "remaining_uncertainty": [
    {
      "summary": "Exact helper naming remains delegated.",
      "trigger": "Escalate only if it changes a public symbol."
    }
  ]
}
```

### Shape mode

```text
concise_self_check
focused_branches
persisted_map
```

Policy defaults:

- R0: `concise_self_check`; map forbidden as a requirement unless explicit
  elevation reason;
- R1: concise or focused;
- R2: focused by default; persisted map required only when typed effort flags
  say multi-session, multiple dependent decisions or resume/audit continuity is
  needed;
- R3: persisted map required;
- a higher materialization may use deeper mode;
- lower mode than required makes receipt structurally valid historical evidence
  but readiness-ineligible.

### Destination rules

- destination required for persisted map;
- summary, scope boundary and at least one exit condition non-empty;
- destination is outcome/boundary, not implementation steps;
- R0 concise receipt may omit destination object when owning Ticket objective +
  scope already provide equivalent boundary;
- destination owner is receipt subject;
- redraw destination requires new receipt and appropriate grant;
- out-of-scope entries cannot be silently promoted into current destination.

### Branch identity and criticality

- IDs follow bounded portable grammar such as `BR-[A-Z0-9-]+`;
- unique within a shaping lineage;
- same semantic branch should preserve ID across replacement receipts;
- branch question non-empty and precise;
- `criticality=critical|non_critical`;
- every critical branch affecting a Ticket must have exactly one disposition;
- branch arrays normalize by ID for deterministic bytes;
- a branch cannot simultaneously be fog and out-of-scope.

### Disposition rules

#### `resolved`

Requires:

- typed canonical resolution reference;
- target exists and revision matches;
- non-empty gist;
- Decision kind for hard-to-reverse tradeoff/intent with durable consequence;
- receipt content/work bindings cover reviewed resolution;
- affected implementation contract does not contradict the resolution.

Kernel validates references/revisions; semantic contradiction review remains
receipt assertion and future reviewer capability.

#### `rejected`

Requires:

- non-empty rationale;
- optional evidence/reference;
- any decision-work Ticket whose only purpose is this branch must be terminal,
  cancelled, superseded hoặc explicitly retained with rationale;
- rejected branch is non-blocking but history remains inspectable.

#### `delegated`

Requires:

- affected work is implementation Ticket;
- implementation mode is `guided|open`;
- `freedom_id` resolves exact `implementation_freedom` entry;
- reason states why choice cannot change objective, acceptance, invariant,
  public contract or irreversible direction;
- `locked` work cannot accept delegated critical branch;
- missing/mismatched freedom makes readiness fail with
  `shaping_delegation_exceeds_freedom`.

#### `deferred`

Payload:

```jsonc
{
  "kind": "deferred",
  "reason": "Not required for the current compatible slice.",
  "owner": "team:identity",
  "target_work": "TK-099",
  "trigger": "Before enabling cross-service telemetry.",
  "non_blocking_for": ["TK-031"]
}
```

Rules:

- reason required;
- owner or target work required; proposal requires both when target exists;
- trigger or linked work required;
- target work resolves and is not already invalid terminal state;
- explicit `non_blocking_for` includes affected current Tickets;
- deferral that may make current acceptance/invariant wrong is invalid;
- kernel validates structure/reference, reviewer owns semantic honesty.

#### `blocking`

Requires:

- precise question;
- affected work list;
- optional linked decision work;
- every affected implementation Ticket fails readiness;
- receipt can still be integrity-valid and useful even when result is
  `inconclusive`; only `passed` current receipt can support `shaped` transition.

### Fog rules

`not_yet_specified` is represented by `fog` entries, not work nodes.

Each fog entry requires:

- stable ID;
- in-scope statement;
- explicit bounds;
- reason question is not yet precise;
- reconsideration trigger;
- `review=bounded_non_blocking` assertion;
- affected work IDs.

Reject/fail readiness when:

- bounds empty;
- trigger empty;
- fog duplicates a known blocking branch;
- statement is already a precise actionable question;
- entry is really backlog/deferred work;
- entry is actually out of scope;
- reviewer cannot assert non-blocking for current contract.

Kernel can detect obvious structural forms and duplicate IDs/references. It
cannot fully prove a sentence is semantically fog rather than a hidden blocker;
that remains shaping reviewer judgment bound into receipt.

### Out-of-scope rules

- stable ID, statement and reason;
- does not participate in frontier;
- cannot be used to hide an acceptance requirement;
- changing destination to include it requires new receipt and authority;
- map projection displays it separately from fog.

### Reconciliation provenance

A replacement receipt may reference prior receipt and record:

- surfaced branches;
- invalidated branches;
- graduated fog;
- affected work;
- canonical resolutions added/changed.

Rules:

- prior receipt exists/hash valid;
- referenced old branch/fog IDs existed;
- current receipt cannot claim unknown historical IDs;
- semantic mutation of graph/content is still performed through normal
  work/edge/Decision APIs before recording new receipt;
- receipt records reviewed result; it does not directly cancel/supersede nodes.

## Local authority policy

> **Đã khóa:** `.pulse/policy/authority.json` là canonical enforceable
> principal/grant registry. `PULSE.md` sở hữu human-readable policy intent;
> `.pulse/config.yaml` chỉ giữ operational configuration. Owner docs phải được
> cập nhật cùng implementation để phản ánh boundary này.

### Contract

```jsonc
{
  "schema_version": 1,
  "revision": 1,
  "principals": [
    {
      "kind": "human",
      "id": "quannv",
      "grants": [
        "shape.approve.R0",
        "shape.approve.R1",
        "shape.approve.R2",
        "shape.approve.R3",
        "shape.apply",
        "shape.invalidate",
        "shape.destination.redraw",
        "decision.accept",
        "work.transition.shaped",
        "work.transition.ready",
        "work.materialization.downgrade",
        "documentation.defer",
        "qa.none.approve",
        "qa.defer_to_story_close"
      ]
    }
  ]
}
```

Rules:

- default deny;
- exact `(kind,id)` match;
- actor syntax compatible evidence `human|agent|system`;
- no wildcard grant in v1;
- principal/grants sorted deterministic;
- policy revision/fingerprint participates readiness fingerprint;
- required grants are derived by kernel from operation, materialization,
  destination change and posture; receipt cannot choose or under-declare them;
- receipt approval principal must own kernel-derived materialization grant;
- shaping apply/invalidate, destination redraw, Decision acceptance,
  materialization downgrade, documentation deferral, QA-none approval,
  Story-close deferral and lifecycle transitions each have closed grant names;
- transition caller must own transition grant;
- Worker/Agent does not gain grant because transport lets it invoke CLI;
- file is tracked canonical policy metadata; normal mutation must eventually
  use CAS/event tooling, while deliberate maintainer hand edit is recoverable
  only through validation/audit and must pass
  `pulse work readiness-policy validate`;
- no cryptographic authentication claim: local policy prevents accidental role
  overreach, not a malicious writer with repository filesystem access;
- public grant/revoke commands defer Phase 3.

Minimum CLI:

```text
pulse work readiness-policy show [--json]
pulse work readiness-policy validate [--json]
```

If policy file is missing, authority gate is `unavailable`, not passed. There is
no implicit `human:*` superuser in machine output.

## Shaping apply/invalidate mutation

### Apply command

```text
pulse work shaping apply <owner-id>
  --receipt <receipt-id>
  --expected-revision <n>
  [--expected-current-receipt <receipt-id>]
  --actor <kind:id>
  [--json]
```

Preconditions:

1. owner exists and normal revision matches;
2. receipt exists, integrity valid, kind `shaping_validation`, current payload v1, result passed;
3. receipt subject/owning work matches current owner `contract_revision`;
4. required contract/content/source/map bindings current;
5. branch/fog/disposition structural invariants pass;
6. materialization/shape mode policy pass;
7. receipt approval principal has kernel-derived materialization grant;
8. caller has `shape.apply` grant;
9. expected current receipt matches to prevent lost reconciliation;
10. graph/docs/evidence/policy snapshot coherent under repository fence.

Mutation:

- set current shaping pointer;
- bump owner revision;
- emit `work.shaping.applied` with old/new receipt IDs/hashes, map revision,
  affected work IDs and readiness invalidation summary;
- invalidate disposable graph/readiness projection;
- no status transition in same command by default.

Receipt binds owner pre-apply `contract_revision`; apply bumps only normal node
revision, so the proof remains current. Event records before/after normal
revision and unchanged contract revision. A replacement receipt binds the latest
contract revision after any semantic contract mutation, not merely after pointer
or status updates.

Retry:

- same current receipt/hash/map returns `unchanged` even if expected revision is
  stale, after verifying historical apply event;
- same receipt ID different hash is corruption;
- different receipt with stale expected current receipt conflicts;
- no silent latest-wins behavior.

### Invalidate command

```text
pulse work shaping invalidate <owner-id>
  --expected-revision <n>
  --reason <text>
  --actor <kind:id>
  [--json]
```

- clears current shaping pointer only with authority;
- bumps revision and emits `work.shaping.invalidated`;
- historical receipt remains;
- does not delete map;
- shaped/ready status is not auto-mutated, but readiness becomes stale/not ready;
- caller may separately transition `ready -> shaped` using existing audited
  transition reason.

## Readiness composition

### Readiness profile

Initial profile ID:

```text
phase1_contract_readiness_v1
```

Only implementation Ticket can be ready under this profile. Decision-work
Tickets use decision-frontier eligibility, not implementation readiness.

### Gate statuses

```text
passed
failed
stale
not_applicable
not_evaluated
unavailable
```

- `failed`: current inputs definitely violate contract;
- `stale`: previously valid/reference input no longer current;
- `unavailable`: required resolver/capability missing;
- `not_evaluated`: consumer intentionally did not evaluate future family;
- `not_applicable`: profile says family irrelevant;
- transition eligible only when every required current-profile family is
  `passed|not_applicable`.

### Readiness report

```jsonc
{
  "schema_version": 1,
  "subject": {"id": "TK-031", "revision": 4},
  "profile": "phase1_contract_readiness_v1",
  "status": "ready",
  "transition_eligible": true,
  "dispatch_authorized": false,
  "readiness_fingerprint": "sha256:...",
  "graph_fingerprint_observed": "sha256:...",
  "gate_families": [
    {
      "family": "graph_validity",
      "status": "passed",
      "reason_codes": []
    },
    {
      "family": "structural_executability",
      "status": "passed",
      "reason_codes": []
    },
    {
      "family": "implementation_contract",
      "status": "passed",
      "reason_codes": []
    },
    {
      "family": "shaping",
      "status": "passed",
      "reason_codes": []
    },
    {
      "family": "documentation",
      "status": "passed",
      "reason_codes": []
    },
    {
      "family": "authority",
      "status": "passed",
      "reason_codes": []
    }
  ],
  "destination": {
    "owner": "ST-014",
    "receipt": "rcpt_01J...",
    "map_revision": 3
  },
  "remaining_non_blocking_uncertainty": ["FOG-AUTH-TELEMETRY"],
  "future_gate_families": [
    {"family": "qa_baseline_and_cases", "owner_phase": 3, "status": "not_evaluated"},
    {"family": "lease", "owner_phase": 2, "status": "not_evaluated"},
    {"family": "source_workspace", "owner_phase": 2, "status": "not_evaluated"}
  ]
}
```

Top-level status:

```text
ready
not_ready
stale
invalid
```

`dispatch_authorized` remains false because readiness is not lease/run
permission.

### Gate families and order

Evaluate in fixed order:

1. `graph_validity`
2. `work_kind_and_role`
3. `lifecycle_eligibility`
4. `structural_executability`
5. `implementation_contract`
6. `required_decisions`
7. `shaping_receipt_integrity`
8. `shaping_bindings`
9. `branch_dispositions`
10. `destination_and_map`
11. `bounded_fog`
12. `authority`
13. `documentation_impact`
14. `applicable_documents`
15. `qa_impact`
16. `content_reference_integrity`

Independent failures should be accumulated when safe. Parse/corruption/graph
invalidity may stop dependent checks but report skipped dependencies clearly.

### Structural executability integration

Readiness consumes existing report:

- only `structural_state=candidate` passes for transition to ready;
- open/unknown hard blocker fails;
- soft preference never fails readiness;
- superseded/terminal/paused states fail;
- `dispatch_authorized=false` from structural report is expected and not itself
  readiness failure.

`MISSING_GATE_FAMILIES` in executability should evolve to describe ownership,
but structural module must not import docs/evidence/readiness.

### Documentation gate

Consume existing `docs applicable --work` logic:

- posture missing/`unknown` fails;
- `none` passes only with rationale already structurally valid;
- `required` passes only when every explicit required doc is current,
  authoritative and content-readable;
- `deferred` requires current linked follow-up work and local policy permits
  deferral;
- retired/stale/superseded/missing required doc fails;
- scope matches remain optional unless explicitly required;
- exact required doc revisions/content hashes participate readiness fingerprint;
- Slice 7 does not require docs validation receipt for ready; that belongs
  close/verification profile later.

### Minimal QA-impact gate

Readiness consumes the current node metadata:

```jsonc
{
  "qa": {
    "impact": {
      "posture": "unknown|required|covered_by_story_close|none",
      "rationale": "Internal refactor; behavior contract unchanged.",
      "behavioral_owner": "ST-014",
      "affected_case_ids": []
    }
  }
}
```

Rules:

- missing metadata derives `unknown`;
- `unknown` always fails ready;
- `none` requires non-empty rationale, contract declares no
  behavior/public-risk change and approval principal has `qa.none.approve`;
- `covered_by_story_close` requires Story behavioral owner + rationale and
  approval principal has `qa.defer_to_story_close`; case IDs may remain empty
  until Phase 3, but full Story qualification remains required for close;
- `required` requires behavioral owner and at least one affected/new case ID;
  until Phase 3 baseline resolver can validate those IDs, gate is `unavailable`;
- Slice 7 owns only posture mutation/CAS/event and readiness structure, not
  baseline parsing, executor selection or QA receipts.

### Required Decisions/content gate

- all references resolve exact ID/revision;
- Decision must not be cancelled/superseded without reconciled replacement;
- hard-to-reverse Decision requires a typed acceptance proof or local authority
  record; mere existence or shaping-summary approval is insufficient;
- until Decision acceptance proof exists, `locked`/tradeoff work depending on it
  is `unavailable`, not passed;
- brief/approach/map/Decision content hashes match receipt bindings;
- missing content is failure;
- content byte change after receipt makes readiness stale;
- kernel never chooses between contradictory contracts.

### Branch gate

For subject Ticket, select branches whose `affected_work` includes subject.

Pass when:

- every critical branch has exactly one non-blocking valid disposition;
- no branch is `blocking`;
- every delegated freedom resolves and is within implementation mode;
- every deferred branch satisfies reason/owner/target/trigger/non-blocking
  contract;
- resolved/rejected references current;
- receipt is current for subject `contract_revision`; normal status/pointer
  revision bumps do not stale it.

### Fog gate

- bounded non-blocking fog is allowed and returned as remaining uncertainty;
- malformed/unbounded fog fails;
- fog does not become frontier item automatically;
- out-of-scope never appears as fog;
- a precise decision-work Ticket may reference `provenance.fog_id` after semantic
  graduation, but Slice 7 does not perform graduation itself.

### Readiness fingerprint

Fingerprint uses explicit **gate projections**, not whole node JSON. This avoids
lifecycle/timestamp/pointer-only mutations invalidating semantic contract proof.

Canonical hash inputs:

```text
readiness profile/version
+ subject ID + contract_revision + normalized contract/readiness fields
+ relevant owner IDs + contract revisions + destination projection
+ relevant hard blocker edge + endpoint semantic status projection
+ required Decision acceptance proof + revision + content hashes
+ current shaping receipt ID/hash
+ map path/revision/content hash
+ affected branch/fog normalized projection
+ authority policy revision/fingerprint
+ documentation impact metadata
+ minimal QA impact metadata
+ required docs registry records/revisions/content hashes
+ brief/shared-approach content bindings
```

Excluded:

- subject normal revision, lifecycle status, status reason, timestamps and
  shaping applied-at/by fields;
- unrelated graph nodes/edges;
- events;
- cache/runtime state;
- optional lexical search ranking;
- unrelated registry edits;
- knowledge entries;
- future lease/claim state.

Global graph fingerprint remains reported for audit and frontier co-snapshot.

### Current/stale semantics

- same narrow inputs -> same fingerprint;
- unrelated work mutation leaves fingerprint unchanged;
- lifecycle-only shaped/ready transition leaves semantic readiness fingerprint
  unchanged;
- contract revision, relevant blocker semantic state, current receipt, bound
  content, Decision proof, required doc, QA impact or policy change changes
  fingerprint/status;
- `ready` lifecycle node whose current report fails is labeled
  `ready_state_stale`;
- cache cannot repair canonical inputs.

## Lifecycle integration

### `draft -> shaped`

Open through dedicated gate, not generic ungated table.

Requires:

- current shaping-validation payload v1 receipt applied;
- receipt integrity/current bindings pass;
- materialization/shape mode/destination/map requirements pass;
- branch/fog structural validation pass;
- kernel-derived shaping approval grant pass;
- no need for full implementation readiness yet;
- reason not required on success.

Transition event `work.node.transitioned` adds:

```jsonc
{
  "gate_profile": "phase1_shaped_v1",
  "shaping_receipt": {"id": "rcpt_...", "hash": "sha256:..."},
  "input_fingerprint": "sha256:..."
}
```

### `shaped -> ready`

Requires complete `phase1_contract_readiness_v1` pass under write fence.

CLI may accept expected fingerprint to prevent acting on an old query:

```text
--expected-readiness-fingerprint sha256:...
```

Kernel always recomputes immediately before commit. Missing expected fingerprint
is allowed for interactive convenience but output returns observed fingerprint;
strict automation/Orchestrator should provide it.

### Blocked resume

Slice 7 does not open direct `blocked -> ready` because current structural
executability intentionally classifies explicit `blocked` status as paused.
Resume path is:

```text
blocked -> shaped   # audited reason, clears paused status
shaped -> ready     # fresh full readiness gate
```

This keeps structural semantics non-target-aware and avoids a special evaluator
that ignores current lifecycle only for one transition.

### `rework -> shaped|ready`

Remain gated until Phase 2 verification/rework receipt exists. Slice 7 does not
open it merely because readiness contract passes.

### Existing transitions

- `ready -> shaped` remains supported with reason to invalidate readiness;
- no `--force` for shaped/ready;
- active/verifying/done remain gated by Phase 2/3 capabilities;
- generic lifecycle pure table should distinguish `GateProfileRequired` from
  permanently unavailable transition.

## Decision frontier

### Membership

A Ticket appears in decision frontier when:

- `role=decision_work`;
- destination owner matches `--for` when provided;
- destination owner current shaping receipt/map is valid enough to identify the
  effort;
- Ticket lifecycle is `draft|shaped|ready` and not terminal/cancelled/
  superseded;
- precise question/gap/output/evidence contract valid;
- hard dependencies are mechanically satisfied when evaluating a
  **decision-work candidate**; draft lifecycle itself is not treated as
  `work_not_shaped` for this specialized projection;
- linked branch remains relevant and not resolved/rejected/invalidated in
  current shaping receipt.

Decision work does not need its own nested shaping receipt merely to enter the
frontier. Its typed precise-question contract is the lightweight executable
boundary; this avoids recursively shaping the work created by shaping.

### Output

```jsonc
{
  "schema_version": 1,
  "kind": "decision",
  "for": "ST-014",
  "graph_fingerprint": "sha256:...",
  "shaping_context": {
    "receipt_id": "rcpt_01J...",
    "receipt_hash": "sha256:...",
    "map_revision": 3
  },
  "claim_state": "not_evaluated",
  "items": [
    {
      "id": "TK-030",
      "revision": 2,
      "gap_kind": "fidelity_gap",
      "branch_id": "BR-PROTOTYPE",
      "structural_state": "candidate",
      "reason_codes": ["open_decision_work"]
    }
  ],
  "excluded": [
    {
      "id": "TK-032",
      "reason_codes": ["decision_work_blocked"]
    }
  ]
}
```

### Unclaimed boundary

Owner docs define frontier as `open + unblocked + unclaimed`, nhưng claim/lease
thuộc Phase 2 runtime. Slice 7 output must say:

```text
claim_state=not_evaluated
```

Phase 2 composes graph frontier with live reservations/leases to produce
available dispatch frontier. Slice 7 never persists or fabricates claim state.

## Execution frontier

### Membership

A Ticket appears when:

- `role=implementation`;
- lifecycle status exactly `ready`;
- current readiness status exactly `ready` under requested profile;
- structural executability candidate;
- minimal QA impact gate current (`none` or valid
  `covered_by_story_close`; `required` waits for Phase 3 resolver);
- not superseded/terminal;
- no hard blocker open.

`shaped` Ticket with a readiness report pass is a readiness candidate but not in
execution frontier until explicit transition to `ready` records the authority
boundary.

### Output

```jsonc
{
  "schema_version": 1,
  "kind": "execution",
  "for": "ST-014",
  "graph_fingerprint": "sha256:...",
  "readiness_profile": "phase1_contract_readiness_v1",
  "claim_state": "not_evaluated",
  "dispatch_authorized": false,
  "items": [
    {
      "id": "TK-031",
      "revision": 5,
      "readiness_fingerprint": "sha256:...",
      "frontier_eligible": true,
      "reason_codes": ["contract_ready"]
    }
  ],
  "excluded": [
    {
      "id": "TK-033",
      "reason_codes": ["ready_state_stale"]
    }
  ]
}
```

### Parent filter

`--for <epic-or-story-id>` includes descendants reached through `parent`
relation and standalone Tickets explicitly related to destination owner when
shaping contract names that owner. Traversal bounded/cycle-safe and deterministic.

### Ordering

- deterministic by subject ID in v1;
- soft preference included as metadata, not membership blocker;
- priority included as display metadata only when schema supports it;
- semantic scheduling/order is a separate reconciliation result/event.

## CLI surface

```text
pulse work contract set <ticket-id>
  --file <implementation-or-decision-work-contract.json>
  --expected-revision <n>
  --actor <kind:id>
  [--json]

pulse work qa-impact <ticket-id>
  --posture <required|covered_by_story_close|none>
  [--rationale <text>]
  [--behavioral-owner <story-id>]
  [--case <case-id>]...
  --expected-revision <n>
  --actor <kind:id>
  [--json]

pulse work shaping apply <owner-id>
  --receipt <receipt-id>
  --expected-revision <n>
  [--expected-current-receipt <receipt-id>]
  --actor <kind:id>
  [--json]

pulse work shaping show <owner-id> [--json]

pulse work shaping invalidate <owner-id>
  --expected-revision <n>
  --reason <text>
  --actor <kind:id>
  [--json]

pulse work ready <ticket-id>
  [--profile phase1_contract_readiness_v1]
  [--json]

pulse work frontier
  --kind <decision|execution>
  [--for <epic-or-story-id>]
  [--profile phase1_contract_readiness_v1]
  [--include-excluded]
  [--json]

pulse work readiness-policy show [--json]
pulse work readiness-policy validate [--json]

pulse work transition <id>
  --to <shaped|ready|...>
  --expected-revision <n>
  [--expected-readiness-fingerprint <sha256>]
  --actor <kind:id>
  [--json]
```

Command semantics:

- `work ready` is read-only query, not status mutation;
- `work transition --to shaped|ready` recomputes gate under lock;
- `work contract set` is typed replacement/patch, not arbitrary JSON Patch;
- `work qa-impact` owns only minimal posture metadata and does not validate
  Story baseline/cases beyond structural references until Phase 3;
- `work shaping apply` references already-recorded immutable receipt;
- no `work frontier set` command;
- no `--force`;
- machine output has `schema_version`, stable `code`, profile/fingerprint and
  reason codes;
- graph/corruption/integrity errors non-zero;
- valid but `not_ready` query returns structured report and a documented
  non-zero gate exit suitable for CI/automation;
- empty frontier is success.

Deferred:

```text
pulse shape ...
pulse work packet ...
pulse work claim|release
pulse run ...
pulse work close
pulse qa ...
```

## Library/module layout đề xuất

```text
src/
  graph/
    node.rs                    # current v1 role/contract/shaping pointer
    contract.rs                # implementation + decision-work validation
    shaping.rs                 # current pointer/apply/invalidate + map checks
    readiness.rs               # pure gate-family composition
    frontier.rs                # decision/execution projections
    lifecycle.rs               # profile-gated transition direction
    executability.rs           # remains structural-only
    projection.rs              # optional readiness/frontier export additions
    store.rs                   # coherent reads, CAS, events, recovery

  evidence/
    model.rs                   # shaping/Decision payload typed dispatch
    shaping.rs                 # current v1 structural validation/currentness
    decision.rs                # immutable Decision acceptance proof
    receipt.rs                 # shared integrity/binding validation

  policy/
    mod.rs
    authority.rs               # local principal/grant resolver

  schema/
    node.schema.json
    evidence/
      shaping-validation.v1.schema.json
      decision-acceptance.v1.schema.json
    policy/
      authority.schema.json

  bin/
    pulse.rs                   # thin clap + renderer

tests/
  shaping_contract.rs
  shaping_receipt_v1.rs
  readiness.rs
  readiness_cli_contract.rs
  frontier.rs
  readiness_process_concurrency.rs
  readiness_crash_recovery.rs
```

Boundary rules:

- readiness pure function consumes a coherent typed snapshot and resolver
  reports; it does not read raw JSON;
- docs applicability remains owned by `src/docs`;
- evidence validates receipts; graph does not duplicate receipt parser;
- structural executability does not depend on readiness;
- authority resolver has no transport/network concerns;
- frontier consumes readiness/executability results and never mutates state;
- binary does not calculate fingerprints, resolve references or decide gates.

## Transaction, recovery và consistency

### Current baseline publication

Bootstrap templates, Rust models, JSON Schemas, tests and fixture repositories
are updated together to the completed current v1 baseline. Existing transaction
recovery remains responsible for actual node/event and receipt/event mutations;
there is no schema-migration transaction or migration event in Slice 7. Read-only
commands never bootstrap or rewrite canonical planes.

Slice 7 also reconciles `src/event.rs` to the typed current event envelope v1 in
[`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md#event-envelope):
`schema_version`, `id`, `event_type`, `occurred_at`, typed actor/subject,
optional typed correlation, and event-specific payload. The earlier internal
string actor/subject encoding is updated in place, not retained through an event
v2 or predecessor decoder.

### Contract set

Node + event single-target transaction:

1. acquire fence/recover;
2. load graph/docs/evidence/policy as needed;
3. compare expected revision;
4. validate typed contract/references/content hash;
5. update node revision/timestamp;
6. prepare `work.contract.updated` event;
7. commit node + event;
8. cache becomes stale by fingerprint.

### Shaping apply/invalidate

Node + event single-target transaction because receipt already exists.

Apply re-verifies receipt/hash/bindings under fence immediately before commit.
Crash after node before event completes exactly one event. Orphan receipt is not
corruption and may be listed as unapplied evidence.

### Ready transition

Critical section:

1. acquire repository fence;
2. recover all pending canonical transactions;
3. load coherent graph, docs registry, authority policy and receipt;
4. read/hash bound work/docs/map/Decision content;
5. calculate readiness report/fingerprint;
6. compare optional expected fingerprint;
7. re-read/re-hash every external canonical input before prepare — bound
   content, authority policy, evidence manifest/schema and receipt/event anchor
   — to reduce TOCTOU with editors that do not honor lock;
8. update status/normal revision only; contract revision remains unchanged;
9. prepare event containing profile and evaluated fingerprint;
10. commit node + event.

Recovery uses prepared event/result and must not recompute readiness against a
later repository state to fabricate historical evidence.

### Concurrent content edits

Pulse cannot prevent arbitrary editor writes outside lock. It must detect:

- hash changed during gate evaluation -> `readiness_inputs_changed`;
- map/doc/brief/Decision bytes no longer match receipt -> stale/fail;
- authority policy or evidence manifest/schema changed -> abort and reload;
- retry caller must reload rather than auto-retry semantic transition.

### Readers

`work ready` and frontier queries:

- recover under repository fence;
- capture coherent graph/registry/policy/receipt references and canonical
  content hashes;
- may release fence after immutable snapshot capture;
- output reports snapshot fingerprint;
- never reads cache as truth.

## Projection/cache evolution

### Graph export

`graph export` may bump projection schema and add:

```jsonc
{
  "readiness": {
    "profile": "phase1_contract_readiness_v1",
    "tickets": {}
  },
  "frontiers": {
    "decision": {},
    "execution": {}
  }
}
```

Avoid O(V×full-doc-corpus) export. Options:

- default export includes compact readiness summaries/reason codes;
- per-work `work ready` computes detailed report;
- frontier computes only candidate sets;
- benchmark before embedding every docs result in graph snapshot.

### Readiness cache

Optional:

```text
.pulse/cache/readiness.snapshot.json
```

- key by profile + sorted subject readiness fingerprints;
- missing/stale/corrupt discard/rebuild;
- cache cannot make transition pass without canonical recomputation;
- delete/rebuild equivalent semantics;
- Phase 1 may initially compute in-memory and reserve file only after benchmark.

## Validation and finding codes

Minimum stable codes:

```text
work_role_invalid
implementation_contract_missing
implementation_mode_missing
implementation_anchor_missing
implementation_invariant_missing
implementation_acceptance_missing
implementation_freedom_missing
implementation_brief_missing
implementation_brief_hash_stale
required_decision_missing
required_decision_revision_stale
decision_work_contract_missing
decision_work_destination_invalid
decision_work_branch_missing
decision_work_question_invalid
shaping_receipt_missing
shaping_receipt_version_ineligible
shaping_receipt_stale
shaping_receipt_subject_mismatch
shaping_receipt_hash_mismatch
shaping_map_required
shaping_map_path_unsafe
shaping_map_missing
shaping_map_revision_stale
shaping_map_content_stale
shaping_destination_missing
shaping_exit_condition_missing
shaping_branch_missing_disposition
shaping_branch_duplicate
shaping_resolution_missing
shaping_rejection_reason_missing
shaping_delegation_exceeds_freedom
shaping_defer_owner_missing
shaping_defer_target_missing
shaping_defer_reason_missing
shaping_defer_trigger_missing
shaping_defer_not_non_blocking
shaping_blocking_branch_open
shaping_fog_unbounded
shaping_fog_trigger_missing
shaping_fog_hides_precise_question
shaping_reconciliation_reference_invalid
readiness_policy_missing
readiness_policy_invalid
readiness_authority_denied
decision_acceptance_missing
decision_acceptance_stale
qa_impact_unknown
qa_impact_invalid
qa_baseline_resolver_unavailable
readiness_profile_unsupported
readiness_inputs_changed
readiness_fingerprint_mismatch
readiness_not_ready
ready_state_stale
frontier_kind_invalid
frontier_destination_invalid
frontier_claim_state_not_evaluated
```

`graph validate` extensions:

1. current schema v1 role/contract combinations;
2. role only on Ticket;
3. brief/map path safety and current hash;
4. current shaping pointer receipt identity/hash/subject;
5. decision-work destination relation;
6. required Decision references and acceptance receipts;
7. minimal QA-impact structural metadata;
8. authority policy schema/repository consistency and canonical bytes;
9. status ready with stale current readiness as finding, not canonical parse
   corruption;
10. frontier cache ignored for canonical correctness.

Validation never:

- writes missing contract prose;
- chooses branch disposition;
- decides fog semantic truth;
- grants authority;
- auto-mutates stale ready status;
- creates/cancels decision work.

## Implementation breakdown cho repository phát triển Pulse

Đây là delivery breakdown để implement **Pulse harness binary/library** trong
repository này. Nó không phải canonical `.pulse/workgraph` của một target
repository và không được bootstrap bằng binary chưa hoàn thành. Khi Pulse có
self-hosting/import capability được chấp thuận riêng, các units dưới đây có thể
được import có chủ ý; trước đó proposal + Git history là planning artifact.

| Unit | Outcome | Phụ thuộc |
|---|---|---|
| S7-I1 | Reconcile typed event envelope v1 and verify the existing current node v1 contract foundation stays aligned across schema/Rust/tests | — |
| S7-I2 | Authority policy loader/fingerprint, completed shaping receipt payload v1 and Decision acceptance proof | S7-I1 |
| S7-I3 | Contract/QA/shaping/Decision mutation APIs, CLI, CAS, events và recovery | S7-I1, S7-I2 |
| S7-I4 | Readiness composition, narrow fingerprint, stale-ready semantics và lifecycle gates qua `ready` | S7-I2, S7-I3 |
| S7-I5 | Deterministic decision/execution frontier projections và CLI | S7-I4 |
| S7-I6 | Crash/concurrency/TOCTOU hardening, benchmarks, docs reconciliation và Phase 1 exit evidence | S7-I1–S7-I5 |

Recommended implementation sequence:

```text
S7-I1 -> S7-I2 -> S7-I3 -> S7-I4 -> S7-I5 -> S7-I6
```

`S7-I1` là entry point tiếp theo. Mỗi unit nên là một coherent Git change hoặc
reviewable series với tests; không tạo Pulse work nodes trong chính repository
này chỉ để mô phỏng target-repository usage.

## Test matrix

### Current schema v1

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-01 | Current node v1 schema/Rust/template alignment | prerequisite | One current baseline; no v2/predecessor/migration path |
| S7-02 | Unknown or manually drifted repository schema | integrity | Reject and preserve files; do not infer/migrate |
| S7-03 | Public create and canonical draft classification | contract | Public create requires assessed classification; draft/bootstrap may use explicit unassessed without fake contract |
| S7-04 | Role fields on non-Ticket | schema | Reject |
| S7-05 | Implementation + decision-work fields together | schema | Reject |

### Implementation/decision-work contracts

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-06 | Missing implementation mode/acceptance | #9 | Not ready |
| S7-07 | Guided Ticket missing anchors | #9 | Not ready unless typed non-code exception |
| S7-08 | R1–R3 missing invariant | #9 | Not ready |
| S7-09 | Stale brief content hash | integrity | Contract/readiness stale |
| S7-10 | Locked Ticket missing Decision/approach | #9 | Not ready |
| S7-11 | Valid concise R0 correction | #10/#35 | No plan/map/ADR/human interview required |
| S7-12 | Decision-work invalid owner/relation | #37 | Reject |
| S7-13 | Draft decision-work precise question + expected evidence | #37 | Frontier eligible without nested shaping receipt when dependencies satisfied |

### Shaping receipt/apply

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-14 | Placeholder/internal shaping receipt bytes | development drift | Regenerate fixture or reject; only completed current payload v1 is gate-eligible |
| S7-15 | Valid R0 concise receipt | #35 | Can support shaped without map |
| S7-16 | R2 multi-session effort missing destination/map | #37 | Shaped gate fails; ordinary focused R2 may omit map |
| S7-17 | Map hash/revision mismatch | #37 | Receipt/readiness stale |
| S7-18 | Apply receipt wrong subject/revision | integrity | Reject, node unchanged |
| S7-19 | Same receipt retry | idempotency | Unchanged, no duplicate event |
| S7-20 | Competing apply CAS | concurrency | One wins, one conflict |
| S7-21 | Invalidate current shaping | invalidation | Pointer clear, history preserved, readiness stale |

### Branch dispositions

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-22 | Critical branch missing disposition | #34 | Not ready |
| S7-23 | Blocking branch | #34 | Receipt valid observation, readiness fail |
| S7-24 | Delegated branch on locked Ticket | #34 | Reject/not ready |
| S7-25 | Delegated freedom ID missing | #34 | `shaping_delegation_exceeds_freedom` |
| S7-26 | Valid delegated reversible choice | #34/#36 | Pass branch gate |
| S7-27 | Deferred missing reason | #34 | Reject |
| S7-28 | Deferred missing owner/target | #34 | Reject |
| S7-29 | Deferred missing trigger/linked work | #34 | Reject |
| S7-30 | Deferral not explicitly non-blocking | #34 | Not ready |
| S7-31 | Resolved Decision revision changes | #39 | Readiness stale |
| S7-32 | Rejected branch still has active sole-purpose decision work | #39 | Validation finding |

### Fog/destination/reconciliation

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-33 | Bounded non-blocking fog | #38 | Does not block; returned in report |
| S7-34 | Fog lacks bounds/trigger | #38 | Not ready |
| S7-35 | Precise question hidden as fog | #38 | Structural finding/reviewer fail |
| S7-36 | Out-of-scope entry | #38 | Not frontier/fog blocker |
| S7-37 | Graduated fog provenance on decision work | #38/#39 | Historical ID resolves |
| S7-38 | Replacement receipt references unknown old IDs | #39 | Reject |
| S7-39 | Replacement receipt current + old preserved | #39 | New pointer, historical audit intact |

### Documentation/readiness

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-40 | Docs impact unknown | #19 | Not ready |
| S7-41 | Internal refactor docs none + rationale | #19 | Docs family passes |
| S7-42 | Required doc missing/retired/stale | #19 | Not ready |
| S7-43 | Required doc content changes | #19 | Readiness fingerprint/status stale |
| S7-44 | Optional scope doc changes | narrow invalidation | Does not stale unless readiness input included it |
| S7-45 | Unrelated graph mutation | narrow invalidation | Same readiness fingerprint |

### Authority/lifecycle

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-46 | Missing authority policy | authority | Gate unavailable, no implicit superuser |
| S7-47 | Receipt approver lacks R2 grant | authority | Denied |
| S7-48 | Caller lacks transition-ready grant | authority | Transition denied |
| S7-49 | Draft -> shaped valid receipt | lifecycle | Success + gate fingerprint event |
| S7-50 | Shaped -> ready current report | lifecycle | Success + fingerprint remains current after status revision bump |
| S7-51 | Expected fingerprint stale | concurrency | Reject, caller reloads |
| S7-52 | Two ready transitions same revision | concurrency | One success, one CAS conflict |
| S7-53 | Ready status then Decision/docs changes | invalidation | Status retained, report stale, frontier excludes |
| S7-54 | Generic --force attempt | guardrail | Unsupported |
| S7-55 | Shaping apply bumps normal revision only | self-invalidation | Receipt remains current by contract revision |
| S7-56 | Ready transition bumps normal revision only | self-invalidation | Readiness fingerprint remains current |
| S7-57 | Required Decision lacks acceptance receipt | authority | Gate unavailable/not ready |
| S7-58 | QA impact unknown | #42 | Not ready |
| S7-59 | QA none with valid rationale | #42 | QA impact family passes |
| S7-60 | QA required before baseline resolver | #42 | Gate unavailable until Phase 3 |

### Frontiers

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-61 | Same snapshot decision vs execution | #40 | Distinct sets, same graph fingerprint |
| S7-62 | Open unblocked decision-work Ticket | #37/#40 | Decision frontier only |
| S7-63 | Current ready implementation Ticket | #40 | Execution frontier only |
| S7-64 | Shaped but not transitioned Ticket | #40 | Not execution frontier |
| S7-65 | Ready stale Ticket | #40 | Excluded with reason |
| S7-66 | Hard blocker open | #40 | Excluded |
| S7-67 | Soft preference open | #40 | Still eligible, advisory metadata |
| S7-68 | Different destination owner | #37 | Excluded by --for filter |
| S7-69 | No runtime claim resolver | #40 | claim_state not_evaluated, no persisted claim |
| S7-70 | Priority difference | reconciliation boundary | Membership unchanged, deterministic ID order |
| S7-71 | Delete/corrupt cache | #40 | Rebuild equivalent semantics |

### Recovery/consistency

| ID | Scenario | Roadmap | Kỳ vọng |
|---|---|---:|---|
| S7-72 | Crash after contract node before event | recovery | Exactly one event after recovery |
| S7-73 | Crash after shaping pointer before event | recovery | Pointer/event coherent |
| S7-74 | Current evidence v1 template/model mismatch | integrity | Reject drift; no migration or read-side rewrite |
| S7-75 | Manual conflicting edit during recovery | recovery | Stop, preserve intent/evidence |
| S7-76 | Map bytes change during ready evaluation | consistency | `readiness_inputs_changed` |
| S7-77 | Concurrent docs registry mutation/readiness transition | consistency | Coherent before or after snapshot |
| S7-78 | Authority policy changes during ready evaluation | consistency | Abort/reload; old grant not committed |
| S7-79 | Process-level apply/transition races | concurrency | CAS contracts hold |
| S7-80 | JSON CLI output/errors | contract | Stable code/order/non-zero behavior |

Tests phải dùng real temporary Git repositories, actual receipt files/content
hashes và process-level concurrency/failpoints. Không mock string existence cho
Decision/docs/map bindings.

## Definition of Done của slice

- [ ] Current node schema v1, Rust model, bootstrap template, tests and fixtures
  agree; no v2/predecessor/migration path is introduced.
- [ ] Ticket có typed `implementation|decision_work` role; không thêm node kind
  hoặc hierarchy thứ hai.
- [ ] New public Tickets have explicit assessed risk/materialization; explicit
  canonical draft/bootstrap `unassessed` remains non-ready without fabricated
  semantic defaults.
- [ ] Contract revision separates semantic freshness from lifecycle/pointer CAS;
  shaping apply và ready transition do not invalidate their own proof.
- [ ] Implementation contract cover objective/current/target, mode, work
  surface, plan policy, brief hash, anchors, changes, invariants, acceptance,
  scope, freedom, Decision/approach refs, verification profile, expected evidence
  và expected handoff.
- [ ] Decision-work contract cover destination owner, branch/fog provenance,
  gap kind, precise question, expected output/evidence và optional resolution
  target.
- [ ] Current shaping pointer reference immutable receipt ID/hash và optional
  map path/revision/hash; node không embed frontier/branch truth.
- [ ] Current shaping receipt payload v1 schema/typed decoder cover contract revisions,
  destination, branches, dispositions, fog, out-of-scope, resolution pointers,
  approval, reconciliation và remaining uncertainty.
- [ ] Evidence manifest/bootstrap references the completed current shaping
  payload v1 schema; read-only queries never bootstrap or rewrite it.
- [ ] Internal placeholder receipt bytes are regenerated or rejected as drift,
  not preserved through a compatibility decoder.
- [ ] Hard-to-reverse Decision references require immutable acceptance proof and
  authority; node existence alone is not approval.
- [ ] Receipt-first shaping apply uses expected revision, idempotent retry,
  immutable event and crash recovery.
- [ ] R0 concise self-check path does not require map, ADR, plan or unnecessary
  human approval beyond policy.
- [ ] R2 requires persisted map only for typed multi-session/multi-decision/
  resume conditions; R3 always requires destination, exit conditions and
  content-bound persisted map.
- [ ] Critical missing/blocking branch fails readiness.
- [ ] Delegated branch resolves explicit implementation freedom and cannot
  exceed mode/contract.
- [ ] Deferred branch requires reason, owner/target, trigger/linked work and
  explicit non-blocking scope.
- [ ] Fog is typed/bounded/non-blocking and distinct from precise branch,
  deferred work and out-of-scope.
- [ ] Local authority policy is default-deny, typed, fingerprinted, derives
  required grants in kernel and does not infer grant from actor kind/receipt.
- [ ] Minimal QA impact metadata blocks `unknown`; `none` and
  `covered_by_story_close` have structural rules, while required case resolution
  remains unavailable until Phase 3.
- [ ] Readiness composition keeps gate-family statuses and stable reason codes;
  no opaque persisted `is_ready` boolean.
- [ ] Narrow readiness fingerprint avoids unrelated graph/registry invalidation
  and includes exact relevant work/docs/Decision/map/policy hashes.
- [ ] `draft -> shaped` and `shaped -> ready` recompute gates under fence and
  have no `--force`; blocked resume uses audited `blocked -> shaped -> ready`.
- [ ] Ready status may become stale projection; execution frontier excludes it
  without hidden mutation.
- [ ] Decision frontier contains relevant precise decision-work Tickets with
  satisfied hard dependencies, including draft work without requiring a nested
  shaping receipt.
- [ ] Execution frontier contains only current-ready implementation Tickets.
- [ ] Frontier does not persist/claim runtime lease state and reports
  `claim_state=not_evaluated` until Phase 2.
- [ ] Frontier ordering is deterministic membership, not semantic priority
  ranking.
- [ ] Cache delete/corrupt/rebuild does not change readiness/frontier semantics.
- [ ] Scenario #9, #10, #19, #34, #35 và #37–40 have automated fixtures.
- [ ] Process concurrency, crash recovery, TOCTOU content hash and JSON CLI
  contract tests pass.
- [ ] CLI remains thin; graph/docs/evidence/policy ownership boundaries remain
  typed and separate.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --quiet -- -D warnings`
  and `cargo test --all-targets` pass.
- [ ] No conversational shaping, work packet, runner, lease, QA baseline,
  verification/close gate or knowledge retrieval is smuggled into Slice 7.

## Handoff sang Phase 2 — Minimal Shaping + Single-Agent Run

Sau Slice 7, Phase 1 foundations có:

```text
canonical graph/lifecycle
+ evidence receipts
+ docs registry/applicability/retrieval
+ knowledge canonical store
+ typed shaping result
+ contract readiness
+ decision/execution graph frontiers
```

Phase 2 có thể xây mà không đổi identity cơ bản:

### Minimal `pulse-shape`

- ground work/docs/Decisions/code trước khi hỏi;
- one-question-at-a-time dependency walk;
- recommended answer khi có strong default;
- classify gap thành fact/intent/tradeoff/fidelity/prerequisite;
- materialize đúng Ticket/Story/Decision/docs owner;
- record shaping receipt payload v1;
- apply receipt và query readiness.

### Shaping reconciliation

- persist resolution/evidence;
- update canonical pointers/contracts;
- graduate precise fog thành decision-work Ticket;
- reject/cancel/supersede invalidated branches qua existing graph APIs;
- record replacement shaping receipt;
- recompute frontiers/readiness;
- maintain CAS/audit across each mutation.

### Work packet/prompt builder

Use Slice 7 IDs/fingerprints to aggregate:

- implementation contract;
- destination/branch/fog refs;
- parent/Decision context;
- structural blockers;
- required/optional/write-candidate docs;
- ranked section refs/read budget;
- future applicable knowledge;
- verification/QA/doc policy;
- hard stops/escalation conditions.

### Runtime composition

Phase 2 adds:

- lease/claim resolver;
- source/workspace identity;
- `dispatch_authorized=true` only after current readiness + lease/capability
  checks; work with `qa.impact=required` remains undispatchable until Phase 3
  baseline/case resolver is installed;
- Worker ambiguity `decision_request`/re-shape path;
- runner, interruption, resume và handoff.

Phase 2 must not replace branch IDs, fog IDs, destination owner, shaping receipt
identity, readiness profile/fingerprint hoặc frontier work identity merely to
support conversation/runtime.

## Phase 3 follow-up

- Full `pulse-shape` risk-adaptive capability pack and reviewer/eval.
- Machine config/init/doctor policy UX for authority registry.
- Story QA baseline, affected/new case resolution and executor/receipt
  validation for `qa.impact=required`.
- Readiness profile bump/refinement to include full QA baseline/case family.
- Generated freshness/link/profile checks.
- Doctor findings for stale ready, hidden fog, orphan receipt, contract/prose
  drift and unresolved authority.

## Risks và open questions còn lại

1. **No cryptographic identity:** local actor IDs are policy principals, not
   authenticated signatures. This matches local-first control-plane scope but
   must not be marketed as adversarial security.
2. **Node schema size:** machine contract can grow. Keep concise bounded fields
   and content hashes; detailed rationale/plan remains Markdown/Decision.
3. **Contract/prose duplication:** metadata and `ticket.md` can drift. Exact
   brief hash catches byte change, but semantic alignment remains reviewer
   responsibility.
4. **Decision acceptance proof:** proposal adds minimal immutable acceptance
   receipt. Need lock exact payload/grant semantics without turning Slice 7 into
   a full Decision lifecycle system.
5. **Dirty source:** code/config/data claims require clean commit because dirty
   snapshot canonicalization remains unresolved; content-only shaping may be
   source-optional under policy. Do not use unstable `git diff | sha256`.
6. **Cross-plane lock:** repository-scoped lock ensures coherence but may make
   readiness queries/mutations serialize with docs/knowledge writes. Benchmark
   before finer locks.
7. **TOCTOU:** arbitrary editors ignore Pulse lock. Double-hash before commit
   reduces but cannot provide OS-wide transactional edit isolation.
8. **Fog semantic detection:** kernel cannot fully know a sentence hides a
    precise blocker. Receipt reviewer/eval is essential; structural schema is
    necessary but insufficient.
9. **Decision frontier relevance:** branch IDs/current shaping receipt provide
    deterministic route, but replacement receipt may invalidate work. Validation
    needs clear historical/current semantics.
10. **Frontier scale:** detailed readiness per Ticket may be expensive with
    large docs corpora. Use required-doc hashes and compact summaries; benchmark
    before persistent cache or incremental dependency index.
11. **Materialization classification:** canonical draft/bootstrap Tickets may
    remain explicitly `unassessed` and block readiness until reviewed. Public
    create requires assessed values; no migration or bulk inference is needed.
12. **Parent approach binding:** exact Story approach path convention may vary.
    Contract should use typed content refs rather than hard-code only
    `approach.md`.
13. **Profile evolution:** readiness profile/version and transition event must
    preserve why a Ticket was considered ready under an older profile.

## Không quyết định trong slice này

Slice 7 không chốt conversational prompt wording, grilling model quality,
semantic ambiguity classifier, priority score, worker authority transport,
lease TTL, dirty snapshot algorithm, QA baseline schema, verification profile
execution, close gate, work packet byte budget, knowledge applicability ranking
hoặc orchestration scheduling.

Nó chỉ chốt typed shaping/readiness/frontier foundation để Phase 2 có thể hỏi,
reconcile và dispatch dựa trên repository-local contracts/evidence thay vì chat
memory, free-text booleans hoặc raw filesystem scans.
