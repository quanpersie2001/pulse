# `pulse:workflow execute` Runtime Appendix

Use this appendix for compact checklists and templates referenced by `command.md`. The command file owns the execution flow; this file should stay short and avoid repeating the full procedure.

## Item contract checklist

Before reserving files or writing code, confirm the selected item is executable:

- kind is `Ticket`
- belongs to the Gate 3-approved current story/slice
- status is `READY` before claiming work; a same-owner resumed item may be
  `ACTIVE`
- dependency-unblocked and not externally blocked
- `content_dir` is exactly `works/<node-id>`; verification evidence uses a declared
  work-artifact path, not a workgraph-owned verification field
- item README has no unresolved placeholders for the fields below

Required README fields:

- Objective
- Parent Story
- Source Plan
- Decision refs
- Learning refs, even when empty
- In scope / Out of scope
- Explicit File Scope, even when intentionally empty
- Dependencies and non-blocking links
- Testing Mode: `standard` or `tdd-required`
- Verification commands with expected outcomes
- Verification Evidence path
- Caveats / Risks, explicitly `None.` when none remain

If these fields are missing or contradictory, route back to `plan`, `design`, `explore`, or `validate` as described in `command.md`.

## Non-trivial item read rule

Read the parent story `plan.md` and `solution-design.md` before writing code whenever any are true:

- `testing_mode=tdd-required`
- the item crosses modules, ownership boundaries, public contracts, runtime/workgraph, data, security, provider, or generated-artifact behavior
- verification is multi-step or integration-heavy
- multiple plausible implementations remain after reading the item README

Also read `discovery.md`, `references/*.md`, or learning refs only when cited or needed for the item.

## Implementation gap log

Create or update an item-local gap log when implementation reveals something the approved contract did not cover.

Default path:

```text
<value.content_dir>/implement-gap.md
```

Use it for:

- implementation decisions not specified by `solution-design.md`, `plan.md`, or the item README
- spec/plan ambiguity, incompleteness, drift, or contradiction with code reality
- tradeoffs that affect maintainability, UX, verification, performance, compatibility, or future work
- deviations, workarounds, scope expansions, or follow-up reviewer notes

Do **not** use this file to bypass approval. If the gap changes behavior, architecture, dependency shape, file scope, verification strategy, risk posture, or public contract, stop and get approval or reroute before implementing.

Use [implement-gap.template.md](implement-gap.template.md) as the starting shape.

If no gap occurred, no `implement-gap.md` is required.

## Verification evidence contract

Update the declared verification evidence path before review.

Evidence must include:

- item ID and parent story
- testing mode
- verification timestamp
- implementation summary
- every command run, exit code, and observed result
- relevant output snippets or artifact paths
- implementation gap path and summary when `implement-gap.md` exists
- unresolved gaps, explicitly `None.` when none remain

For `tdd-required`, also record:

- red command
- expected failure signal
- observed failure signal
- green command
- observed pass signal

Use [verification.template.md](verification.template.md) as the minimum evidence shape.

## Completion report contract

Worker `[DONE]` report:

```text
[DONE]
item_id: <TK-id>
runtime_identity: <worker-id>
commit: <hash or COMMIT_BLOCKED reason>
files_changed: [<path>]
verification: PASS
commands: [<command> -> exit <code>]
evidence_paths: [<declared-verification-path>]
implementation_gap: None. | <implement-gap.md path> — <summary>
follow_up: None. | <needed follow-up>
reservations: released | <release issue>
```

Standalone work-artifact note should include the same fields.

Blocked report:

```text
[BLOCKED]
item_id: <TK-id>
phase: contract | reservation | implementation | verification | close | commit
blocking_reason: <specific reason>
implementation_gap: None. | <implement-gap.md path> — <summary>
needs: <coordinator/user/upstream action>
recommended_reroute: use | explore | design | plan | validate | none
reservations: held | released | none
```

## Handoff for execute

Use the shared envelope from [`../use/handoff-contract.md`](../use/handoff-contract.md). For execute pauses, make sure the handoff points to the active item, current phase, reserved files, verification state, and `implement-gap.md` path when present. Do not duplicate the shared handoff schema here.

## Post-compaction recovery sequence

If compaction is detected, stop and re-read before implementing:

1. `AGENTS.md`
2. `pulse daemon status`
3. daemon posture from `pulse daemon status`
4. current item via `pulse work show <id> --repo-root <repo> --json`
5. item README, verification file, and `implement-gap.md` when present
6. parent `plan.md` and `solution-design.md`
7. active reservations
8. latest coordinator updates when in worker mode

## Red flags

Stop if you notice:

- Gate 3 approval is missing or stale
- item kind is not `Ticket`
- item README has placeholders or missing contract sections
- file scope is absent, too broad, or contradicted by implementation needs
- implementation requires changing `solution-design.md` or `plan.md`
- unplanned decisions, deviations, or tradeoffs are not captured in `implement-gap.md`
- `implement-gap.md` is used to bypass required approval
- verification evidence is stale or incomplete
- worker commit starts without `COMMIT_SLOT_GRANTED`
