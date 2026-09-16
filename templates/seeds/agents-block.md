## Pulse

Pulse is the local CLI truth layer for work in this repository: one JSONL
store (`.pulse/issues.jsonl`) for Epic/Story/Ticket/Decision records,
receipts as the only proof of completion, an append-only event log, and a
friction -> learning -> check ratchet. Pulse does not run agents and has no
daemon.

Before any mutation, ask: does this id exist (`pulse work show <id>`)? what
is its current status? am I the actor allowed to change it? will this leave
`issues.jsonl` schema-valid?

Route by shape: a small, well-understood change is `pulse work new ticket
"<title>" --risk <low|medium|high> --surface <cli|api|ui|lib|docs>`, then
`pulse work ready <id>`, then `pulse run worker <id>` — the worker itself
reads `.pulse/prompts/worker.md` before `{input}`, so that contract is not
restated here. Anything bigger needs a Story first (`pulse work new story
...`), with an Epic above it if the work doesn't fit under an existing one.

`done` is never a claim, only a gate reading receipts: a Ticket goes
`verifying -> done` only through `pulse close`, after every lane in its
profile has a passing receipt on the handoff's commit.

Hit friction (a Pulse bug, an unclear doc, a missing check)? Record it:
`pulse note <id> "<what happened>" --friction` — don't work around it
silently. Learned something worth keeping from it (a failure, a
constraint, a technique)? `pulse learn add --title "..." --kind
<failure|constraint|technique|routing> --applies-to <glob>` records a
candidate; `pulse learn applicable <id>` shows what already applies to a
Ticket.

Need a doc before writing one? `pulse docs applicable <id>` shows which
docs match a Ticket's anchors/tags; `pulse docs check` finds broken links
and stale generated sections under `docs/`.

Context filling up mid-Ticket? `pulse checkpoint <id> --from <cp.json>`
recording what's done, what's next and any gotchas, then exit printing
exactly `{"status":"continue"}` — the runner resumes in a fresh process
with that checkpoint in the packet.

| Command | Does |
|---|---|
| `pulse work new <kind> <title>` | create a draft record |
| `pulse work show <id>` / `list` / `tree` | read records |
| `pulse work ready <id>` | run the ready gate |
| `pulse work update <id>` / `dep` / `transition` | edit a record |
| `pulse packet <id>` | the one input to read before working |
| `pulse run worker <id>` | dispatch the configured worker |
| `pulse checkpoint <id> --from <f>` | save progress mid-run |
| `pulse handoff <id> --from <f>` | hand off for review |
| `pulse run <lane> <id>` | run one review/qa lane |
| `pulse close <id>` / `close-story <id>` | the only way to `done` |
| `pulse note <id> <text> [--friction]` | append-only note |
| `pulse learn add` / `applicable <id>` | record / recall a learning |
| `pulse docs applicable <id>` / `check` | find relevant docs / doc rot |
