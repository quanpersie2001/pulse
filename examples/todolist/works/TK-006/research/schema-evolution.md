# State-file schema evolution research

## Question and constraints

`.todolist.json` is currently an unversioned JSON array. The immediate change
under consideration adds an optional `due` property to individual todo
objects. The chosen strategy must:

- keep every pre-due array loading with the same meaning;
- avoid losing fields or records when a command writes the file; and
- give TK-005 an implementation rule without introducing a migration that is
  outside that ticket's scope.

## Current persistence path

The relevant behavior is concentrated in `src/cli.mjs`:

1. `loadTodos` reads `.todolist.json`, returns `[]` for `ENOENT`, and otherwise
   returns `JSON.parse(...)` directly (`src/cli.mjs:18-25`). It does not check a
   version, validate the parsed shape, or normalize record fields.
2. `saveTodos` serializes the whole value it receives with `JSON.stringify`
   (`src/cli.mjs:27-29`). A successful mutating command therefore rewrites the
   complete state file.
3. `add`, `done`, `rename`, and `remove` all begin with the value returned by
   `loadTodos`; `list`, `count`, and `completed` only read it
   (`src/cli.mjs:50-116`).
4. The current domain transforms preserve existing record properties when
   changing a record (`completeTodo` and `renameTodo` use object spread), and
   the array transforms retain existing objects (`addTodo` appends and
   `removeTodo` filters) (`src/todolist.mjs:17-27`, `src/todolist.mjs:45-80`).

That last property matters for forward compatibility: an optional or unknown
field survives existing transformations and the subsequent whole-file save.
Any future load-time normalization must retain the same property rather than
projecting records onto a known-field whitelist.

Invalid JSON is a separate concern. The CLI reports it and does not reach a
command or `saveTodos` (`src/cli.mjs:39-48`), so schema evolution should not
silently repair or replace such a file.

## Strategy 1: additive fields with tolerant semantic defaults

Keep the top-level array and add optional fields to todo objects. A reader
interprets a missing field using a documented semantic default; for `due`,
missing means undated. The loader does not eagerly insert `due`, and a save
does not add `due: null` to old records.

### Trade-offs

**Advantages**

- Pre-due files are already valid inputs and retain the same on-disk record
  shape until a user explicitly adds new information.
- TK-005 only needs to create and render the optional field; no coordinated
  file migration is necessary.
- Existing domain transforms preserve optional and unknown record fields, so
  an unrelated `done`, `rename`, or `add` operation need not discard them.
- The change is proportional to an additive, independently defaultable field.

**Costs and risks**

- There is no single version number that tells a reader which optional fields
  may be present. Each field needs a defined absence meaning and validation
  when present.
- The current loader performs no baseline shape validation. Tolerating a
  missing optional field must not be confused with accepting invalid top-level
  or required-field shapes. Shape validation can be added separately while
  preserving the original value and refusing to save invalid input.
- Writers must keep unknown properties. Reconstructing each todo from only
  known fields would make the whole-file save lossy.

## Strategy 2: explicit top-level format version and migration

Replace the array with an envelope such as `{ "version": 2, "todos": [...] }`
and teach `loadTodos` to detect the old array, migrate it, and return the new
model. `saveTodos` would write only the new envelope.

### Trade-offs

**Advantages**

- A version gives migrations an unambiguous dispatch key and supports changes
  whose old and new meanings cannot coexist.
- Validation and unsupported-version errors can be defined per format.
- A deliberately designed migration chain can make breaking changes explicit
  and testable.

**Costs and risks**

- The envelope is a breaking top-level change: every current consumer assumes
  an array. It requires a coordinated dual-reader/migration release rather
  than the small TK-005 field addition.
- The first successful mutation would rewrite a pre-due file into a different
  representation. A migration that selects only known properties could lose
  user or future-writer data.
- Safe migration needs more machinery than a version key: validate before
  writing, preserve unknown data or reject it, write atomically, and retain a
  recoverable original until the replacement succeeds. The current
  `writeFile` path provides none of that migration protocol.
- Versioning does not itself solve optional-field defaults; records inside a
  version still need rules for absent optional values.

## Strategy 3: additive now, version only for breaking changes

Use Strategy 1 for compatible record additions, but establish a trigger for a
future explicit versioned format: introduce a version and migration only when
the top-level representation or the meaning/requiredness of existing data
must change incompatibly.

This keeps the current array as format generation 1 without writing a
synthetic version into it. If a trigger occurs, a separate ticket can design a
dual-reader cutover and lossless migration using real examples of both
formats. The cost is that the initial unversioned array must remain a
permanently recognized legacy input to that future reader.

## Comparison

| Criterion | Tolerant additive fields | Version + migration now | Additive now, version on breaking change |
| --- | --- | --- | --- |
| Pre-due arrays | Load directly and remain undated | Must be detected and migrated | Load directly; remain supported by any future migrator |
| TK-005 scope | Fits directly | Requires persistence redesign | Fits directly |
| Write-time data-loss risk | Low if unknown fields are preserved | Higher until a lossless migration protocol exists | Low now; migration risk deferred to a dedicated change |
| Breaking changes later | Weak without an added mechanism | Strong migration dispatch | Explicitly reserves versioning for that need |
| Complexity now | Low | High | Low |

## Research conclusion

Choose Strategy 3. For TK-005, use tolerant **semantic** defaulting: absence of
`due` means undated, and absence stays absent. Do not add a format version or
run a migration for this additive field. Preserve all existing properties when
transforming and saving records, and reject invalid input rather than
normalizing it destructively.

