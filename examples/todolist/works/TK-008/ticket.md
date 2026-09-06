# TK-008 Print usage from an explicit help command

## Objective
Give scripts and humans a discoverable, successful way to see usage:
`node src/cli.mjs help` (and a `--help` flag) prints the usage text to
stdout with exit code 0.

## Current behavior
Usage text only appears on stderr as the failure path for unknown commands
or missing arguments, always with exit code 2. `help` itself is an unknown
command today.

## Target behavior
- `node src/cli.mjs help` prints the usage text to stdout and exits 0.
- Any invocation containing `--help` or `-h` as its first flag prints the
  same usage text to stdout and exits 0, before any other parsing.
- Unknown commands keep the current behavior: usage on stderr, exit 2.

## Code anchors
- src/cli.mjs

## Required changes
- Add a `help` case and a `--help`/`-h` pre-check to the CLI.
- Keep the usage text itself in one place (no divergence between stdout
  and stderr variants).
- Document `help` in the behavior doc's usage notes.

## Invariants
- The stderr usage path for genuine misuse stays exit 2.
- No new dependencies.

## Implementation freedom
guided: output stream, exit codes, and the single-source usage text are
fixed; placement of the pre-check is the worker's.

## Scope
- CLI help paths, behavior doc note.

## Non-scope
- Per-command help pages, man pages, colored output.

## Acceptance
- AC-1: `help` prints usage to stdout and exits 0.
- AC-2: `--help` (anywhere as a flag) prints usage to stdout and exits 0
  without executing any command.
- AC-3: An unknown command still prints usage to stderr and exits 2.

## Verify
- node scripts/verify.mjs

## Open questions
- (delegated) Whether `--help` detection happens before or after command lookup, as long as it never executes a command.

## Documentation impact
- Posture: required
- Documents: DOC-TODOLIST-BEHAVIOR
- Required update: mention `help` / `--help` in the usage section.

## QA impact
- Posture: none
- Rationale: pure output-shape change covered by focused verify tests; no
  behavioral baseline applies.

## Expected handoff
- Diff, `node scripts/verify.mjs` result, AC to check mapping, updated
  behavior doc.
