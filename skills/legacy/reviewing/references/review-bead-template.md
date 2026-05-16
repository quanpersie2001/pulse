# Review Bead Template

Use this template when the reviewing skill turns a review issue into a bead.

Per-finding markdown files are retired. The bead is now the primary review artifact.

---

## Bead Creation Rules

Operational rule:
- every accepted review finding must be materialized as a bead before Gate 4 is presented
- if multiple reviewers report the same issue, deduplicate to one canonical bead and reference that bead ID in the review summary
- do not present prose-only accepted findings without creating a bead or explicitly marking them duplicate/no-issue

### Severity Mapping

| Review Severity | Bead Priority | Linkage | Blocking |
|-----------------|---------------|---------|----------|
| `P1` | `1` | Current review / epic-close path | Yes |
| `P2` | `2` | `external_ref=<source-epic-id>` + review labels | No |
| `P3` | `3` | `external_ref=<source-epic-id>` + review labels | No |

### Labels

Always include:

- `review`
- `review-p1` or `review-p2` or `review-p3`
- source reviewer label such as `code-quality`, `architecture`, `security`, `test-coverage`

Optional:

- `known-pattern`
- `breaking-change`
- `migration-risk`
- `needs-human-review`

### Title Pattern

```
Resolve Review P1: <problem title>
Resolve Review P2: <problem title>
Resolve Review P3: <problem title>
```

The title should describe the problem, not just the file name.

---

## Description Template

Put the full review detail directly in the bead description:

````markdown
## Plain-Language Summary

[1-3 sentences. Explain the issue like you are talking to a teammate who has not read the diff. Avoid shorthand such as "non-monotonic write" unless you immediately explain what that means in practice.]

## What The Code Does Today

- [current behavior in concrete terms]
- [where that behavior comes from]

## Why This Is A Problem

- [what requirement, decision, or invariant this breaks]
- [what could go wrong if left as-is]

## Concrete Failure Scenario

- [step 1 with realistic values, timestamps, request flow, or user actions]
- [step 2]
- [step 3]
- [what incorrect outcome the system reaches]

## Evidence

**File:** `<path>`
**Line(s):** <range>
```<language>
<relevant code snippet>
```

**Why this is a problem:**
[one sentence connecting the evidence to the issue]

If you reference a decision ID such as `D5`, include one short sentence translating it:

`Decision D5 in practice: <plain-language meaning>`

## Verification Evidence

**Preferred artifact:** `history/<feature>/verification/<bead-id>.md`

**Artifact status:** Missing | Stubbed | Substantive | Integrated

**Evidence:** What was actually observed or run, including commands, outputs, screenshots, logs, exit codes, or artifact paths.

**Gap:** If the evidence is missing, vague, or only asserted in prose, say so directly. Treat this as an artifact defect, not a soft note.

## Proposed Solutions

### Option A — [Recommended] <label>
**Pros:** ...
**Cons:** ...
**Effort:** Small | Medium | Large

### Option B — <label>
**Pros:** ...
**Cons:** ...
**Effort:** Small | Medium | Large

## Acceptance Criteria

- [ ] [specific, testable condition 1]
- [ ] [specific, testable condition 2]

### Writing Standard

- A reader should understand the issue before reading the code snippet
- Prefer "here is what happens" over "here is the abstract property being violated"
- If timestamps, ordering, retries, stale data, auth paths, or user actions are involved, include one realistic scenario
- The first paragraph should make the user say "I see the bug now," not "I see the terminology now"
````

If the learnings-synthesizer identifies a known pattern, append a note block to the bead:

```markdown
> KNOWN PATTERN: Matches YYYYMMDD-<slug>.md
> Previous occurrence: <path or short note>
> Past resolution: <one sentence>
```

---

## Example Commands

### P1 Blocking Review Bead

```bash
br create "Resolve Review P1: Missing auth check on route" \
  -t task \
  -p 1 \
  --parent <epic-id> \
  -l review,review-p1,security \
  -d "<full review bead body>"
```

### P2 / P3 Non-Blocking Follow-Up Bead

```bash
br create "Resolve Review P2: Tighten rate limit coverage" \
  -t task \
  -p 2 \
  --external-ref <epic-id> \
  -l review,review-p2,security \
  -d "<full review bead body>"
```

Do **not** use `--parent <epic-id>` for `P2` / `P3` review follow-up beads. That would make them part of the current epic-close path and violate the non-blocking contract.

## Artifact Verification Guidance

Use `P1` when the evidence itself is missing, stubbed, or untrustworthy.
Use `P1` when the expected `history/<feature>/verification/<bead-id>.md` artifact is absent or lacks command/exit/evidence summaries.
Use `P2` when evidence exists, but the artifact is still not wired into the review or merge path.
Use `P3` when the issue is real but clearly follow-up material that does not affect the current path.
