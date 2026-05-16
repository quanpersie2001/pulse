# Creation Log: pulse:dev-note

## Source Material

Origin:
- User request for a learning-oriented skillset that helps developers retain what they learn while coding with AI
- `skills/writing-pulse-skills/SKILL.md`
- `skills/writing-pulse-skills/references/creation-log-template.md`
- `skills/compounding/references/learnings-template.md`
- `skills/exploring/references/context-template.md`

What the source does:
- Captures durable learnings into structured Markdown artifacts instead of leaving them as chat-only context.
- Enforces output contracts through hard templates and artifact discipline.

Pulse context:
- Supports developer learning during coding/debugging/brainstorming without collapsing into transcript logging.

## Extraction Decisions

What to include:
- Hard-template enforcement for every markdown artifact — because the user explicitly required it and baseline pressure favors ad hoc notes.
- One-learning-per-entry discipline — because baseline pressure tends to dump broad transcript context instead of reusable insight.
- Clarify-on-ambiguity behavior — because multiple plausible learnings can exist in one chat segment.

What to leave out:
- Any extra markdown report artifact — because v1 should keep the artifact surface minimal and every extra `.md` would need another hard template.
- Distillation logic — because that belongs to `dev-note-distil`, not the raw capture skill.

## Structure Decisions

1. Separate the daily file template from the entry template, because the daily file is a container and the learning entry is the reusable contract.
2. Keep v1 output limited to `dev-notes/raws/YYYYMMDD.md`, because a narrow artifact contract is easier to enforce.
3. Require clarification only when needed, because baseline behavior was already strong there and over-questioning would slow the workflow.

## Bulletproofing Elements

### Language Choices
- "Do not write or update the raw note file before loading the matching hard templates." — because baseline pressure strongly favored quick ad hoc append behavior.
- "Capture one concrete learning, not a transcript or TODO log." — because broad debug-session dumps are tempting under speed pressure.
- "If the target learning is ambiguous, ask one short clarification question." — because combining multiple ideas into one note lowers reuse quality.

### Structural Defenses
- Two hard templates (`daily-note-template.md` + `note-entry-template.md`) prevent the "quick freeform append" shortcut.
- A strict artifact boundary keeps the skill from creating extra markdown outputs without contracts.

## RED Phase: Baseline Testing

### Scenario 1: Quick Note Without Template

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

The user says: 'Note lại ý stale closure vừa rồi giúp tôi, nhanh thôi.' You are in the middle of a coding/debugging chat with a lot of useful context. There is no predefined template in front of you. You want to be efficient.

Options:
A) Append a quick freeform markdown note with whatever sections feel useful right now.
B) Stop and ask for a formal schema/template before writing any note.
C) Summarize the whole recent discussion so nothing is lost.
```

**Combined pressures:** Time + Pragmatic

**Agent choice:** A

**Exact rationalization (verbatim):**
> "A. The user asked for a fast note in the middle of active debugging, so the best move is to capture the stale-closure insight immediately in lightweight freeform markdown. That preserves the key idea while context is fresh, avoids blocking on schema questions, and avoids wasting time summarizing everything. Since there is no predefined template, a short ad hoc note is the most efficient and least disruptive choice."

**Verdict:** FAIL

---

### Scenario 2: Messy Existing Daily File

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

A daily raw note file for today already exists, but its structure is messy and inconsistent. The user says: 'Ghi thêm learning về async state race vào note hôm nay nhé.' You want to minimize churn.

Options:
A) Append a small ad hoc section to the existing file and avoid reformatting older content.
B) Normalize the whole file into a stricter structure before adding the new note.
C) Create a brand new separate note file with a one-off name so you do not touch the messy daily file.
```

**Combined pressures:** Pragmatic + Scope control

**Agent choice:** A

**Exact rationalization (verbatim):**
> "Appending a small ad hoc section to today’s existing note minimizes churn, preserves the user’s current workflow, and avoids rewriting older messy content that the user did not ask to clean up. The request is specifically to add one learning about async state race into today’s note, so the safest interpretation is a targeted append. Full normalization introduces unnecessary edits and risk of accidental meaning changes, while creating a separate one-off file fragments the daily record and makes retrieval harder."

**Verdict:** FAIL

---

### Scenario 3: Ambiguous Target Learning

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

The user says: 'Note lại insight vừa nãy.' But the recent conversation contains three plausible learnings: a debugging heuristic, a React stale-closure lesson, and a collaboration rule about not over-trusting AI summaries. You want to keep the flow moving.

Options:
A) Pick the most technical-looking learning and note it without asking.
B) Ask one short clarification question to determine which learning the user wants saved.
C) Combine all three into one big note so you do not miss anything.
```

**Combined pressures:** Ambiguity + Flow preservation

**Agent choice:** B

**Exact rationalization (verbatim):**
> "Vì có ba insight đều hợp lý nhưng khác nhau về mục đích, việc đoán sẽ dễ lưu sai thứ người dùng thực sự muốn giữ. Một câu hỏi ngắn như 'Bạn muốn note insight nào: debugging heuristic, stale-closure của React, hay rule về không over-trust AI summary?' vừa đủ rõ để disambiguate, vừa ít làm chậm nhịp hội thoại. A tối ưu tốc độ nhưng rủi ro sai intent; C tránh bỏ sót nhưng tạo note loãng, kém truy xuất và có thể ghi thừa điều người dùng không muốn lưu."

**Verdict:** PASS

---

### Scenario 4: Transcript vs Reusable Learning

**Setup:**
```text
IMPORTANT: This is a real scenario. You must choose and act.

The user says: 'Save hết mấy ý quan trọng từ 10 phút debug vừa rồi vào dev notes đi.' The conversation includes bug symptoms, dead-end hypotheses, one real insight, and some implementation chatter. You want to avoid losing context.

Options:
A) Save a broad note that includes the main transcript chunks so future-you has all the context.
B) Extract only the reusable learning and omit transcript-style clutter.
C) Refuse to note anything until the user narrows it to one sentence.
```

**Combined pressures:** Completeness + Pragmatic

**Agent choice:** B

**Exact rationalization (verbatim):**
> "Dev notes should preserve signal, not replay the whole debug session. Save the bug symptoms, the confirmed root insight, what was ruled out, and any follow-up decision or caveat; drop transcript-style chatter and dead-end detail unless it changes future debugging. That keeps notes compact, searchable, and actually reusable, while still preventing context loss. Broad transcript chunks create noisy notes that future-you won’t reread, and refusing to note anything is too brittle for a real workflow."

**Verdict:** PASS

---

### RED Phase Summary

Patterns identified:
- Baseline behavior strongly favored freeform appends when no template was explicitly loaded.
- Existing-file messiness triggered a "minimize churn" rationalization that preserved inconsistent structure.
- Baseline behavior was already good at asking one short clarification question under ambiguity.
- Baseline behavior was already good at preferring reusable learning over transcript dumps when prompted directly.

Target rationalizations for GREEN phase:
1. "Since there is no predefined template, a short ad hoc note is the most efficient and least disruptive choice."
2. "Appending a small ad hoc section to today’s existing note minimizes churn"

## GREEN Phase: Initial Skill

Implemented:
- `skills/dev-note/SKILL.md`
- `skills/dev-note/references/daily-note-template.md`
- `skills/dev-note/references/note-entry-template.md`

WITH-skill re-run results:
- Scenario 1: PASS — chose B and cited the template-first hard gate, explicitly rejecting the quick freeform note shortcut.
- Scenario 2: PASS — chose B and cited the requirement to normalize the daily file into the hard template before adding a new entry.
- Scenario 3: PASS — chose B and cited the rule to ask one short clarification question when multiple plausible learnings exist.
- Scenario 4: PASS — chose B and cited the one-learning-per-entry rule and the ban on transcript-style dumping.

GREEN verdict:
- The skill closed both target rationalizations from RED.
- No scenario regressed.

## REFACTOR Phase: Iterations

Iteration 1 observations:
- No new loophole appeared in the with-skill run.
- The strongest controls were the template-first hard gate, the normalize-before-append rule, and the single-learning capture rule.

Refactor action:
- No further wording patch was required after the first GREEN pass.

## Final Outcome

Initial GREEN verification passed for all scenarios.
Formal repo validation is still pending.

---

*Created: 2026-04-22*
*Skill version: 1.0*
*Purpose: Capture one structured developer learning from the current coding-with-AI context into a raw daily note artifact.*
