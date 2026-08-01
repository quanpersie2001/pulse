# Review Agent Prompts

Use when `pulse:workflow review` needs focused specialist review passes before Gate 4 synthesis.

Use these focused prompts to run specialist review passes. Give each reviewer the approved `solution-design.md`, approved `plan.md`, relevant item README files, verification evidence, `implement-gap.md` files when present, and the reviewed diff/range.

## Behavior correctness reviewer

```text
Review this changeset for functional correctness against the approved current-slice scope, solution-design decisions, plan constraints, and item README contracts. Lead with findings. For each finding include severity P1/P2/P3/P4, affected item/story, evidence path or file/line, failure scenario, and smallest credible fix. Do not rewrite code.
```

## Regression and boundary reviewer

```text
Review this changeset for regressions across neighboring flows, module boundaries, public contracts, runtime/workgraph behavior, docs contracts, and approved file scope. Lead with concrete boundary violations or regression risks. Include severity, evidence, affected paths/items, and smallest credible fix. Do not rewrite code.
```

## Security and misuse reviewer

```text
Review this changeset for security weaknesses introduced or exposed by the implementation. Prioritize auth, data handling, trust boundaries, unsafe input/output, secret handling, permissions, and abuse paths. Lead with exploitable or policy-relevant findings. Include severity, evidence, failure scenario, and smallest credible fix. Do not rewrite code.
```

## Evidence and implementation-gap reviewer

```text
Validate whether verification evidence is fresh, specific, reproducible enough, and mapped to each closed Ticket. Review every implement-gap.md file and flag unapproved deviations, hidden decisions, missing gap logs, stale evidence, or unresolved gaps. Include severity, affected item, evidence path, and required repair. Do not rewrite code.
```

## Release-readiness synthesizer

```text
Consolidate specialist findings, deduplicate overlap, assign final severity/owner/reroute, and decide Gate 4 posture: pass, pass-with-follow-ups, or fail. Explicitly summarize evidence sufficiency, implementation-gap posture, UAT/acceptance posture, blocking findings, follow-up findings, and recommended next command. Do not rewrite code.
```
