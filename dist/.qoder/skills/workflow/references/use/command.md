# `pulse:workflow use`

Session-entry guidance for a repository that uses Pulse. This workflow skill is
advisory: it does not bootstrap, migrate, repair, back up, or write Pulse
state. Use the concrete Rust commands below when an operation is required.

## Rust authority

The Rust CLI and daemon are the only authorities for mutable Pulse state:

- `pulse graph bootstrap --repo-root <repo> --json` initializes the supported
  graph repository layout.
- `pulse graph validate --repo-root <repo> --json` validates existing graph
  state.
- `pulse daemon start`, `pulse daemon status`, and `pulse daemon doctor` manage
  daemon lifecycle and report daemon posture.
- `pulse work list --repo-root <repo> --json`, `pulse work show <id> --repo-root
  <repo> --json`, and `pulse work ready <id> --repo-root <repo> --json` read
  workgraph state.
- `pulse work create`, `pulse work edit`, and `pulse work transition` perform
  supported work mutations with their required command arguments.
- `pulse project`, `pulse workspace`, and `pulse session` operate the daemon
  runtime through its versioned protocol.

Do not substitute hand-edits, shell writes, copied scripts, or guessed command
names for these interfaces. The supported repository graph is rooted at
`.pulse/workgraph/`, with node files under `nodes/`, edge files under `edges/`,
the manifest under `manifest.json`, and embedded schemas under `schemas/`.

## When to run

Use this guidance when:

- entering a repository and the graph or daemon posture is unknown;
- resuming work from approved work artifacts;
- checking whether the current work item is readable and executable; or
- a workflow command reports stale or conflicting evidence.

First inspect the target repository and current work artifacts. If the graph is
missing, ask for permission to run the explicit `pulse graph bootstrap` command.
If the daemon is required, ask for permission to run `pulse daemon start`.

## Resume and handoff guidance

Handoffs are advisory work artifacts. Read the handoff supplied by the current
work boundary, verify its source commit and changed paths, then confirm live
state with `pulse daemon status`, `pulse work show <id> --repo-root <repo> --json`, or `pulse session inspect <id>`
when the relevant identifier is known. Do not claim that this skill creates or
loads hidden repository-local daemon posture.

## Routing outcome

Report:

- the repository and source commit inspected;
- whether graph validation passed or which explicit Rust command is needed;
- the current work item or daemon session, when an identifier is available;
- blockers, stale evidence, and the next `pulse:workflow` command; and
- any command that requires explicit operator approval before mutation.

`pulse:workflow use` chooses the next workflow move. It is not an operational
authority and it must never imply that merely invoking the skill changed the
target repository.
