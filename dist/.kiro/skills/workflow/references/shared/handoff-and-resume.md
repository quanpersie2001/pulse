# Handoff and resume

Handoffs preserve execution continuity across pauses, context limits, and
ownership transitions. They are human-authored work artifacts; the workflow
skill does not create a repository-local manifest or hidden state record.

## Principles

- handoffs are owner-scoped;
- the owning work artifact is the source for the handoff note;
- live runtime posture is confirmed with `pulse daemon status`; and
- a known daemon session is inspected with `pulse session inspect <id>`.

## Required handoff payload

A handoff should capture:

- active workflow command and current work slice/item;
- completed work and remaining work;
- blockers/open questions;
- reservation or conflict state when relevant;
- read-first artifacts for fast restore; and
- recommended next action/command.

## Resume flow

1. Read the supplied work-artifact handoff.
2. Verify its source commit and changed paths.
3. Confirm live posture with `pulse daemon status` and `pulse work show <id>
   --repo-root <repo> --json` when the item ID is known.
4. Inspect `pulse session inspect <id>` when a daemon session is involved.
5. Continue the same workflow command or reroute to the smallest safe command.

The workflow skill may draft or summarize a handoff, but it must not write
Pulse canonical state. Use the existing `pulse session handoff` command only
when a daemon-owned handoff is explicitly required and all required arguments
and approvals are present.
