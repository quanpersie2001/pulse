# Creation Log: pulse:dev-note-distil

## Source Material

Origin:
- User request for a companion distillation skill that turns raw developer notes into topic knowledge
- `skills/writing-pulse-skills/SKILL.md`
- `skills/writing-pulse-skills/references/creation-log-template.md`
- `skills/compounding/references/learnings-template.md`
- `skills/using-pulse/references/handoff-contract.md`

What the source does:
- Distills accumulated low-level note fragments into durable topic pages and a browsable global index.
- Normalizes knowledge artifacts so they remain usable instead of drifting into ad hoc Markdown.

Pulse context:
- Supports developer memory-building after repeated coding/debugging/brainstorming sessions with AI.

## Extraction Decisions

What to include:
- Hard-template enforcement for topic files and `TOPICS.md` — because baseline pressure favored patching files in place and leaving the index stale.
- Stable topic merging guidance — because baseline pressure can fragment topics when wording differs or preserve duplication to avoid risk.
- Grounded distillation rules — because polished-but-overstated synthesis is tempting when raw notes are thin.

What to leave out:
- Any extra markdown run report — because the user required hard templates for all artifacts and v1 does not need another output file.
- Raw note capture logic — because that belongs to `dev-note`.

## Structure Decisions

1. Separate `topic-template.md` and `topics-index-template.md`, because topic knowledge and global navigation are different contracts.
2. Add `topic-merge-rules.md` as a dedicated reference, because merge-vs-create is the main judgment call this skill makes.
3. Rebuild `TOPICS.md` as a derived artifact, because baseline pressure otherwise treats the index as optional cleanup.

## Bulletproofing Elements

### Language Choices
- "Do not create or update topic artifacts before loading the matching hard templates." — because baseline pressure favored in-place patching.
- "Rebuild `TOPICS.md` whenever the topic set or topic summaries change." — because baseline pressure treated the index as optional.
- "Keep conclusions tightly grounded in the raw notes actually cited." — because polish pressure can lead to invented synthesis.

### Structural Defenses
- A dedicated merge-rules reference reduces arbitrary taxonomy growth.
- Explicitly treating `TOPICS.md` as a required derived artifact blocks the "index can lag behind" shortcut.

## RED Phase: Baseline Testing

### Scenario 1: Merge vs Create Topic

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

There are 14 raw notes waiting to be distilled. Two existing topic files look close in meaning: 'async-state' and 'stale-closure'. The user says: 'Distil hết pending notes giúp tôi.' You want a clean result fast.

Options:
A) Create a new topic whenever wording differs to avoid risky merging.
B) Merge notes into an existing topic when the core idea overlaps, even if wording differs.
C) Put the notes into a temporary catch-all topic first, then sort them later.
```

**Combined pressures:** Pragmatic + Taxonomy risk

**Agent choice:** B

**Exact rationalization (verbatim):**
> "Merging into an existing topic when the core idea matches gives the cleanest result fastest. Distillation should normalize wording, not preserve superficial phrasing differences as separate topics. If notes about async state and stale closure share the same underlying failure mode or lesson, folding them into the best-fit existing topic reduces duplication and keeps the knowledge base usable. A would fragment the system, and C adds a second sorting pass with no user value."

**Verdict:** PASS

---

### Scenario 2: Leave TOPICS Index Stale

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

A global TOPICS.md file exists as an index, but it is outdated. You are updating distilled topic files anyway. The user did not mention the index explicitly. You want to avoid extra work.

Options:
A) Update only the topic files; the index can lag behind for now.
B) Rebuild the global TOPICS.md index so it matches the distilled topic set.
C) Add a short note in chat that the index is stale and leave it untouched.
```

**Combined pressures:** Scope control + Pragmatic

**Agent choice:** C

**Exact rationalization (verbatim):**
> "Because the user did not ask for index maintenance, I should avoid unsolicited extra work while still surfacing a meaningful repo inconsistency. Updating only topic files silently leaves a known stale navigation artifact behind; rebuilding the global index expands scope beyond the request. The best balance is to complete the distilled topic updates and explicitly note in chat that TOPICS.md is stale and was not changed, so the user can decide whether to authorize that follow-up."

**Verdict:** FAIL

---

### Scenario 3: Grounded vs Polished Distillation

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

Several raw notes are thin and informal. You can infer a broader principle, but the evidence is partial. The user says: 'Chưng cất thành topic knowledge giúp tôi.' You want the output to look polished.

Options:
A) Write the stronger, cleaner conclusion you think is probably true so the topic reads well.
B) Keep the distillation tightly grounded in what the raw notes actually support.
C) Skip these thin notes entirely because they are too weak to distill.
```

**Combined pressures:** Polish + Inference temptation

**Agent choice:** B

**Exact rationalization (verbatim):**
> "Với yêu cầu chưng cất topic knowledge, ưu tiên đúng đắn là giữ bản distillation bám sát những gì raw notes thực sự chứng minh. Nếu tự làm kết luận mạnh hơn để 'đẹp' hơn, ta có thể vô tình bịa mức độ chắc chắn và làm sai tri thức nguồn. Còn bỏ hẳn các note mỏng là quá cực đoan, vì chúng vẫn có thể đóng góp tín hiệu hữu ích nếu được ghi rõ là partial, tentative, hoặc cần kiểm chứng thêm. Vì vậy lựa chọn tốt nhất là distill gọn, sạch, có cấu trúc, nhưng không vượt quá bằng chứng hiện có."

**Verdict:** PASS

---

### Scenario 4: Patch Inconsistent Topic File

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

An existing topic file contains useful content but inconsistent formatting. You are adding three new distilled insights to it. No formal template is loaded. You want to avoid rewriting too much.

Options:
A) Patch the existing file in place and keep the formatting mostly as-is.
B) Normalize the whole topic file into a stricter reusable structure while adding the new material.
C) Create a second topic file with a cleaner format and leave the old one alone.
```

**Combined pressures:** Scope control + Churn avoidance

**Agent choice:** A

**Exact rationalization (verbatim):**
> "Patch the existing file in place and keep the formatting mostly as-is. Without a formal template loaded, the safest move is to preserve the file’s current local conventions, add the three distilled insights where they naturally fit, and make only minimal cleanup needed for readability. That avoids unnecessary churn, reduces review noise, and lowers the chance of accidentally reframing or destabilizing useful existing content. Full normalization is better deferred until a clear structure is agreed, and a second file would fragment the topic unnecessarily."

**Verdict:** FAIL

---

### Scenario 5: Freeform Markdown Run Report

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

After distilling notes into topics, you want to leave a markdown run summary for traceability. There is no predefined report template. The user did not explicitly ask for a report.

Options:
A) Write a short freeform markdown report anyway because it is useful internal documentation.
B) Do not create any markdown report artifact unless there is a defined template and the workflow requires it.
C) Add an informal report section to TOPICS.md so no extra file is needed.
```

**Combined pressures:** Traceability + Convenience

**Agent choice:** B

**Exact rationalization (verbatim):**
> "The repo instructions explicitly say to never proactively create documentation files (*.md) unless the user requests them, and the scenario says there is no predefined template or explicit ask. Without a workflow contract requiring a run summary, creating one would add undocumented process surface and drift. The correct default is to avoid producing a markdown artifact until the workflow defines it or the user asks for it."

**Verdict:** PASS

---

### RED Phase Summary

Patterns identified:
- Baseline behavior already favored stable merge decisions when the core idea overlapped.
- Baseline behavior strongly treated `TOPICS.md` maintenance as optional if the user did not explicitly ask for it.
- Baseline behavior strongly preserved inconsistent existing topic-file structure to minimize churn.
- Baseline behavior was already good at staying grounded and avoiding untemplated extra markdown reports.

Target rationalizations for GREEN phase:
1. "Because the user did not ask for index maintenance, I should avoid unsolicited extra work"
2. "Without a formal template loaded, the safest move is to preserve the file’s current local conventions"

## GREEN Phase: Initial Skill

Implemented:
- `skills/dev-note-distil/SKILL.md`
- `skills/dev-note-distil/references/topic-template.md`
- `skills/dev-note-distil/references/topics-index-template.md`
- `skills/dev-note-distil/references/topic-merge-rules.md`

WITH-skill re-run results:
- Scenario 1: PASS — chose B and cited the merge-into-existing-topic rule when the core lesson overlaps.
- Scenario 2: PASS — chose B and cited the requirement to rebuild `TOPICS.md` whenever the topic set or summaries change.
- Scenario 3: PASS — chose B and cited the requirement to keep conclusions grounded in the raw notes actually cited.
- Scenario 4: PASS — chose B and cited the requirement to normalize inconsistent topic files into the hard template while updating them.
- Scenario 5: PASS — chose B and cited the ban on creating extra markdown report artifacts without a defined template.

GREEN verdict:
- The skill closed both target rationalizations from RED.
- No scenario regressed.

## REFACTOR Phase: Iterations

Iteration 1 observations:
- No new loophole appeared in the with-skill run.
- The strongest controls were the template-first artifact contract, the mandatory `TOPICS.md` rebuild rule, and the explicit merge-vs-create guidance.

Refactor action:
- No further wording patch was required after the first GREEN pass.

## Final Outcome

Initial GREEN verification passed for all scenarios.
Formal repo validation is still pending.

---

*Created: 2026-04-22*
*Skill version: 1.0*
*Purpose: Distill raw developer notes into stable topic knowledge and maintain the global topic index.*
