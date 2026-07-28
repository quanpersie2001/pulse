# Phase 2 — Slice 2: Atomic Reservation + Workspace Binding

> Trạng thái: **implemented and verified** for Phase 2 Slice 2. Implementation
> landed across commits `0c9a888`, `57ec28f`, `f66d103`, `9abb940`, `dd704da`,
> `e1f86e6`, `1dc2473`, `7108e9e`, `f8a747b`, `367dfed` and verification
> hardening commit `e6c6402`; supporting fix/verification commits are listed in
> the completion evidence below. This remains a pre-Core-v1 current baseline,
> not a released compatibility contract.
> Tiền đề:
> [`phase2-slice1-work-packet-dispatch-foundation.md`](phase2-slice1-work-packet-dispatch-foundation.md)
> is implemented and verified through commit `6d3076b`. Slice 1 owns the
> read-only `WorkPacketV1` preview packet. This Slice 2 implementation does not
> mutate `WorkPacketV1` semantics.
> Sở hữu: implemented Slice 2 behavior for the second Phase 2 slice:
> atomic assignment reservation, runtime lease record, workspace
> binding record, concrete capability match, `PreparedAssignmentV1`, gated
> `ready -> active`, release/recovery of prepared-but-not-runnable assignments,
> and claim-state projection from runtime.
> Tham chiếu normative:
> [`PULSE_REBOOT.md`](../PULSE_REBOOT.md),
> [`02-work-graph.md`](../pulse-reboot/02-work-graph.md),
> [`04-runtime-harness.md`](../pulse-reboot/04-runtime-harness.md),
> [`05-cross-agent-coordination.md`](../pulse-reboot/05-cross-agent-coordination.md),
> [`06-priority-reconciliation.md`](../pulse-reboot/06-priority-reconciliation.md),
> [`07-verification-ratchet.md`](../pulse-reboot/07-verification-ratchet.md),
> [`08-implementation-roadmap.md`](../pulse-reboot/08-implementation-roadmap.md),
> [`09-decisions-and-dod.md`](../pulse-reboot/09-decisions-and-dod.md).

## Executive summary

Slice 1 made `pulse work packet <ticket-id> --json` a deterministic, bounded,
read-only preview. Every successful preview packet says:

```json
{
  "dispatch": {
    "reservation_candidate": true,
    "dispatch_authorized": false,
    "authorization_status": "not_reserved"
  },
  "workspace": {"binding_status": "not_allocated"},
  "capabilities": {"evaluation_status": "not_evaluated"}
}
```

Slice 2 adds the first mutation that turns a current preview candidate into an
exclusive prepared assignment:

```bash
pulse --repo-root <repo> work claim <ticket-id> \
  --actor <principal> \
  --assignee <principal> \
  --capabilities <capability-file> \
  --ttl-seconds <n> \
  --json
```

On success it returns a new wrapper, **`PreparedAssignmentV1`**, that references
and embeds the revalidated preview packet without changing its semantics:

```text
WorkPacketV1 preview
  + atomically revalidate packet preconditions
  + acquire exclusive runtime lease for a concrete assignee
  + materialize/adopt a bound workspace
  + match concrete capability inventory
  + transition Ticket ready -> active under the same repository fence
  + record assignment-prepared event
  -> PreparedAssignmentV1(dispatch_authorized=true)
```

The wrapper is the first artifact allowed to say `dispatch_authorized=true`.
`WorkPacketV1` remains a preview with `dispatch_authorized=false` even when it is
nested inside `PreparedAssignmentV1`.

This slice still does **not** start Codex or any Worker, send prompts, manage
acknowledgement/mailbox delivery, stream events, create handoff receipts, run
verification, run QA, close work, or claim Phase 2 complete. Those remain later
Phase 2 slices.

---

## Baseline from Slice 1 and current seams

Implemented Slice 1 provides:

- public `pulse::work_packet::WorkPacketV1` DTO/schema/fingerprint;
- `JsonGraphStore::work_packet(&self, id: &str) -> PulseResult<WorkPacketV1>`;
- `src/kernel/packet.rs` coherent two-fence read algorithm;
- `src/source.rs::packet_base_snapshot` and `revalidate_packet_base` for exact
  clean Git `HEAD` binding;
- `work packet` CLI rendering in `src/cli/work.rs`;
- packet output that contains workspace requirement, lease requirement and
  capability requirements but does not satisfy them.

Current implementation seams to reuse, not bypass:

- repository fence is `storage::WriteGuard` over
  `.pulse/runtime/locks/workgraph.lock`;
- recoverable canonical transactions live in `src/storage/transaction.rs` and
  commit canonical target bytes before immutable event bytes, then clean intent;
- graph mutations go through `JsonGraphStore::commit_mutation`, which prepares a
  transaction intent, writes canonical JSON, writes event, then completes;
- lifecycle transition gates currently install only `draft -> shaped` and
  `shaped -> ready`; `ready -> active` is not installed yet;
- graph pure read layers must not import docs/source/runtime;
- `source.rs` owns Git/source binding; do not duplicate packet/source Git logic;
- CLI handlers are thin and must not own assignment business logic.

Slice 2 adds runtime coordination modules around these seams. It must not store
lease/workspace/assignment state on graph nodes or edges, and it must not turn
claim state into canonical graph truth.

---

## Goals

Implement enough assignment preparation that a caller can:

1. claim exactly one current ready implementation Ticket for one concrete
   assignee principal;
2. prove no other live exclusive lease exists for that Ticket;
3. revalidate the exact Slice 1 packet preconditions under a repository fence;
4. evaluate a concrete capability inventory against packet-required
   capabilities;
5. materialize or bind a workspace whose repository identity and base commit
   match the packet;
6. atomically persist the runtime lease record, workspace record and
   `ready -> active` lifecycle mutation ordering so recovery is deterministic;
7. receive a `PreparedAssignmentV1` wrapper that sets `dispatch_authorized=true`
   only after lease, workspace, capability, source and lifecycle gates pass;
8. release/recover stale prepared assignments without corrupting graph or
   worktree state;
9. show runtime claim state in execution frontier as a derived projection;
10. verify duplicate/concurrent claims cannot both succeed.

---

## Non-goals

Slice 2 does not implement:

- Codex task/thread create/resume/send/wait/interrupt/archive;
- Agent Registry, native thread identity mapping or presence heartbeat;
- typed mailbox, assignment delivery receipt or acknowledgement states beyond a
  local prepared lease record;
- `pulse run`, prompt builder or prompt transport;
- Worker handoff, verification runner, review dispatch, QA dispatch or close
  gate;
- `active -> verifying`, `verifying -> done|rework|blocked` proof transitions;
- dirty source snapshot canonicalization;
- enforced path ACL/sandbox beyond bound workspace identity and existing
  repository-relative validation;
- multi-worker orchestration, peer-agent dispatch batches or conflict advisory;
- knowledge injection or learning retrieval;
- release/merge/deploy authority;
- changing the `WorkPacketV1` schema/profile/meaning.

If implementation discovers a need for runner/acknowledgement/proof semantics,
that should become a later Slice 2+ proposal rather than being folded into this
slice.

---

## Key decisions for this proposal

### P2S2-D1 — `PreparedAssignmentV1` is a wrapper, not WorkPacketV1 v2

`PreparedAssignmentV1` references the exact preview packet and repeats only the
runtime binding data needed to authorize dispatch. It may embed the full
`WorkPacketV1` for caller convenience, but it must not mutate preview fields:

- nested packet keeps `schema_version=1`;
- nested packet keeps `profile="phase2_work_packet_preview_v1"`;
- nested packet keeps `code="reservation_candidate"`;
- nested packet keeps `dispatch.reservation_candidate=true`;
- nested packet keeps `dispatch.dispatch_authorized=false`;
- nested packet keeps `authorization_status="not_reserved"`;
- nested packet keeps `workspace.binding_status="not_allocated"`;
- nested packet keeps `workspace.workspace_id=null`;
- nested packet keeps `capabilities.evaluation_status="not_evaluated"`;
- nested packet keeps `capabilities.inventory_identity=null`.

`PreparedAssignmentV1.dispatch.dispatch_authorized=true` is the authorization
claim. This prevents callers and tests from treating old preview packet bytes as
a bearer token.

The nested packet's `packet_fingerprint` is recomputed by the claim pipeline and
must equal the bytes embedded in the prepared assignment. Claim must not load an
old packet JSON file supplied by the caller, and no `--packet` argument is added
in Slice 2.

### P2S2-D2 — Assignment state is runtime coordination, not graph state

Lease/workspace/assignment records live under `.pulse/runtime/assignment/` and
are gitignored/rebuildable local coordination state. The only canonical graph
mutation in this slice is the lifecycle transition `ready -> active` after a
lease/workspace/capability assignment is prepared.

Do not add `lease_id`, `workspace_id`, `assignee`, `claim_state` or
`prepared_assignment` fields to node schema. Frontier claim state is a runtime
join over graph projections + lease store.

Because pure execution frontier currently lists ready executable candidates and
claim moves a Ticket to `active`, Slice 2 must not force `graph::read` to know
about runtime leases just to display claimed work. The kernel/CLI composition
layer owns any enriched frontier view:

- the existing `graph::read` execution frontier remains the graph/readiness-only
  source for available `ready` implementation candidates;
- kernel/CLI may wrap that pure report in an enriched execution-frontier DTO that
  adds `claim_state` for ready rows and a separate `active_assignments` section
  for prepared assignments on `active` Tickets;
- live prepared assignments for `active` Tickets are loaded from runtime lease
  records and joined only in that wrapper/section;
- runtime-only rows must be labeled as projection data, never as executable
  ready candidates;
- if an `active` Ticket has no matching live/tombstoned assignment record, the
  projection reports a stale/ambiguous claim-state finding rather than silently
  treating it as available work;
- the CLI must keep the stable pure frontier semantics available to tests and
  internal callers; any public JSON extension uses additive top-level fields or
  nullable fields, not a change to graph node/read DTOs.

### P2S2-D3 — `ready -> active` is gated by prepared assignment

`ready` means current executable contract. It does not mean assigned.

This slice installs one new lifecycle gate:

```text
ready -> active requires phase2_prepared_assignment_v1
```

The gate passes only for the claim pipeline's in-memory prepared-assignment
context. The public/generic transition path cannot accept a user-supplied JSON
blob as proof, because that would turn a stale local artifact into an authority
bypass. The claim operation internally creates and validates a live
`PreparedAssignmentV1` whose subject, ticket revision, readiness fingerprint,
packet fingerprint, lease, workspace, capability match, repository identity and
source base all match current state.

No `--force` bypass is introduced.

### P2S2-D4 — Claim is one kernel mutation pipeline

The public claim command performs the whole preparation pipeline. It is not a
composition of user-visible `work packet`, manual worktree create and manual
transition commands.

Why:

- the kernel must close TOCTOU between packet preview and lease commit;
- duplicate claim prevention requires the lease record and lifecycle transition
  to be ordered under the same repository fence;
- callers should not need to guess transaction choreography.

A lower-level internal API may be split into functions, but the public command
must remain an atomic claim/prepare operation.

### P2S2-D5 — Specific assignee is required, but full Agent Registry is deferred

Slice 2 requires a concrete assignee principal string, parsed with the existing
local actor/principal vocabulary where possible:

```text
human:<id>
agent:<id>
tool:<id>
```

The assignee is the lease owner and the future runner identity. Slice 2 does not
validate native Codex thread existence, presence or acknowledgement. Those are
Phase 5/runner concerns.

### P2S2-D6 — Capability inventory is explicit local input

Because Agent Registry is not installed, public claim requires a capability
inventory file. Tests may use named built-in local inventories only through
internal helpers or fixture factories, not as a documented CLI shortcut. The
inventory identity is hashed into the prepared assignment.

The match is exact over required packet capability strings. Extra capabilities
are allowed and reported. Missing required capabilities reject the claim.

Authority grants and runtime capabilities remain separate planes:

- capability inventory says the assignee/tool can perform operations;
- authority policy says whether a principal may perform Pulse-controlled
  business mutations.

Slice 2 checks authority only for the actor that invokes a Pulse mutation:
`work.assignment.prepare` for claim and `work.assignment.release` for release or
mutating recovery. The assignee's capability inventory is never interpreted as a
policy grant, and the actor's policy grant is never interpreted as proof that the
assignee runtime can edit source, create worktrees or run tests.

### P2S2-D7 — Workspace binding is runtime-owned and source-exact

Slice 1 mapped risk to required workspace strategy:

- low: `in_place_allowed`;
- medium/high/critical: `isolated_worktree_required`.

Slice 2 may choose isolated worktree even when in-place is allowed. It may not
downgrade isolated-required work to in-place.

Every workspace record binds:

- workspace ID;
- mode (`in_place` or `isolated_worktree`);
- canonical path;
- repository ID;
- base commit;
- owning lease ID;
- cleanliness/currentness at preparation;
- lifecycle state.

The workspace is an execution location, not a proof that any Worker has started.

### P2S2-D8 — Worktree creation happens before durable lease commit, then is adopted

Git worktree creation cannot participate in the canonical JSON transaction
primitive. The safe ordering is:

1. hold repository fence;
2. revalidate packet/source/no live lease;
3. create isolated worktree in a deterministic pending path when required;
4. validate worktree base/source identity;
5. prepare lease/workspace/assignment records and lifecycle mutation;
6. commit runtime records + graph transition + event;
7. make the workspace visible only through the committed workspace record whose
   first durable state is `bound`.

Managed isolated worktrees use `git worktree add --detach <path> <base-commit>`.
No local branch/ref is created in Slice 2; branch creation is deferred to a
future human-facing workspace UX if needed. The deterministic path is the only
pre-commit ownership hint, and it is not a live lease until the workspace record
commits.

If steps 1-4 fail, remove the pending worktree and return no lease. If step 6
crashes after files are partially written, recovery uses the transaction intent
under `.pulse/runtime/transactions` to complete or roll back deterministically;
orphan worktree directories that have no committed workspace record are reported
as pending-orphan cleanup candidates.

### P2S2-D9 — Runtime transaction uses the storage multi-target intent

A successful claim changes multiple local files:

- lease record;
- workspace record;
- prepared assignment record;
- Ticket node status/revision;
- semantic event file.

This must be one recoverable transaction family, not several independent writes.
Use the existing domain-neutral storage transaction model where possible:
`MultiTargetTransactionIntent` stores sorted target paths, before/after states,
after bytes and one event payload under `.pulse/runtime/transactions`. Extend it
only if implementation proves a missing domain-neutral capability; do not invent
per-domain ad-hoc recovery.

The commit/recovery visibility rule is storage-owned, not assignment-owned:
canonical targets are written in lexicographic path order as recorded in the
intent, then the event is written create-new, then the intent is removed. Because
recovery recognizes prefix-after states in that sorted order, assignment code
must not rely on a semantic write order such as "lease before node" for
correctness. No prepared assignment is externally dispatch-authorized unless the
transaction event and all target after-states are durable or recovery can roll
them forward from the intent.

### P2S2-D10 — Expired prepared assignment requeues to `ready` only through release/recovery

If a claim succeeds, the Ticket becomes `active`. If the assignee never starts a
runner, a later release/recover operation may revoke the lease and transition the
Ticket back to `ready` only if:

- no handoff/run/verification state exists for the assignment;
- the node is still at the active revision created by the claim;
- source/workspace state is either clean at base or preserved for manual review;
- release actor has policy authority.

This slice may implement release of `prepared` leases whose subject Ticket is
`active` because of the claim and has no run state. It must not introduce a live
lease state named `active`, and it must not implement runner-aware cancellation
or evidence-preserving handoff cleanup.

### P2S2-D11 — Events describe assignment preparation, not Agent execution

Claim emits exactly one semantic event, `work.assignment.prepared`, using the
shared event envelope. Its payload includes the lifecycle transition data that
existing graph history needs (`from=ready`, `to=active`, revisions, graph
fingerprints and gate coverage). It must not also emit a separate
`work.node.transitioned` event for the same status change, and it must not emit
`run.started`, `assignment.delivered`, `assignment.acknowledged`,
`ticket.handoff_submitted`, `verification.passed` or any runner/proof event.

### P2S2-D12 — Stale/ghost leases are recoverable but conservative

A lease whose TTL expires before acknowledgement/runner state can be revoked by
`work release` or `work leases recover --actor <principal>`. `recover` applies
only safe deterministic fixes by default, matching existing graph transaction
recovery behavior: it rolls forward/cleans prepared intents, expires a no-run
lease, requeues `active -> ready` when the active revision exactly matches the
claim, and marks clean managed worktrees for cleanup. A lease or orphan
workspace with dirty, unknown or externally modified state is reported as
`stale_needs_operator` and blocks duplicate claim until an operator resolves it.

Correctness beats convenience: no ghost ownership, no duplicate exclusive
Workers, and no source deletion without clear ownership.

---

## Public CLI contract

### Claim / prepare

```bash
pulse --repo-root <repo> work claim <ticket-id> \
  --actor <principal> \
  --assignee <principal> \
  --capabilities <capability-file> \
  [--ttl-seconds <seconds>] \
  [--workspace-mode auto|in-place|isolated-worktree] \
  [--json]
```

The canonical command is `work claim`. Slice 2 does not add a public or
documented `work prepare` command. A hidden developer alias may exist only if it
is mechanically routed to the same handler and excluded from public docs,
acceptance tests and stable contract examples. Output, events, error codes,
public docs and tests use `claim` and `prepared_assignment`; any hidden alias
must not introduce distinct JSON fields, event types, error codes or help text
that looks like a separate operation.

Arguments:

- `<ticket-id>`: required implementation Ticket ID.
- `--actor`: required authorized principal performing the Pulse mutation. The
  actor may differ from the assignee and is checked for `work.assignment.prepare`.
- `--assignee`: required local principal string that will own the lease.
- `--capabilities`: required path to inventory JSON, repository-relative or
  absolute; file content is hashed into `inventory_identity`.
- `--ttl-seconds`: optional; default 1800; min 60; max 86400.
- `--workspace-mode`: optional; default `auto`.
  - `auto`: isolated when packet requires isolated, otherwise in-place;
  - `in-place`: only valid when packet says `in_place_allowed`;
  - `isolated-worktree`: always valid if Git worktree creation succeeds.
- no `--force`;
- no `--allow-dirty`;
- no `--skip-capabilities`;
- no runner/agent runtime flags.

Human output example:

```text
TK-031 prepared for agent:codex-local
lease: lease_01J...
workspace: wt_TK-031_01J... (isolated worktree)
source: 0123456789abcdef0123456789abcdef01234567
capabilities: all required matched
status: active
prepared assignment: pa_01J...
dispatch authorized: yes (runner not started)
```

### Release / revoke prepared lease

```bash
pulse --repo-root <repo> work release <ticket-id> \
  --lease <lease-id> \
  --expected-revision <active-revision> \
  --reason <reason> \
  --actor <principal> \
  [--json]
```

Scope in this slice:

- releases prepared assignments before runner/acknowledgement state exists;
- removes/revokes lease record;
- marks workspace `released` or `stale_needs_operator`;
- transitions `active -> ready` only for a prepared/no-run assignment that still
  matches the expected active revision;
- emits `work.assignment.released` event.

Do not implement general cancellation of running work.

### Lease listing / recovery projection

```bash
pulse --repo-root <repo> work leases [--ticket <ticket-id>] [--json]
pulse --repo-root <repo> work leases recover --actor <principal> [--json]
```

`work leases` is read-only and joins runtime leases/workspaces with graph nodes.
It must perform preserve/no-bootstrap enrollment validation before reading
runtime paths and must not create `.pulse/runtime` if no runtime directory
exists. `recover` runs safe deterministic runtime assignment recovery under the
repository fence and reports completed, expired/requeued, released,
stale-needs-operator, orphaned-workspace and invalid records. It requires an
actor because safe expiry/requeue writes graph/runtime state and emits events.
Ambiguous records are never fixed silently.

### Frontier claim-state join

Existing execution frontier output should continue to derive executable Tickets
from graph/readiness. Slice 2 extends claim-state composition by joining runtime
leases:

```json
{
  "claim_state": "not_claimed|prepared|stale|blocked_by_live_lease|ambiguous",
  "lease_id": "lease_...",
  "prepared_assignment_id": "pa_...",
  "assignee": "agent:codex-local",
  "expires_at": "..."
}
```

This remains a projection. It is not persisted into graph nodes. JSON shape is
stable for this slice: unavailable optional fields are `null`, not omitted, so
callers can distinguish `not_claimed` from `prepared`/`stale`/`ambiguous`
without parsing human text. `prepared` is the only live Slice 2 assignment state;
`active` remains the graph lifecycle status of the subject Ticket, not a
frontier claim-state value.

---

## Capability inventory contract

Minimum input JSON:

```json
{
  "schema_version": 1,
  "principal": "agent:codex-local",
  "inventory_id": "local-codex-default",
  "capabilities": [
    "repository.inspect",
    "source.read",
    "source.write",
    "test.run",
    "workspace.worktree"
  ]
}
```

Rules:

- `schema_version` must be 1.
- `principal` must exactly equal `--assignee` unless omitted; mismatch rejects.
- `inventory_identity` is `sha256:` over canonical JSON bytes of the inventory.
- capability strings are sorted/deduped for matching;
- unknown extra strings are retained under `extra`, not rejected;
- every `packet.capabilities.required[]` must be present;
- `workspace.worktree` is required whenever chosen workspace mode is isolated,
  even if the preview packet only allowed in-place;
- capability matching is performed for the assignee inventory only after the
  claim actor passes policy authorization; matching must not read policy grants;
- no capability implies authority grant.

Capability match report:

```json
{
  "inventory_identity": "sha256:...",
  "principal": "agent:codex-local",
  "status": "matched",
  "required": ["repository.inspect", "source.read", "source.write"],
  "matched": ["repository.inspect", "source.read", "source.write"],
  "missing": [],
  "extra": ["test.run"],
  "reason_codes": []
}
```

---

## Runtime filesystem layout

All paths below are target-repository runtime coordination state and should be
covered by the target repo gitignore/runtime policy. Slice 2 commands must
validate that the target repository is already enrolled before creating any of
these directories; a non-enrolled path must fail without bootstrapping runtime
state:

```text
.pulse/runtime/
  locks/
    workgraph.lock
  assignment/
    leases/
      lease_01J....json
    workspaces/
      wt_TK-031_01J....json
    prepared/
      pa_01J....json
    # no assignment-specific transaction intents; shared intents live in
    # .pulse/runtime/transactions/txn_01J....json
    tombstones/
      lease_01J....json    # final non-live lease summary retained locally
  workspaces/
    wt_TK-031_01J.../       # default isolated worktree parent, gitignored
```

Record files are local runtime state, not canonical work graph. They are still
written with canonical JSON, hashes and recoverable transactions so local
restart can reconstruct assignment ownership. Release/recovery writes a
non-live tombstone summary for the lease ID and updates/removes the live lease
record in the same recovery/release transaction; immutable events remain the
audit truth, while tombstones are a local idempotency/projection aid.

ID prefixes:

- `lease_` for lease records;
- `wt_` for workspace records;
- `pa_` for prepared assignment records.

IDs should use existing monotonic/random ID utility style if available; they
must be unique and filesystem-safe. Do not use Ticket ID alone as lease ID.

---

## Assignment lease record

```json
{
  "schema_version": 1,
  "lease_id": "lease_01J...",
  "kind": "implementation_assignment",
  "subject": {
    "kind": "ticket",
    "id": "TK-031",
    "revision": 8,
    "contract_revision": 4,
    "status_at_claim": "ready"
  },
  "assignee": {
    "principal": "agent:codex-local"
  },
  "issued_by": "human:quannv",
  "issued_at": "2026-07-28T10:00:00Z",
  "expires_at": "2026-07-28T10:30:00Z",
  "ttl_seconds": 1800,
  "state": "prepared",
  "packet_fingerprint": "sha256:...",
  "readiness_fingerprint": "sha256:...",
  "workspace_id": "wt_TK-031_01J...",
  "prepared_assignment_id": "pa_01J...",
  "capability_inventory_identity": "sha256:...",
  "source": {
    "repository_id": "repo_...",
    "base_commit": "0123456789abcdef0123456789abcdef01234567"
  }
}
```

State vocabulary for live lease files in this slice:

```text
prepared
```

Terminal outcomes are represented by removing or rewriting the live lease in the
same transaction that writes a tombstone summary with `state="released"`,
`state="expired"` or `state="stale_needs_operator"`. A terminal tombstone is not
a live exclusive lease.

The wider orchestration state machine from `05-cross-agent-coordination.md`
(`delivered`, `acknowledged`, `handed_off`) is intentionally not installed yet.
Fields may be reserved in schema only if they are null/typed as `not_installed`;
do not fabricate runner states.

Live exclusive lease predicate:

- kind is `implementation_assignment`;
- subject ID matches;
- live lease file exists and no terminal tombstone exists for that lease ID;
- state is `prepared`;
- `expires_at > now`;
- node is still at the active revision produced by the prepared assignment or,
  during transaction preparation, at the ready revision being claimed.

Expired leases are not live, but they block duplicate claim until recovery or
release classifies them and writes the terminal tombstone/requeue transaction.
Claim may run safe recovery first under the same fence, but it must not silently
delete ambiguous workspace state.

---

## Workspace record

```json
{
  "schema_version": 1,
  "workspace_id": "wt_TK-031_01J...",
  "lease_id": "lease_01J...",
  "prepared_assignment_id": "pa_01J...",
  "subject": {
    "kind": "ticket",
    "id": "TK-031",
    "revision": 8
  },
  "mode": "isolated_worktree",
  "path": ".pulse/runtime/workspaces/wt_TK-031_01J...",
  "repository_id": "repo_...",
  "base_commit": "0123456789abcdef0123456789abcdef01234567",
  "head_commit_at_bind": "0123456789abcdef0123456789abcdef01234567",
  "cleanliness_at_bind": "clean",
  "state": "bound",
  "created_at": "2026-07-28T10:00:00Z",
  "released_at": null,
  "cleanup": {
    "policy": "safe_remove_if_clean_at_base",
    "status": "not_requested"
  }
}
```

Modes:

- `in_place`: path is repository root, no new worktree created;
- `isolated_worktree`: path is under `.pulse/runtime/workspaces/` by default.

Isolated worktree creation rules:

- source repository must be Git and match packet source base;
- worktree is created at exact base commit with `git worktree add --detach`;
- no Slice 2 managed branch/ref is created for the worktree;
- worktree path must be within runtime workspace root unless a future explicit
  config permits external paths;
- path must not already exist unless recovery is adopting a matching pending
  worktree;
- after creation, `source::packet_base_snapshot` or a workspace-specific sibling
  validates full commit, repository identity, cleanliness and no operation in
  progress;
- failed creation removes the pending directory when safe.

In-place binding rules:

- only allowed when packet workspace requirement is `in_place_allowed` and user
  did not request isolated;
- revalidate root source remains clean/current immediately before transaction;
- workspace path is the repo root but record still binds lease/source identity;
- release must not delete repo root.

---

## PreparedAssignmentV1 JSON contract

Top-level shape:

```json
{
  "schema_version": 1,
  "profile": "phase2_prepared_assignment_v1",
  "code": "prepared_assignment",
  "prepared_assignment_id": "pa_01J...",
  "subject": {
    "id": "TK-031",
    "kind": "ticket",
    "revision_before": 8,
    "revision_after": 9,
    "contract_revision": 4,
    "status_before": "ready",
    "status_after": "active"
  },
  "packet": {},
  "packet_fingerprint": "sha256:...",
  "revalidated_snapshot": {},
  "lease": {},
  "workspace": {},
  "capability_match": {},
  "lifecycle": {},
  "dispatch": {},
  "transaction": {},
  "prepared_assignment_fingerprint": "sha256:...",
  "reason_codes": []
}
```

`packet` is the exact `WorkPacketV1` returned by the internal Slice 1 builder in
this claim attempt, after revalidation. Its preview semantics remain unchanged.

### `revalidated_snapshot`

```json
{
  "graph_fingerprint": "sha256:...",
  "readiness_profile": "phase1_contract_readiness_v1",
  "readiness_fingerprint": "sha256:...",
  "authority_policy_fingerprint": "sha256:...",
  "docs_registry_fingerprint": "sha256:...",
  "docs_index_fingerprint": "sha256:...",
  "source_commit": "0123456789abcdef0123456789abcdef01234567",
  "source_cleanliness": "clean",
  "repository_id": "repo_..."
}
```

This must match the packet snapshot/preconditions. If any value changes during
claim, return a typed stale/snapshot/source error and do not commit lease.

### `lease`

```json
{
  "lease_id": "lease_01J...",
  "state": "prepared",
  "assignee": "agent:codex-local",
  "issued_by": "human:quannv",
  "issued_at": "2026-07-28T10:00:00Z",
  "expires_at": "2026-07-28T10:30:00Z",
  "ttl_seconds": 1800,
  "exclusive": true
}
```

### `workspace`

```json
{
  "workspace_id": "wt_TK-031_01J...",
  "binding_status": "bound",
  "mode": "isolated_worktree",
  "path": ".pulse/runtime/workspaces/wt_TK-031_01J...",
  "repository_id": "repo_...",
  "base_commit": "0123456789abcdef0123456789abcdef01234567",
  "cleanliness": "clean",
  "owner_lease_id": "lease_01J..."
}
```

### `capability_match`

Uses the report described above. `status` must be `matched` for claim success.

### `lifecycle`

```json
{
  "transition": "ready_to_active",
  "gate_profile": "phase2_prepared_assignment_v1",
  "gate_status": "passed",
  "expected_revision": 8,
  "new_revision": 9,
  "event_id": "evt_01J..."
}
```

### `dispatch`

```json
{
  "dispatch_authorized": true,
  "authorization_status": "prepared_assignment",
  "runner_status": "not_started",
  "gate_families": [
    {"family": "packet_revalidation", "status": "passed", "reason_codes": []},
    {"family": "lease", "status": "passed", "reason_codes": []},
    {"family": "workspace_binding", "status": "passed", "reason_codes": []},
    {"family": "capability_match", "status": "passed", "reason_codes": []},
    {"family": "lifecycle", "status": "passed", "reason_codes": []},
    {"family": "runner", "status": "not_installed", "reason_codes": ["runner_not_started_by_slice2"]},
    {"family": "handoff", "status": "not_installed", "reason_codes": ["handoff_gate_not_installed"]},
    {"family": "verification", "status": "not_installed", "reason_codes": ["verification_runner_not_installed"]}
  ]
}
```

`dispatch_authorized=true` means only: this assignment is prepared and may be
handed to a later runner slice. It does not mean a runner has started or that
work is complete.

### `transaction`

```json
{
  "transaction_id": "txn_01J...",
  "committed_targets": [
    ".pulse/runtime/assignment/leases/lease_01J....json",
    ".pulse/runtime/assignment/workspaces/wt_TK-031_01J....json",
    ".pulse/runtime/assignment/prepared/pa_01J....json",
    ".pulse/workgraph/nodes/TK-031.json"
  ],
  "event_path": ".pulse/events/2026-07-28/evt_01J....json",
  "recovery_state": "complete"
}
```

### Fingerprints, schema boundaries and determinism

`PreparedAssignmentV1`, lease records, workspace records, prepared-assignment
records and capability inventory reports are new public DTO/schema contracts for
Slice 2. They are not canonical graph node/edge schema extensions and they are
not a `WorkPacketV1` schema change. Rust DTOs must use `#[serde(deny_unknown_fields)]`
in round-trip tests, and schema fixtures must reject unknown runner/mailbox/proof
fields unless those fields are explicitly present as nullable `not_installed` v1
placeholders.

`prepared_assignment_fingerprint` hashes a projection containing:

- schema/profile;
- prepared assignment ID;
- subject before/after revisions/status;
- packet fingerprint;
- revalidated snapshot;
- lease ID/state/assignee/expiry;
- workspace ID/mode/path/repository/base commit;
- capability inventory identity and match report;
- lifecycle event ID and transition profile;
- dispatch gate statuses.

It excludes itself and any non-semantic rendering fields. Unlike `WorkPacketV1`,
lease issue/expiry times are semantically part of assignment identity and may be
included. If issue/expiry times are included, the same values must appear in the
lease record, prepared assignment record, event payload and returned JSON. No
floats.

A committed prepared-assignment record is local runtime coordination state, but
its bytes are still the schema/fingerprint source for the command response:
`work claim --json` returns the same canonical value that was committed or a
lossless projection with the same `prepared_assignment_fingerprint`.

---

## Lifecycle gate design

Update lifecycle policy:

```text
ready -> active
  supported target: yes
  required reason: no by default, because claim event carries assignment reason
  installed gate: phase2_prepared_assignment_v1
  public transition CLI without claim context: reject with prepared_assignment_required
```

Implementation approach:

- Extend `graph::model::lifecycle` so `Ready -> Active` reports the
  `prepared_assignment` gate family/profile for introspection, but do not let
  the generic transition evaluator treat that as user-passable proof. The code
  change must avoid the current default behavior where a gated transition with no
  installed gate returns `transition_gate_unavailable`; direct public transition
  must instead return `prepared_assignment_required`.
- Keep existing `transition_node_gated_with_context` behavior for normal
  user-facing transitions; direct `ready -> active` must continue to reject with
  `prepared_assignment_required` rather than `transition_gate_unavailable` or a
  generic lease placeholder.
- Add an internal kernel method that evaluates the prepared assignment gate from
  already assembled claim inputs to avoid recomputing stale packet after
  workspace creation. This method owns the node mutation bytes that are included
  in the multi-target assignment transaction; it must not call
  `commit_mutation`, which would create an independent node/event transaction.
- Event payload for status transition must include:
  - lease ID;
  - workspace ID;
  - prepared assignment ID;
  - packet fingerprint;
  - readiness fingerprint;
  - source base commit;
  - capability inventory identity;
  - graph fingerprint before/after;
  - gate profile/status.

Do not expose a generic `work transition TK --to active` path that can pass
without a prepared assignment. If the existing transition CLI remains available,
`ready -> active` rejects with `prepared_assignment_required`; the only accepted
entry point is `work claim`, which commits the runtime records, node update and
assignment event together.

---

## Atomic claim algorithm

### High-level flow

```text
0. Validate enrolled target repository before creating runtime directories or
   acquiring the runtime lock.
1. Acquire repository WriteGuard.
2. Recover graph/runtime prepared transactions.
3. Authorize the claim actor for `work.assignment.prepare`.
4. Reject live exclusive lease for subject.
5. Build fresh WorkPacketV1 using an internal no-deadlock packet builder or an
   extracted builder that can run under the existing fence.
6. Revalidate packet candidate status, source, docs, policy and readiness.
7. Load and hash capability inventory; require full capability match.
8. Select workspace mode from packet requirement + CLI request.
9. Materialize or bind workspace and validate exact source base.
10. Create lease/workspace/prepared-assignment records in memory.
11. Mutate Ticket ready -> active in memory; validate graph.
12. Prepare one multi-target transaction for runtime records + node + event.
13. Commit transaction.
14. Return PreparedAssignmentV1.
```

### Lock ordering and WriteGuard self-deadlock prevention

There is exactly one repository-wide writer lock in this slice:
`.pulse/runtime/locks/workgraph.lock`, acquired through `storage::WriteGuard`.
All claim, release and mutating recovery operations take this lock before
reading or writing runtime assignment records, canonical workgraph files or
semantic events. No second assignment-specific mutex is introduced. If a future
implementation adds narrower advisory locks, they must always be acquired after
`WriteGuard` and released before it; no code may acquire `WriteGuard` while
holding an assignment/workspace lock.

Current `JsonGraphStore::work_packet` acquires and releases the repository fence
internally. Claim already needs to hold the fence through live-lease check,
workspace binding and lifecycle mutation.

Implementation must therefore extract a fence-aware packet builder, for example:

```rust
impl JsonGraphStore {
    pub fn work_packet(&self, id: &str) -> PulseResult<WorkPacketV1>;

    pub(crate) fn work_packet_under_claim_fence(
        &self,
        id: &str,
        claim_ctx: &ClaimRevalidationContext,
    ) -> PulseResult<WorkPacketV1>;
}
```

The extracted builder may still use Slice 1's two-fence docs-cache algorithm for
the public read command. For claim, prefer this sequence:

1. acquire fence and capture packet preconditions;
2. if docs cache refresh is required, release fence, refresh cache-only, then
   reacquire and revalidate as Slice 1 does;
3. after reacquiring, restart claim preflight from recovery + live lease scan +
   source/readiness revalidation, because another process may have claimed or
   changed the Ticket while the fence was released;
4. continue holding the second fence through lease/workspace/lifecycle commit.

Do not call public `work_packet()` while already holding `WriteGuard`.

### Detailed transaction ordering

#### Phase A — preflight under fence

- Validate existing workgraph and repository identity with no bootstrap; this
  must already have happened once before lock acquisition so a non-enrolled
  repository cannot gain `.pulse/runtime` directories as a side effect.
- Authorize the claim actor with the default-deny authority policy before any
  durable runtime assignment record is prepared.
- Run storage transaction recovery once under the fence. Slice 2 assignment
  transactions use the same `.pulse/runtime/transactions` intent directory as
  current graph/docs/evidence mutations unless a later storage refactor moves
  all domains together; do not create a separate recovery root that can order
  inconsistently with graph recovery.
- Load runtime lease index from `.pulse/runtime/assignment/leases`.
- Reject any live exclusive implementation lease for subject.
- Build/revalidate `WorkPacketV1`; require:
  - `code="reservation_candidate"`;
  - nested preview `dispatch.dispatch_authorized=false`;
  - packet source clean/current;
  - packet workspace/capability future gates not evaluated as expected.
- Verify subject is still `ready` at packet revision.

#### Phase B — capability match

- Read capability inventory bytes.
- Canonicalize/hash inventory.
- Match packet required capabilities plus workspace-induced requirement.
- Reject missing capabilities before creating worktree.

#### Phase C — workspace materialization

For `in_place`:

- revalidate repo root source snapshot equals packet source;
- create only a workspace record in memory.

For `isolated_worktree`:

- create a pending directory name under `.pulse/runtime/workspaces`;
- run Git worktree creation at exact packet base commit;
- validate new worktree source state and repository identity;
- if validation fails, remove pending worktree when safe and abort;
- keep workspace record in memory with `state="bound"` only after validation.

Workspace creation occurs before durable lease commit. Until commit succeeds, the
pending worktree is not owned by a live lease and must be cleaned on error.

#### Phase D — prepared records + lifecycle mutation

While still holding the fence:

- re-scan live lease predicate to close race;
- revalidate source root has not changed since packet/workspace materialization;
- for isolated worktree, revalidate worktree still clean at base;
- build lease record;
- build workspace record;
- build prepared assignment record with nested packet;
- load Ticket node bytes and require revision/status match;
- mutate node status `ready -> active`, increment revision/update timestamp;
- validate graph with planned node;
- compute graph fingerprint before/after.

#### Phase E — one recoverable commit

Prepare a single `MultiTargetTransactionIntent` with canonical targets:

- lease record create-new;
- workspace record create-new;
- prepared assignment record create-new;
- Ticket node update from revision N to N+1.

The semantic event is the transaction event payload/path, not a normal target.
The intent contains the event payload/hash and all target after bytes.

Commit/recovery order is the storage transaction order:

1. persist transaction intent under `.pulse/runtime/transactions`;
2. write target files in the intent's sorted path order using temp + atomic
   rename/create-new;
3. write event file create-new;
4. mark intent complete and remove intent.

Recovery rule: if intent exists, compare every target to before/after hashes and
recognize only all-before, prefix-after or all-after according to the sorted
intent target order. If no target is after and no event exists, roll back by
cleaning temps and removing the intent. If some/all targets are after and the
event is absent, roll forward by writing remaining after targets and the event.
If all targets and matching event exist, clean the intent. Any event mismatch,
event-before-all-targets, non-prefix target state or target content outside
before/after is ambiguous and returns `assignment_recovery_failed`; new claims
for affected subjects are blocked until operator repair.

#### Phase F — post-commit return

- Return in-memory `PreparedAssignmentV1` matching committed bytes.
- Do not start a runner.
- Do not send assignment to assignee.
- Do not mark delivered/acknowledged.

---

## Rollback and recovery behavior

### Failure before workspace creation

No runtime records or graph mutation exist. Return typed error.

### Failure during isolated worktree creation

Remove pending worktree if:

- path is under `.pulse/runtime/workspaces`;
- no lease record points to it;
- it is clean or Git reports no user-created changes.

If removal cannot be proven safe, leave directory and write no lease; return
`assignment_workspace_cleanup_needed` with path. `work leases recover` reports it
as orphaned pending workspace.

### Failure after workspace creation but before transaction intent

Same as above: no live lease, clean pending workspace may be removed; ambiguous
workspace is reported for operator cleanup.

### Failure after intent, before all targets

`work leases recover` and claim preflight must run assignment transaction
recovery. Recovery completes or rolls back according to target before/after
hashes. New claims for the same subject are blocked until recovery completes.

### Failure after lease/workspace records but before lifecycle node update

This is an inconsistent prepared assignment because runtime says claimed but
node may still be ready. Recovery completes the planned node update and event
when the sorted-prefix intent state can be rolled forward; it rolls back only
when every target remains at its before state and the event is absent. Ambiguous
cases block.

### Failure after lifecycle node update but before event

Existing transaction recovery pattern writes/validates the event as part of
completion. If event cannot be reconstructed from intent, block with
`assignment_recovery_failed`; do not silently leave active status without audit.

### Expired prepared assignment with no run state

`work release` or `work leases recover --actor <principal>` may, in one
recoverable transaction:

- remove/rewrite the live lease and write a terminal tombstone as
  `expired`/`released`/`stale_needs_operator`;
- transition Ticket `active -> ready` if expected revision and no later work
  state exists;
- update workspace state to `released` or `stale_needs_operator`;
- emit release/recovery event.

### Workspace cleanup after release

- `in_place`: never delete; just mark released.
- isolated clean at base: safe remove if policy requests cleanup, after the
  release/recovery transaction has recorded the terminal state.
- isolated dirty or unknown: mark `stale_needs_operator`, preserve path.

---

## Error codes

All public JSON errors use the existing global error envelope. The top-level
`code` must be one of the stable assignment codes below for assignment-owned
failures. When a lower-layer packet/source/docs/storage error causes rejection,
keep the assignment code that describes the failed phase and include the
lower-layer code as structured `cause_code` if available, otherwise as stable
`cause_code=<code>` text. Do not expose Rust enum/debug names.

### Claim/precondition

| Code | Meaning |
| --- | --- |
| `assignment_subject_not_ready` | Subject is not ready implementation work |
| `assignment_packet_stale` | Packet preconditions changed |
| `assignment_packet_invalid` | Packet violates preview contract |
| `assignment_live_lease_exists` | Live exclusive lease exists |
| `assignment_expired_lease_needs_recovery` | Expired lease needs recovery |
| `assignment_lock_timeout` | Repository fence could not be acquired |
| `assignment_recovery_failed` | Assignment/canonical recovery failed |

### Capability

| Code | Meaning |
| --- | --- |
| `assignment_capability_inventory_missing` | Inventory path absent/unreadable |
| `assignment_capability_inventory_invalid` | Inventory schema invalid |
| `assignment_capability_principal_mismatch` | Inventory principal conflicts |
| `assignment_capability_missing` | Required capability absent |

### Workspace/source

| Code | Meaning |
| --- | --- |
| `assignment_workspace_mode_unsupported` | Unsupported workspace mode |
| `assignment_workspace_worktree_required` | Isolated worktree required |
| `assignment_workspace_create_failed` | Git worktree creation failed |
| `assignment_workspace_source_mismatch` | Workspace source mismatches base |
| `assignment_workspace_dirty` | Bound workspace is not clean at base |
| `assignment_workspace_cleanup_needed` | Pending workspace needs cleanup |
| `assignment_source_changed` | Source root changed during claim |

### Lifecycle/transaction

| Code | Meaning |
| --- | --- |
| `prepared_assignment_required` | Direct `ready -> active` lacks assignment |
| `assignment_lifecycle_gate_failed` | Prepared assignment gate did not pass |
| `assignment_transaction_failed` | Multi-target commit failed |
| `assignment_schema_invalid` | Assignment schema invalid |
| `assignment_fingerprint_failed` | Assignment fingerprint failed |

### Release/recovery

| Code | Meaning |
| --- | --- |
| `assignment_lease_not_found` | Requested lease missing |
| `assignment_lease_not_releasable` | Lease is outside Slice 2 release scope |
| `assignment_release_revision_mismatch` | Active revision mismatch |
| `assignment_workspace_not_safe_to_remove` | Workspace needs operator review |

Lower-layer `work_packet_*` and `source` errors should be preserved as
`cause_code` where the global error envelope supports it, or appended as stable
`cause_code=<code>` message token as Slice 1 specified.

---

## Security and authority boundaries

- Prepared assignment is local coordination state, not cryptographic authority.
- Possession of `PreparedAssignmentV1` does not grant permission to close work,
  change acceptance, merge, deploy or edit approved docs.
- Claim actor must be authorized for a new grant, e.g.
  `work.assignment.prepare`, under existing default-deny policy.
- Release actor must be authorized for `work.assignment.release`; mutating
  recovery uses the same grant unless a later policy proposal introduces a
  narrower `work.assignment.recover` grant.
- Assignee capability inventory does not grant policy authority.
- Actor authority does not prove assignee runtime capability; both gates must
  pass independently before claim succeeds.
- Workspace path validation rejects traversal and symlink escape for managed
  runtime paths.
- Isolated workspace defaults under `.pulse/runtime/workspaces`, not arbitrary
  user path.
- No secrets, prompts or environment variables are stored in lease/workspace
  records.
- Runtime records should be treated as local coordination and may contain paths
  and principal IDs; do not publish them as evidence.

---

## Architecture and module ownership

Proposed new modules:

```text
src/assignment.rs
  # PreparedAssignmentV1, lease/workspace/capability DTOs,
  # schema projections and fingerprints.
src/kernel/assignment.rs
  # Claim/release/recover orchestration under repository fence.
src/kernel/assignment_store.rs
  # Runtime lease/workspace/prepared record IO for Slice 2.
src/workspace.rs
  # Git worktree materialization/binding helpers; source validation remains
  # owned by source.rs.
src/schema/prepared-assignment.schema.json
src/schema/assignment-lease.schema.json
src/schema/assignment-workspace.schema.json
src/schema/capability-inventory.schema.json
src/schema/capability-match.schema.json
```

Do not add a broad `src/runtime/` namespace in this slice unless a maintainer
accepts a separate runtime-module layout decision. Place the store under
`src/kernel/assignment_store.rs` for now, keep DTOs in neutral
`src/assignment.rs`, and leave a narrow move path for future runner/orchestration
code to reuse DTOs without importing kernel.

Ownership constraints:

- `src/assignment.rs` owns value types, normalization, schema-facing enums and
  fingerprint projections only; it may depend on `canonical_json`, `identity`,
  `work_packet` DTOs and error primitives, but not on `graph::store`, docs,
  policy, filesystem or Git;
- `src/kernel/assignment.rs` owns cross-domain composition, authority checks,
  packet revalidation, capability matching invocation, lifecycle gate evaluation
  and mutation ordering;
- `src/kernel/assignment_store.rs` owns filesystem IO for runtime assignment
  records and tombstones; it does not evaluate lifecycle, authority or
  capabilities and it does not call Git;
- `src/workspace.rs` owns worktree commands and workspace path policy; it calls
  `src/source.rs` or source-owned helpers for Git/source validation rather than
  duplicating source snapshot logic;
- `src/source.rs` remains owner of Git source snapshot/currentness functions and
  may grow workspace-specific snapshot helpers behind the existing public owner;
- `src/graph/model/lifecycle.rs` owns transition vocabulary/gate profile only;
  the claim-only proof check lives in `src/kernel/assignment.rs`;
- `src/cli/work.rs` only parses/renders commands and delegates to
  `JsonGraphStore` APIs;
- `graph::read` remains pure and does not import runtime assignment store,
  source, workspace, docs or policy modules;
- frontier claim-state joins happen in kernel/CLI composition after pure
  execution frontier evaluation, not inside `graph::read`;
- node/edge schemas remain free of runtime lease/workspace fields;
- storage transaction changes, if any, stay in `src/storage/transaction.rs` and
  remain domain-neutral; assignment modules must not fork transaction recovery.

Public exports:

```rust
pub mod assignment;
pub mod workspace;
```

`pulse::work_packet` remains the owner of `WorkPacketV1`; Slice 2 must not move,
rename or re-export a mutated packet type from `pulse::assignment`.

Recommended store API:

```rust
impl JsonGraphStore {
    pub fn claim_work(&self, request: ClaimWorkRequest) -> PulseResult<PreparedAssignmentV1>;
    pub fn release_work(&self, request: ReleaseWorkRequest) -> PulseResult<AssignmentReleaseReport>;
    pub fn list_leases(&self, filter: LeaseFilter) -> PulseResult<LeaseListReport>;
    pub fn recover_assignments(&self, actor: ActorRef) -> PulseResult<AssignmentRecoveryReport>;
}
```

`ClaimWorkRequest`, `ReleaseWorkRequest` and mutating recovery requests carry an
actor/principal for authority evaluation. `LeaseFilter` is read-only and carries
no authority grant. `claim_work` returns the committed `PreparedAssignmentV1`
record or a lossless projection with the same fingerprint; it must not return an
uncommitted in-memory-only value.

---

## Testing strategy

Follow the existing integration crate convention. Never run Pulse commands
against this repository root or mutate tracked fixtures in place. Use
`TestRepo::from_fixture` or external `TempDir` repositories. The tracked
`tests/fixtures/target-repos/minimal-service/` template remains read-only input:
claim/release/recover tests must copy it to a temp repo, initialize any runtime
state there, and assert no `.pulse/runtime/assignment` or managed worktree paths
appear in the tracked fixture.

Suggested files:

```text
tests/graph.rs
  #[path = "graph/assignment_model.rs"]
  #[path = "graph/assignment_lifecycle.rs"]

tests/graph/assignment_model.rs
tests/graph/assignment_lifecycle.rs

tests/process.rs
  #[path = "process/assignment_claim_concurrency.rs"]
  #[path = "process/assignment_recovery.rs"]

tests/process/assignment_claim_concurrency.rs
tests/process/assignment_recovery.rs

tests/target_repo.rs
  #[path = "target_repo/assignment_claim_cli.rs"]
  #[path = "target_repo/assignment_workspace.rs"]

tests/target_repo/assignment_claim_cli.rs
tests/target_repo/assignment_workspace.rs
```

Use process crate for subprocess/concurrency/failpoint suites. Extend shared
helpers for capability inventory creation, Git worktree inspection and runtime
assignment path assertions.

---

## Acceptance matrix

### A. Happy path

1. Ready implementation Ticket with current Slice 1 packet can be claimed.
2. Claim returns `PreparedAssignmentV1` schema v1.
3. Nested `WorkPacketV1` remains preview with `dispatch_authorized=false`.
4. Top-level prepared assignment has `dispatch_authorized=true` and
   `runner_status=not_started`.
5. Lease record exists with subject revision, assignee, TTL, packet fingerprint
   and workspace ID.
6. Workspace record exists and binds repository ID/base commit.
7. Capability match report is `matched` with no missing requirements.
8. Ticket transitions `ready -> active` and revision increments exactly once.
9. Event payload contains lease/workspace/prepared assignment IDs and packet
   fingerprint.
10. No runner, mailbox, handoff, verification or QA state is created.

### B. WorkPacketV1 boundary

1. Claim does not change `WorkPacketV1` schema/profile.
2. Claim does not set nested packet workspace ID.
3. Claim does not set nested packet capability evaluation to matched.
4. Claim rejects if internal packet builder returns a non-preview profile.
5. Existing `work packet` command remains read-only and never creates lease or
   active status.

### C. Subject/readiness/lifecycle

1. Missing/non-ticket/decision-work/draft/shaped/blocked subjects reject via
   stable assignment or packet cause codes.
2. Ready but stale readiness rejects and writes no lease/workspace record.
3. Direct `work transition --to active` rejects without prepared assignment.
4. Claim installs and passes `ready -> active` gate only through claim pipeline.
5. Claim against superseded/terminal Ticket rejects.
6. Release of prepared/no-run assignment transitions `active -> ready` only with
   expected active revision.

### D. Lease exclusivity and concurrency

1. Two concurrent claim processes for the same Ticket: exactly one succeeds.
2. Two different Tickets can be claimed without corrupting shared runtime state.
3. Existing live lease blocks new claim.
4. Expired lease blocks new claim until recovery/release classifies it and
   writes a terminal tombstone.
5. Runtime claim state appears in execution frontier projection.
6. Claim state is not persisted in node JSON.

### E. Capability matching

1. Missing inventory file rejects.
2. Invalid inventory JSON/schema rejects.
3. Principal mismatch rejects.
4. Missing required capability rejects and writes no workspace/lease.
5. Extra capabilities are reported but do not fail.
6. Isolated workspace selection requires `workspace.worktree`.
7. Inventory hash is deterministic and included in prepared assignment
   fingerprint.

### F. Workspace/source

1. Low-risk `auto` may bind in-place when packet allows in-place.
2. User can choose isolated worktree for low-risk work.
3. Medium/high/critical isolated-required work rejects requested in-place.
4. Isolated worktree is created at exact packet base commit and clean.
5. Workspace repository ID mismatch rejects.
6. Source root changes during claim rejects and writes no live lease.
7. Dirty root source rejects before workspace/lease commit.
8. Failed worktree creation cleans pending directory when safe.
9. Ambiguous pending workspace is reported as cleanup needed, not hidden.
10. Release never deletes in-place repo root.

### G. Transaction/recovery

1. Crash/failpoint after intent before any target recovers cleanly.
2. Crash after runtime records before node update either completes or rolls back
   deterministically.
3. Crash after node update before event completes event or blocks with explicit
   recovery error.
4. New claim is blocked while ambiguous assignment recovery exists.
5. Recover command reports completed/released/expired/requeued/stale/ambiguous
   records.
6. No duplicate event for one completed claim.
7. Prepared assignment returned by claim matches committed record bytes.

### H. Read-only and side-effect boundaries

1. Failed preflight writes no lease/workspace/prepared records.
2. Successful claim writes only runtime assignment records, managed workspace,
   Ticket node transition and event.
3. Claim does not edit docs registry, docs files, evidence receipts, knowledge
   records or WorkPacket schema.
4. Non-enrolled repo rejects before creating `.pulse/runtime` paths.
5. Tracked fixture remains immutable and contains no generated runtime state.

### I. Determinism/schema

1. DTOs use `deny_unknown_fields` in round-trip tests.
2. Canonical JSON contains no floats.
3. Prepared assignment fingerprint excludes itself.
4. Same committed assignment record validates against schema.
5. Lease/workspace/prepared records validate independently.
6. Unknown future runner fields are rejected unless explicitly nullable in v1.

### J. Architecture

1. CLI handler owns no claim business logic.
2. Graph pure read layers import no runtime assignment modules.
3. Node/edge schemas gain no lease/workspace fields.
4. `pulse::source` public path remains valid and reused.
5. Storage transaction extensions stay domain-neutral.
6. Public API compile guard covers `pulse::assignment::PreparedAssignmentV1`.

---

## Implementation sequence

### P2S2-I1 — Lock assignment value contracts

- Add `PreparedAssignmentV1`, lease, workspace, capability inventory and
  capability match DTOs.
- Add JSON Schemas and deny-unknown round-trip tests for every public Slice 2
  DTO, including fixture cases that reject runner/mailbox/proof fields not
  explicitly represented as nullable `not_installed` placeholders.
- Implement normalization, canonical fingerprint and no-float guarantees.
- Export `pulse::assignment` and add/extend public API compile guards without
  changing `pulse::work_packet::WorkPacketV1`.

### P2S2-I2 — Add runtime assignment store and recovery skeleton

- Define runtime paths and safe repository-relative validation.
- Implement list/load/write for lease/workspace/prepared records and terminal
  tombstones using canonical JSON.
- Add read-only recovery classification over incomplete/expired/ambiguous
  records before adding mutating repair.
- Ensure preserve/no-bootstrap enrollment validation runs before runtime
  directory creation or lock acquisition, including negative tests on
  non-enrolled repositories.

### P2S2-I3 — Extend storage transactions for multi-target assignment commit

- Add domain-neutral multi-target + event transaction support if existing
  `prepare_multi_target_transaction` cannot include event/node/runtime targets
  as needed.
- Use current storage failpoint semantics (`AfterIntent`,
  `AfterMultiTargetFirst`, `AfterMultiTargetAll`, `AfterEvent`) as the stable
  mechanical crash points. Do not encode semantic assumptions such as
  "after-runtime-records" or "after-node" unless storage provides a
  domain-neutral way to name target-index failpoints after lexicographic sorting.
- Add recovery tests before wiring claim.

### P2S2-I4 — Implement capability inventory matching

- Add inventory DTO/parser/canonical hash.
- Match exact required packet capabilities plus workspace-induced requirements.
- Add CLI fixture helpers and unit tests.

### P2S2-I5 — Implement workspace binding helpers

- Add `src/workspace.rs` for in-place and isolated worktree binding.
- Reuse/extend `source.rs` for validation; do not duplicate Git state logic.
- Add cleanup/adoption behavior for pending worktrees.
- Add source/worktree tests including detached/linked worktree where supported.

### P2S2-I6 — Extract fence-aware packet revalidation for claim

- Refactor `kernel::packet` so claim can build/revalidate a fresh packet without
  deadlocking on `WriteGuard`.
- Preserve public `work packet` behavior and tests unchanged.
- Add tests proving claim does not mutate preview packet semantics.

### P2S2-I7 — Install `ready -> active` prepared-assignment gate

- Extend lifecycle model supported targets/gates without allowing the generic
  public transition path to satisfy the gate.
- Add internal gate evaluation from prepared assignment context.
- Ensure direct transition CLI rejects with `prepared_assignment_required`, not
  `transition_gate_unavailable` and not a generic lease error.
- Add graph/lifecycle tests.

### P2S2-I8 — Implement `work claim`

- Add CLI args and thin handler.
- Implement `JsonGraphStore::claim_work` orchestration and transaction ordering.
- Return committed `PreparedAssignmentV1` bytes/projection.
- Add happy path integration tests.
- Verify JSON errors preserve assignment top-level codes and lower-layer packet
  or source `cause_code` values.

### P2S2-I9 — Implement release/list/recover surfaces

- Add `work release`, `work leases`, `work leases recover`.
- Scope release to prepared/no-run assignments only; do not add runner-active
  cancellation semantics or a live `active` lease state.
- Add stale/expired lease tests and safe cleanup tests.
- Add enriched execution-frontier claim-state join in kernel/CLI composition,
  keeping the pure graph/read frontier unchanged.

### P2S2-I10 — Concurrency, failpoint and side-effect hardening

- Add process tests for duplicate claim race.
- Add failpoint crash recovery tests using storage-level failpoints rather than
  semantic write-order names.
- Assert forbidden side effects on failed preflight, failed capability matching
  and failed workspace binding.
- Assert target fixture immutability and non-enrolled repositories reject before
  `.pulse/runtime` is created.
- Run `git diff --check` plus both tracked and untracked diff inspections before
  handing off implementation work.

### P2S2-I11 — Implementation completion documentation after code only

After code is implemented and verified, update implementation completion
evidence and roadmap completion status. This item is completed through the
status header update, DoD check-off, completion evidence section and roadmap
updates applied in this documentation commit. The accepted proposal status above
only locks this file as the implementation plan; it does not claim source
implementation or verification completion. Documentation updates must not create
Pulse work-graph/docs-registry/evidence state in this development repository
unless a maintainer separately approves self-hosting.

Implementation and verification commits are listed in the completion evidence
section above. This documentation commit itself is the I11 deliverable: it does
not create Pulse work-graph/docs-registry/evidence state, and it accurately
cites implementation/verifier commits and validation results while
distinguishing Slice 2 completed from broader Phase 2 not complete.

---

## Roadmap scenarios owned by this slice

This slice advances but does not fully complete:

- **#8:** final lease/workspace-bound assignment wrapper on top of work packet;
- **#12:** interruption/recovery foundation for prepared assignment state only,
  not runner resume;
- **#40:** execution frontier claim state becomes runtime-derived instead of
  `not_evaluated` for live prepared leases;
- **#66/#69 subset:** specific-assignee exclusive reservation prevents duplicate
  Worker ownership, but independent Codex task creation remains later;
- **#67 subset:** unstarted prepared assignment can expire/release without ghost
  ownership, but acknowledgement/delivery TTL remains later;
- **#75 subset:** release/revoke preserves workspace conservatively, but active
  supersession with partial handoff remains later.

It does not claim full Phase 2 exit because no run, cancel/resume, handoff,
verification or close gate exists yet.

---

## Definition of Done

All DoD items are verified complete through the implementation commits listed
in the completion evidence below.

- [x] `PreparedAssignmentV1`, assignment lease, workspace and capability match
      DTOs/schemas exist with deny-unknown tests and canonical fingerprints.
- [x] `WorkPacketV1` preview semantics remain unchanged; tests assert nested
      packet remains non-authorized.
- [x] `pulse work claim <ticket-id> --assignee ... --capabilities ... --json`
      returns prepared assignment on happy path.
- [x] Claim revalidates graph/readiness/docs/policy/source/packet fingerprint
      before committing lease.
- [x] Concrete capability inventory must fully match packet-required
      capabilities.
- [x] Workspace mode respects Slice 1 risk/strategy mapping; no downgrade from
      isolated-required to in-place.
- [x] Isolated worktree binding creates a clean exact-base workspace and records
      workspace identity.
- [x] In-place binding is recorded and never deletes repo root on release.
- [x] Exclusive live lease prevents duplicate claims; concurrent same-ticket
      claim has exactly one winner.
- [x] Ticket transitions `ready -> active` only via prepared assignment gate.
- [x] Direct transition to `active` without assignment is rejected.
- [x] Lease/workspace/prepared records and Ticket transition commit/recover as
      one coherent transaction family.
- [x] Crash/failpoint recovery covers intent, runtime records, node update,
      event and workspace pending cleanup/adoption.
- [x] `work release` safely releases prepared/no-run assignments and can requeue
      to `ready` with expected revision.
- [x] `work leases` and `work leases recover` expose runtime state without
      mutating graph except explicit recovery/release operations.
- [x] Execution frontier joins runtime claim state without persisting it in node
      JSON.
- [x] Failed claims do not leave live leases; ambiguous workspaces are surfaced
      as operator cleanup, not hidden.
- [x] Claim creates no runner/mailbox/handoff/verification/QA state.
- [x] Non-enrolled repo rejects before runtime bootstrap.
- [x] `cargo fmt --check` passes.
- [x] `cargo clippy --all-targets --quiet -- -D warnings` passes.
- [x] `cargo test --all-targets` passes under default threading.
- [x] `git diff --check` passes.
- [x] Implementation completion evidence and roadmap completion status are
      updated only after verified implementation commits land.

### Completion evidence

Verified implementation commits (all between proposal acceptance `075b161` and
final verification `e6c6402`):

- `0c9a888` — clean: P2S2-I1 assignment value contracts, DTOs, schemas,
  fingerprints and deny-unknown tests.
- `770fc74` — commit: P2S2-I1 schema verification hardening.
- `57ec28f` — commit: P2S2-I2 runtime assignment store, record IO and
  recovery classification.
- `37605db` — commit: P2S2-I2 store verification hardening.
- `f66d103` — commit: P2S2-I3 domain-neutral multi-target + event transaction
  support.
- `119c328` — commit: P2S2-I3 transaction semantics hardening.
- `9abb940` — commit: P2S2-I4 capability inventory matching.
- `5620467` — commit: P2S2-I4 matching verification hardening.
- `dd704da` — commit: P2S2-I5 workspace binding helpers (in-place and
  isolated worktree).
- `b3720ac` — fix: P2S2-I5 workspace binding safety.
- `e1f86e6` — commit: P2S2-I6 fence-aware packet revalidation for claim.
- `ff571d6` — fix: fenced packet docs search deadlock resolution.
- `1dc2473` — commit: P2S2-I7 prepared-assignment lifecycle gate.
- `aa60e44` — fix: P2S2-I7 gate hardening.
- `7108e9e` — commit: P2S2-I8 claim pipeline orchestration and transaction
  ordering.
- `a3c80cb` — fix: atomic claim pipeline hardening.
- `f8a747b` — commit: P2S2-I9 release/leases listing/recovery surfaces and
  frontier claim-state enrichment.
- `a2d6e10` — commit: P2S2-I9 release recovery frontier hardening.
- `367dfed` — commit: P2S2-I10 concurrency, failpoint and side-effect
  hardening.
- `e6c6402` — commit: P2S2-I10 final hardening and concurrent verification.

Final P2S2-I11 verification evidence before marking complete:

- `cargo fmt --check` — pass.
- `cargo clippy --all-targets --quiet -- -D warnings` — pass.
- `cargo test --all-targets` under default threading — pass: 675 tests across
  library, binary and integration crates.
- `git diff --check` — pass.

This completes only Phase 2 Slice 2: atomic reservation, workspace binding and
`PreparedAssignmentV1`. Phase 2 as a whole is not complete: Pulse still has no
runner, cancel/resume, handoff, verification or close gate. Those remain later
Phase 2 slices beyond Slice 2.

---

## Resolved proposal choices

The transaction/recovery choices below are locked for implementation unless a
maintainer explicitly reopens this proposal:

1. Canonical command is `work claim`; `work prepare` is not a public Slice 2
   command and may exist only as a hidden developer alias routed to the same
   handler.
2. Managed isolated worktrees use detached HEAD at the packet base commit.
3. Release/recovery keeps terminal lease tombstones as local projection and
   idempotency aids; immutable events remain the audit truth.
4. Default TTL is 1800 seconds, with CLI bounds min 60 and max 86400.
5. `work leases recover --actor <principal>` applies safe deterministic fixes by
   default and reports ambiguous/dirty/unknown cases without mutating them.
6. Assignment transaction intents use the shared storage transaction directory
   `.pulse/runtime/transactions`; no assignment-specific transaction root is
   introduced in Slice 2.
7. Direct `ready -> active` is not a public transition path; only `work claim`
   may commit that transition with runtime records and event atomically.

None of these choices justify changing `WorkPacketV1` or adding runner/proof
semantics to Slice 2.
