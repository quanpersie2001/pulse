# TK-006 decision — evolve compatible fields additively

## Decision

Keep `.todolist.json` as an unversioned JSON array for optional, additive todo
fields. Interpret each missing optional field at its point of use according to
its documented default; do not eagerly materialize defaults during load.

Reserve an explicit format version and migration for a future breaking change,
such as changing the top-level array, making formerly optional data required,
or changing the meaning or representation of an existing field.

## TK-005 rule

TK-005 may add `due` as an optional property on a todo:

- a pre-due file loads through the current `loadTodos` path unchanged;
- a todo with no own `due` property is undated;
- creating an undated todo omits `due` rather than writing `due: null`;
- creating a dated todo stores the validated `YYYY-MM-DD` string verbatim;
- no format version, envelope, migration, or load-time rewrite is added; and
- all transformations must preserve properties they do not own so a later
  whole-file `saveTodos` call cannot discard user data.

This is directly compatible with the current implementation: `loadTodos`
returns the parsed array unchanged and `saveTodos` serializes the returned
array (`src/cli.mjs:18-29`), while current record updates use object spread
(`src/todolist.mjs:45-80`).

## Rationale

`due` is independently defaultable: its absence has the unambiguous meaning
"undated." Adding it does not require reinterpreting any existing value or
changing the top-level structure. A versioned envelope would therefore impose
a breaking representation change and migration risk without resolving a
problem that this field has.

Semantic defaulting also preserves the hard compatibility constraint more
strongly than normalization: old records are consumed as undated but are not
silently rewritten merely because another command saved the array.

## Consequences

- Readers and domain operations must define behavior for a missing `due` and
  validate it when present.
- Optional fields can coexist across files and records; code cannot infer a
  uniform schema generation from a version number.
- Load-time validation may still reject an invalid top-level value or invalid
  required fields, but it must not treat a missing optional `due` as invalid,
  silently repair invalid data, or discard unknown properties.
- Whole-file writes must remain lossless for properties outside the operation
  being performed. A known-field projection is not an acceptable normalizer.
- If a future breaking format is introduced, its reader must continue to
  recognize the historical unversioned array and migrate it without destroying
  the original on failure.

## Reversal triggers

Revisit this decision and design an explicit versioned format plus migration
when at least one of these becomes necessary:

- the top-level array must become an envelope or another incompatible shape;
- an existing field changes meaning or representation and cannot be decoded
  unambiguously from its value;
- previously absent data becomes required and no safe semantic default exists;
- multiple released writers need negotiated format capabilities; or
- validation or storage requirements demand a coordinated, atomic rewrite.

Crossing a trigger does not authorize an in-place lossy conversion. The
migration design must include dual-format reading, validation before replace,
unknown-field handling, atomic persistence, failure recovery, and tests using
pre-change files.

