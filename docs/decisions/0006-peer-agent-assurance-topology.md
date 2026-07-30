# Decision 0006: Peer Worker, Reviewer, and QA task topology

## Status

Accepted.

## Context

Pulse must scale from one reliable Worker to multiple coding Agents without
making one Agent the canonical owner of implementation, review, QA and close.
At the same time, spawning an independent Agent for every low-risk check would
add cost and ceremony without proportional assurance.

## Decision

Each implementation assignment maps to one independent daemon-managed Worker
session and one writable workspace. Runtime parentage records who created or
controls a session; it does not grant business authority. The Rust daemon owns
the provider process and session lifecycle.

Orchestrator may dispatch multiple Workers when hard dependencies are satisfied,
exclusive leases and workspaces differ, write-scope conflicts are acceptable
and integration order remains explicit.

Worker performs developer verification and self-review. After handoff,
Orchestrator freezes the source snapshot and dispatches independent Reviewer,
documentation review and QA peer tasks when policy requires them. Read-only
assurance tasks may run in parallel on the same frozen snapshot. A later source
change invalidates affected receipts unless explicit ancestor/impact policy
allows reuse.

Independence is risk-adaptive:

- low-risk Ticket checkpoints may run under the Worker when policy permits;
- behavior-changing or high-risk checkpoints use an independent QA Agent;
- R2/R3 or critical Story qualification uses an independent QA Agent;
- security, destructive migration and production boundaries may require a
  specialist or human gate.

Worker, Reviewer and QA submit typed handoffs, receipts and findings. The
conductor-owned gate, not any of those Agents, decides close/rework/block.

## Consequences

- Reviewer and QA are peer Agents, not sub-agents hidden inside a Worker.
- Runtime ownership tree and peer communication graph remain separate.
- Multiple coding Workers can run concurrently without sharing writable
  workspaces.
- QA does not replace Worker tests.
- QA/Reviewer prompts are workflow bootstraps and load frozen assignments,
  baselines, docs and applicable knowledge through Pulse CLI.
- Reviewer/QA lack implicit source-write, acceptance-change, waiver, close,
  merge or deploy authority.
