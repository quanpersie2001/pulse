# `/pulse note-distill`

Reader-facing synthesis command that converts pending raw dev notes into durable topic knowledge and keeps the global topics index aligned.

This is topic consolidation, not raw capture and not post-cycle memory compounding.

## Mission

Turn evidence-grounded raw notes into reusable conceptual topics without duplicating taxonomy, overstating confidence, or leaving the index stale.

## Entry criteria

Run `/pulse note-distill` when:

- undistilled raw notes have accumulated
- recurring tactical insights should become stable topic guidance
- readers need a cleaner topic surface than the raw-note stream provides

Do not run when:

- the user only wants to save one new learning now (`note`)
- there are no pending raw notes worth synthesizing
- the goal is post-cycle memory/rule synthesis across completed work (`compound`)

## Required reads

Before updating any artifact, read:

1. `skills/pulse/commands/note-distill/references/topic-template.md`
2. `skills/pulse/commands/note-distill/references/topics-index-template.md`
3. `skills/pulse/commands/note-distill/references/topic-merge-rules.md`
4. pending entries in `works/notes/raws/YYYYMMDD.md` files
5. existing topic artifacts under `works/notes/distil/topics/` when relevant
6. `works/notes/distil/TOPICS.md` when it already exists

Template-first is mandatory. Do not patch topic artifacts freeform.

## Output contract

This command may create or update only these artifacts:

- `works/notes/distil/topics/<slug>/<slug>.md`
- `works/notes/distil/TOPICS.md`
- distilled status/provenance fields inside raw-note source files

Topic files must match `topic-template.md`.
The global index must match `topics-index-template.md`.
Raw-note status writes must remain factual and minimal.

## Phase model (mandatory order)

### Phase 1 — Collect pending raw-note evidence

Select only raw-note entries that are not yet marked distilled.

Those entries are the evidence ceiling:

- do not over-claim certainty
- preserve ambiguity where source notes are thin
- do not manufacture a stronger principle than the notes support

### Phase 2 — Build the topic candidate set

Cluster the pending notes by reusable lesson.

For each cluster, identify:

- likely core idea
- overlapping existing topics, if any
- whether the note adds heuristics, examples, or failure shapes

This phase is only for organizing evidence. Do not create topic files yet.

### Phase 3 — Decide merge vs create

Load and apply `topic-merge-rules.md` explicitly.

For each cluster:

- merge when the core lesson overlaps an existing stable topic
- create a new topic only when the concept is genuinely distinct

Do not create near-duplicate topics because wording differs.
Do not force unrelated notes together just to keep the list short.

### Phase 4 — Normalize and update topic artifacts

For each affected topic:

- create the file from `topic-template.md` if it does not exist
- normalize it first if the existing structure drifted
- update `description`, `note_count`, `updated_at`, and `source_notes` honestly
- refresh `Core idea`, `Heuristics`, `Common failure shapes`, and `Examples from notes`
- keep `Related topics` useful but lean

Each topic file represents one stable concept artifact, not a distillation run log.

### Phase 5 — Rebuild the global topics index

After any topic create/update, rebuild:

- `works/notes/distil/TOPICS.md`

Use `topics-index-template.md`.
This is required, not optional.
If topics changed, the index changes too.

### Phase 6 — Write provenance back to raw notes

For every raw-note entry actually distilled:

- set `Status: distilled`
- set `Distilled into: [topic-slug, ...]` with only the topics truly influenced by that note

Do not write narrative run logs into raw-note files.
Do not claim linkage to topics that were not updated or created from that note.

### Phase 7 — Return concise synthesis results

Respond with a short operator summary covering:

- how many raw notes were distilled
- which topics were created
- which topics were updated
- whether `TOPICS.md` was rebuilt

Do not create a markdown run-summary artifact.

## Role boundaries

`/pulse note-distill` owns:

- topic-level knowledge synthesis
- topic/index template conformance
- factual provenance from raw notes to topics

`/pulse note-distill` does not own:

- new raw-note capture (`note`)
- broad workflow/process memory propagation (`compound`)
- confidence inflation beyond note evidence

## Guardrails

- Never treat `TOPICS.md` as optional cleanup.
- Never patch inconsistent topic files without normalizing them.
- Never create duplicate topics for minor wording differences.
- Never infer stronger conclusions than the raw notes support.
- Never create extra markdown artifacts outside the topic-distillation contract.

## Red flags

Stop and correct the approach if any of these appear:

- multiple plausible merge targets with no clear primary home
- a topic file is being updated freeform without template normalization
- a note is being marked distilled without a real topic update
- the index is left stale after topic changes
- uncertain raw notes are rewritten as deterministic doctrine

## Exit contract

Successful exit requires:

- every distilled note linked to exact affected topic slugs
- updated/created topic artifacts in managed structure
- refreshed `works/notes/distil/TOPICS.md`
- concise chat confirmation only

## Next command guidance

- `compound` when distilled insights should influence broader workflow policy or memory
- `note` for future single-learning captures
