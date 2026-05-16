# Corrections And Ratchet Templates

Use this file when writing either:

- `.pulse/memory/corrections/YYYYMMDD-<slug>.md`
- `.pulse/memory/ratchet/YYYYMMDD-<slug>.md`

Use YAML frontmatter so recall can score the entry reliably.

---

## Shared YAML Frontmatter (required, line 1 of file)

```yaml
---
date: YYYY-MM-DD
feature: <feature-name>
severity: critical | standard
tags: [tag1, tag2]
applies_when: <specific technical trigger>
scope: [path/or/component, optional-second-scope]
signals: [symptom1, symptom2]
---
```

---

## Correction Body Format

Use for tactical corrective rules that prevent a repeated or expensive mistake.

```markdown
# Correction: <Concise Title>

**Why this exists:** <the repeated or expensive mistake this correction prevents>

## Wrong move

<Describe the bad assumption or action in one short paragraph.>

## Correct move

<Describe the concrete replacement behavior future agents should follow.>

## Evidence

- Feature: <feature-name>
- Files / commands / artifacts:
  - <path or command>

## Propagation

**Propagation:** correction
**Planner action:** attach this file in bead `learning_refs` when the trigger clearly matches.
```

---

## Ratchet Body Format

Use for non-regression rules earned by repeated failures, repeated corrections, or an especially costly miss.

```markdown
# Ratchet: <Concise Title>

**Rule:** <the must-check or non-regression bar future agents must honor>

## Why this became a ratchet

<Explain the repeated or expensive failure pattern that promoted this into a must-check rule.>

## Required checks

- <verification or review step 1>
- <verification or review step 2>

## Evidence

- Feature: <feature-name>
- Files / commands / artifacts:
  - <path or command>

## Propagation

**Propagation:** ratchet
**Planner action:** attach this file in bead `learning_refs` when the trigger clearly matches.
**Validator action:** treat this as a must-check when the trigger clearly matches.
```

---

## Quality bar

- `applies_when` must name a specific trigger, not a general phase.
- `scope` should name files, components, or surfaces when known.
- `signals` should capture the symptoms that would make this entry relevant.
- Corrections should stay short, tactical, and directly actionable.
- Ratchets should include concrete required checks and stay narrow enough that they are not ignored as noise.
