# Topic Merge Rules

Load this reference before deciding whether a raw note should merge into an existing topic or create a new topic.

## Merge into an existing topic when

- the raw note reinforces the same core idea as an existing topic
- the wording differs, but the reusable lesson is the same
- the new note adds another example, heuristic, or failure shape to a stable concept
- splitting the note into a new topic would create taxonomy drift or near-duplicate pages

## Create a new topic when

- the raw note introduces a genuinely different reusable lesson
- the existing topics would become vague or overloaded if this note were merged into them
- the concept would be hard to rediscover later without its own stable slug

## Never do these

- create a new topic only because the wording differs
- merge two topics that have meaningfully different advice just to keep the list short
- infer a stronger principle than the raw notes support
- leave `TOPICS.md` stale after changing the topic set

## Tie-break rule

If a note plausibly fits multiple topics, prefer the topic whose `Core idea` would need the fewest semantic changes to absorb the new note. Use `Related topics` for the secondary connection instead of spawning a near-duplicate page.
