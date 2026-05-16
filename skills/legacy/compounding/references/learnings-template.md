# Learnings File Template

Use this template when writing `.pulse/memory/learnings/YYYYMMDD-<slug>.md`.

One file per feature. Multiple learnings can appear in a single file — separate them
with `---` dividers. Start each file with the YAML frontmatter below, then add
individual learning entries.

---

## YAML Frontmatter (required, line 1 of file)

```yaml
---
date: YYYY-MM-DD
feature: <feature-name>
categories: [pattern, decision, failure]   # include only categories present
severity: critical | standard              # use "critical" if ANY entry is critical
tags: [tag1, tag2, tag3]                   # domains covered (e.g., auth, testing, beads)
applies_when: <specific technical trigger> # shared trigger for the file when relevant
scope: [path/or/component, optional-second-scope]
signals: [symptom1, symptom2]
---
```

---

## Learning Entry Format

Repeat this block for each distinct learning. Separate entries with `---`.

```markdown
# Learning: <Concise Title>

**Category:** pattern | decision | failure
**Severity:** critical | standard
**Tags:** [tag1, tag2]
**Applicable-when:** <one sentence — under what conditions should future agents use this?>

## What Happened

<2-4 sentences describing the situation: what was being built, what went wrong or right,
what surprised the team. Be specific — name files, functions, tools, or commands involved.>

## Root Cause / Key Insight

<The underlying reason this happened. For failures: what assumption was wrong, what was
missing, what interaction wasn't understood. For patterns: what property makes this
approach better than alternatives. For decisions: what information made this the right call.>

## Recommendation for Future Work

<Concrete, imperative advice. Start with a verb: "Always...", "Never...", "When X, do Y...",
"Check Z before starting...". Specific enough that a future agent can follow it without
additional context.>

## Propagation Guidance

**Propagation:** global-critical | bead-local | planner-only
**Embed-in-bead-when:** <one sentence for future planners>
**Bead hint:** <1-2 sentence summary safe to copy into a bead body if relevant>
```

For reliable targeted recall, keep `feature`, `tags`, `applies_when`, `scope`, and `signals` specific enough that the scout can distinguish a true trigger match from a weak filename similarity.

---

## Complete Example File

```markdown
---
date: 2026-03-15
feature: user-auth-refresh
categories: [pattern, failure]
severity: critical
tags: [auth, database, testing]
---

# Learning: Token Refresh Race Condition

**Category:** failure
**Severity:** critical
**Tags:** [auth, concurrency]
**Applicable-when:** Implementing any token refresh or session renewal logic with parallel requests

## What Happened

When implementing the JWT refresh endpoint, two simultaneous requests would both
pass the "token not yet expired" check, both issue new tokens, and both invalidate
the old token. The second response would arrive with a token already invalidated.
Discovered during load testing, not in unit tests.

## Root Cause / Key Insight

The check-then-act sequence was not atomic. Database read + write happened in two
separate operations with no locking. Unit tests mock the database and never simulate
concurrency, so this class of bug is invisible to standard test suites.

## Recommendation for Future Work

When implementing any token rotation or session state mutation, use a database-level
atomic operation (SELECT FOR UPDATE, optimistic locking, or a transaction with
conflict detection). Always add a concurrency integration test that fires 10 parallel
requests and asserts only one succeeds.

## Propagation Guidance

**Propagation:** global-critical
**Embed-in-bead-when:** The bead touches token refresh, session renewal, or token rotation behavior
**Bead hint:** Token rotation is concurrency-sensitive. Use an atomic database operation and include a parallel-request verification path.

---

# Learning: useAuthToken Composable Pattern

**Category:** pattern
**Severity:** standard
**Tags:** [auth, frontend]
**Applicable-when:** Any component that needs to access or refresh authentication state

## What Happened

During auth implementation, three different components each wrote their own token
retrieval logic. Consolidated into a single `useAuthToken()` composable that handles
expiry checks, automatic refresh, and error states.

## Root Cause / Key Insight

Auth state has enough complexity (expiry logic, refresh timing, error handling) that
inline implementations diverge and accumulate subtle bugs. A single composable
centralizes the contract and makes auth behavior consistent across the app.

## Recommendation for Future Work

Always reach for `useAuthToken()` in components that need auth state. Do not access
the token store directly. If the composable doesn't cover your use case, extend it
rather than writing parallel logic.

## Propagation Guidance

**Propagation:** bead-local
**Embed-in-bead-when:** A bead adds or changes frontend auth state handling
**Bead hint:** Reuse `useAuthToken()` instead of writing parallel token retrieval logic.
```

---

## Slug Naming Rules

- Format: `YYYYMMDD-<primary-topic>-<secondary-topic>`
- Use lowercase, hyphens only
- Primary topic = most important domain (`auth`, `database`, `beads`, `api`, `testing`)
- Secondary topic = specific problem or pattern (`token-refresh`, `migration-ordering`, `scope-isolation`)
- Examples:
  - `20260315-auth-token-refresh.md`
  - `20260320-bead-scope-isolation.md`
  - `20260318-db-migration-ordering.md`
  - `20260322-agent-coordination-handoff.md`

---

## critical-patterns.md Entry Format

When promoting a critical learning to `.pulse/memory/critical-patterns.md`,
use this condensed format:

```markdown
## [YYYYMMDD] <Learning Title>
**Category:** pattern | decision | failure
**Feature:** <feature-name>
**Tags:** [tag1, tag2]

<2-4 sentence summary. What happened, root cause, and what to do differently.
Enough context that a reader doesn't need to open the full file to act on it.>

**Full entry:** .pulse/memory/learnings/YYYYMMDD-<slug>.md
```

Only promote entries whose propagation is `global-critical`.
