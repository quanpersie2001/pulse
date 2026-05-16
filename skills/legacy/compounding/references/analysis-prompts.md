Use these prompts for Phase 2 subagents.

## Agent 1: Pattern Extractor

```text
Read the feature artifacts provided. Identify all REUSABLE PATTERNS that emerged:

- Code patterns: new functions, utilities, abstractions worth standardizing
- Architecture patterns: structural decisions that worked and should be repeated
- Process patterns: workflow approaches that saved time or prevented errors
- Integration patterns: how this feature connected to other systems

For each pattern:
1. Name it concisely
2. Describe what it does and why it's valuable
3. Note the specific file/location where it was first used (if applicable)
4. State "applicable-when": under what conditions should future agents use this?

Write findings to: /tmp/compounding-patterns.md
```

## Agent 2: Decision Analyst

```text
Read the feature artifacts provided. Identify all significant DECISIONS made during this work:

- Good calls: decisions that proved correct, saved time, or prevented problems
- Bad calls: decisions that were wrong, required rework, or added unnecessary complexity
- Surprises: things that turned out differently than expected (either direction)
- Trade-offs accepted: conscious choices where alternatives were considered

For each decision:
1. State the decision clearly (what was chosen vs what was rejected)
2. Describe how it played out in practice
3. Tag as: GOOD_CALL | BAD_CALL | SURPRISE | TRADEOFF
4. State the recommendation for future work

Write findings to: /tmp/compounding-decisions.md
```

## Agent 3: Failure Analyst

```text
Read the feature artifacts provided. Identify all FAILURES, BLOCKERS, and WASTED EFFORT:

- Bugs introduced and their root causes
- Wrong assumptions that required backtracking
- Blockers encountered and how they were resolved (or not)
- Wasted effort: work done that turned out unnecessary
- Missing prerequisites discovered mid-execution
- Test gaps that allowed regressions

For each failure:
1. Describe what went wrong
2. Identify the root cause (not just the symptom)
3. State how long it blocked progress (estimate)
4. Write the prevention rule: what should future agents do differently?

Write findings to: /tmp/compounding-failures.md
```