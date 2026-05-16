# Dream Consolidation Rubric

Use this rubric to classify each candidate signal and choose the correct write disposition.

## 1) Signal-Type Classification

Classify every extracted candidate into exactly one signal type before deciding where it goes:
- `pattern`
- `decision`
- `failure`
- `stable-fact`
- `correction-candidate`
- `ratchet-candidate`
- `critical-promotion-candidate`
- `noise`

Guidance:
- Use `pattern` for reusable approaches that should be repeated.
- Use `decision` for durable choices or trade-offs that future agents should understand.
- Use `failure` for costly misses with a reusable prevention lesson.
- Use `stable-fact` for durable facts that meaningfully shape future work.
- Use `correction-candidate` when the core lesson is that a prior move or assumption was wrong.
- Use `ratchet-candidate` when the lesson has become a must-check or non-regression rule.
- Use `critical-promotion-candidate` only when the signal likely deserves promotion into `.pulse/memory/critical-patterns.md` after approval.
- Use `noise` for transient output, one-off command spew, local-state trivia, or non-reusable details.

## 2) Evidence Resolution Before Routing

Resolve contradictions and normalize time before deciding the destination.

### Evidence priority
1. Verified current Pulse durable memory that is still authoritative.
2. Direct timestamped runtime evidence.
3. Prior dream provenance.
4. Inferred or synthesized summaries.

### Resolution rules
- If newer/higher-confidence evidence clearly supersedes prior guidance, rewrite the prior memory entry instead of appending vague addenda.
- If the durable lesson is specifically that an older move was wrong, prefer a correction artifact over a silent overwrite when the distinction matters.
- If repeated failures or repeated corrections have hardened into a must-check, route to a ratchet artifact.
- If conflict remains material and cannot be resolved confidently, treat the candidate as ambiguous.

### Date normalization
- Convert relative dates (`today`, `yesterday`, `last week`, `this session`) into absolute dates before persistence.
- Use `YYYY-MM-DD` in durable memory content and frontmatter.
- Use precise timestamps only where provenance needs them.

### Stale-reference validation
- Validate file, command, or resource references before carrying them into durable memory.
- Remove or rewrite stale references instead of copying them forward.

## 3) Disposition Routing

After classification and evidence resolution, route the candidate into exactly one disposition:
- `merge-existing-learning`
- `create-learning`
- `create-correction`
- `create-ratchet`
- `propose-critical-promotion`
- `pending-ambiguous`
- `skip`

## 4) merge-existing-learning

Choose `merge-existing-learning` only when all are true:
- exactly one existing learning file clearly owns the same durable lesson
- the candidate strengthens or corrects that lesson without creating ownership ambiguity
- no correction or ratchet artifact is the better fit

Action:
- merge/rewrite that one owner file
- preserve durable guidance and remove contradicted details
- update `last_dream_consolidated_at` in frontmatter

## 5) create-learning

Choose `create-learning` when:
- no existing learning file is a good owner, and
- the signal is durable, but not better expressed as a correction or ratchet

Action:
- create a new dated learnings file
- add durable synthesis only
- set `last_dream_consolidated_at` in frontmatter

## 6) create-correction

Choose `create-correction` when:
- the most reusable lesson is that a prior move, assumption, or recommendation was wrong
- future agents need a tactical guardrail more than a broad learning narrative

Action:
- create or update a correction file under `.pulse/memory/corrections/`
- keep it short, trigger-based, and directly actionable

## 7) create-ratchet

Choose `create-ratchet` when:
- the lesson is now a must-check or non-regression rule
- repeated failures, repeated corrections, or a very costly miss justify a permanent bar

Action:
- create or update a ratchet file under `.pulse/memory/ratchet/`
- include concrete required checks

## 8) propose-critical-promotion

Choose `propose-critical-promotion` when:
- the lesson is broadly reusable and severe enough to deserve a critical-pattern promotion

Action:
- propose the promotion in the run summary
- do not edit `.pulse/memory/critical-patterns.md` without explicit approval

## 9) pending-ambiguous

Choose `pending-ambiguous` when any are true:
- two or more destinations have plausible ownership
- the best target file cannot be justified confidently
- the evidence conflict is material and unresolved
- the user asked for a non-blocking run that preserves unresolved items

Action:
- surface candidate-specific choices in chat:
 - `merge -> <target file>` for each plausible target
 - `create new`
 - `create correction`
 - `create ratchet`
 - `skip`
- do not silently choose a target file
- only write `.pulse/memory/dream-pending/<candidate-slug>.md` when the run is explicitly non-blocking or the user asked to preserve unresolved items

## 10) skip

Choose `skip` when:
- the signal is `noise`, or
- safe redaction is impossible, or
- no durable lesson remains after validation

Action:
- perform no durable memory write for that candidate
- record the skip reason in the run summary when useful

## Exact-One-Owner Rewrite Rule

Rewrite existing content only when exactly one owner is clear. If more than one target is plausible, treat as `pending-ambiguous` and require explicit operator choice or queue-mode preservation.
