# Harness Backlog

Use this backlog to capture improvements to the Pulse harness itself.

This is a **seed artifact**, not the harness contract.
The contract lives in `skills/workflow/references/HARNESS.md`.

## How to use

- add one entry per harness pain point or improvement idea
- keep each entry scoped to one observable problem
- link supporting evidence when it exists
- update status instead of duplicating the same issue repeatedly

## Entry template

```markdown
### <short title>
- Discovered while: <command, task, or scenario>
- Current pain: <what made the harness harder to use or trust>
- Suggested improvement: <concrete change>
- Risk: <low|medium|high>
- Status: <new|triaged|planned|done>
- Evidence: <optional links, files, or notes>
```

## Seed entries

### Router help could be clearer
- Discovered while: `pulse:workflow` with no subcommand
- Current pain: help surfaces can drift away from the actual command table
- Suggested improvement: keep help text generated from the structured command metadata
- Risk: medium
- Status: new
- Evidence: `skills/workflow/SKILL.md`, the Rust CLI command surface

### Runtime relocation follow-up
- Discovered while: Phase 1 router build
- Current pain: coordination and readiness contracts need continued verification at the Rust CLI boundary
- Suggested improvement: keep the Rust CLI and daemon as the only mutable authorities while materializing the target-repository layout under `target-repository Pulse data/`
- Risk: high
- Status: planned
- Evidence: `plan.md`, `solution-design.md`
