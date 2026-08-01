# Pulse handoff guidance

Handoffs are human-authored work artifacts that preserve context across
workflow commands. They are not a repository-local Pulse runtime database.

## Minimum content

```yaml
surface: pulse:workflow
active_command: plan
source_commit: <commit>
changed_paths:
  - <path>
summary: <what is true now>
next_action: <smallest safe next action>
read_first:
  - <work artifact>
blockers:
  - <blocker or none>
```

Keep one handoff per owning work boundary and preserve the source commit and
changed paths. A later operator must verify current state with concrete Rust
commands such as `pulse daemon status`, `pulse work show <id> --repo-root
<repo> --json`, or `pulse session inspect <id>` when an identifier is known.

The workflow skill may draft or summarize this note, but it must not create a
runtime manifest, write a hidden state file, or claim that a handoff changed
Pulse canonical state. When a daemon session handoff is required, use the
existing `pulse session handoff` command with its required arguments and
operator approval.
