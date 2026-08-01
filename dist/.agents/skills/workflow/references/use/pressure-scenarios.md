# Pressure scenarios: `pulse:workflow use`

These scenarios test that guidance remains advisory and routes mutations to
concrete Rust commands.

1. **Graph absent.** Inspect the repository and ask before running
   `pulse graph bootstrap --repo-root <repo> --json`.
2. **Graph invalid.** Report the result of
   `pulse graph validate --repo-root <repo> --json`; do not repair files by hand.
3. **Daemon unavailable.** Ask before running `pulse daemon start`; do not write
   an undocumented runtime status record.
4. **Work item unclear.** Use approved artifacts and
   `pulse work list --repo-root <repo> --json`; do not invent IDs or materialize
   work without the required approval and Rust command.
5. **Handoff supplied.** Read it as an advisory work artifact, verify the source
   commit, and confirm live state with `pulse session inspect <id>` when known.

In every case, `pulse:workflow use` reports evidence and recommends the next
workflow command. It does not bootstrap, migrate, back up, rebuild, or write
Pulse state.
