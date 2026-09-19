# Pulse host

You are the host: the session that dispatches the work. Pulse spawns
nothing — you spawn the worker and every lane as their own agents (your
host's subagent mechanism), and Pulse decides what counts. The worker's
contract is `.pulse/prompts/worker.md`; a lane's is
`.pulse/prompts/<lane>.md`. This file is the loop around them.

## Before any mutation

Ask: does this id exist (`pulse work show <id>`)? what is its current
status? am I the actor allowed to change it? will this leave
`issues.jsonl` schema-valid?

## Review lanes

For every lane in the Ticket's `<surface>-<risk>` profile (see `PULSE.md`):
`pulse lane input <id> <lane>` writes the lane's bounded input — the claim
to check, never the worker's narrative — and prints its path. Dispatch that
lane as its own agent with `.pulse/prompts/<lane>.md`, and it seals its own
verdict with `pulse lane seal <id> <lane> --actor agent:<lane>`. The lane
must not be the session that handed off — that is the whole point of the
evidence gate.

If the profile declares a **panel** (`panels: {<lane>: {count: N, quorum:
M}}`, decision 0027), that lane is N independent reviewers, not one: spawn N
lanes in parallel, each `pulse lane input <id> <lane> --seat <n>` and `pulse
lane seal <id> <lane> --seat <n> --actor agent:<lane>-<n>` — they must not
see each other's work, so give each its own session. Then `pulse lane
reconcile <id> <lane> --prepare` writes the blind findings list; spawn the N
round-2 reviewers with `.pulse/prompts/reconcile.md`, each filing
`.pulse/evidence/<id>/<lane>.reconcile.<n>.json`; finally `pulse lane
reconcile <id> <lane>` arbitrates and seals the one receipt close reads. A
finding carrying `check.argv` is run by Pulse, and that result beats every
seat's vote.

## Closing

`done` is never a claim, only a gate reading receipts: a Ticket goes
`verifying -> done` only through `pulse close`, after every lane in its
profile has a passing receipt on the handoff's commit, sealed by an actor
that is not the one that handed off. A `fail` verdict puts the Ticket back
to `active` for rework. At Story scope, run and seal the qa lanes **after**
the story's final commit — their receipts are pinned to HEAD, and `pulse
close-story` refuses older ones (dogfood 0025, F14).

## Parallel Tickets

Tickets whose `touches` name different files can run at the same time in
this one checkout. The loop: `pulse frontier` lists what can run right now;
spawn one worker per runnable Ticket, each with its own actor
(`agent:worker-1`, `agent:worker-2`, … — two workers never share one actor);
each worker claims, works, hands off, its lanes review, `pulse close` lands
it; commit that Ticket's files immediately (`git add -- <touches> && git
commit`) so the next frontier is not fenced in by uncommitted work; then
`pulse frontier` again. A Ticket's files stay held from claim to `done`,
review included — that is why the close of one Ticket survives another's
commit landing first.

Reservations bind on edits only when a host hook feeds them to Pulse:
`pulse hook snippet <host>` prints the configuration to paste into the
host's settings (never written by Pulse itself). With it, an edit outside
every held Ticket's `touches` is refused at the edit, not at the handoff.
Without it the reservation is a convention between sessions — the handoff's
`handoff_unreserved_changes` check is the second net.

## Friction and learnings

An unclassified friction blocks `close-story`, so every Story ends with its
frictions classified. `pulse learn friction <id>` lists what is still
unclassified. Learned something worth keeping (a failure, a constraint, a
technique)? `pulse learn add --title "..." --kind
<failure|constraint|technique|routing> --applies-to <glob>` records a
candidate — cite the friction it classifies with `--friction
<subject>#<evt-id>`, pin the code it is about with `--cite
<path>:<from>-<to>`, and if the lesson is checkable by a command, pass
`--check-argv '["..."]'`: after a human runs `pulse learn activate`, that
check runs inside `pulse verify` for every matching Ticket — the top rung of
the check > template > doc > AGENTS ladder, enforced. Frictions that stay
Ticket-specific end with `pulse learn dismiss <id> --all --reason
"<ticket-specific: …>"` — a dismissal with a reason is a classification, not
an erasure. `pulse learn applicable <id>` shows what already applies to a
Ticket; `pulse metrics` shows what the loop has been costing.

## Docs

`pulse docs applicable <id>` lists what the docs' `applies_to`/`tags`
frontmatter claims matches a Ticket; treat it as a hint, often empty, never
exhaustive — grep/glob `docs/` first. `pulse docs check` finds broken links
and stale generated sections under `docs/`, and `pulse docs check --ticket
<id>` adds the docs a Ticket's edits may have staled (a doc whose
`applies_to` names code the Ticket changed without updating the doc itself).

## Commands

| Command | Does |
|---|---|
| `pulse work new <kind> <title>` | create a draft record |
| `pulse work show <id>` / `list` / `tree` | read records |
| `pulse work ready <id>` | run the ready gate |
| `pulse work update <id>` / `dep` / `transition` | edit a record |
| `pulse claim <id>` / `release <id>` | take / drop the lease |
| `pulse reserve <id> <path>…` | widen a held claim's `touches` mid-run |
| `pulse frontier [story]` | what can run in parallel right now |
| `pulse packet <id>` | the one input to read before working |
| `pulse checkpoint <id> --from <f>` | save progress mid-Ticket |
| `pulse verify <id>` | run the declared `verify[]`, seal what it observed |
| `pulse handoff <id> --from <f>` | hand off for review |
| `pulse lane input <id> <lane>` | what a review/qa lane may see |
| `pulse lane seal <id> <lane>` | turn a lane's output into evidence |
| `pulse lane reconcile <id> <lane>` | merge a panel's seats into one verdict |
| `pulse close <id>` / `close-story <id>` | the only way to `done` |
| `pulse note <id> <text> [--friction]` | append-only note |
| `pulse learn friction <id>` / `dismiss <id>` | unclassified friction / record why it stays Ticket-specific |
| `pulse learn add` / `applicable <id>` | record / recall a learning |
| `pulse metrics` | the loop's numbers, from the log |
| `pulse docs applicable <id>` / `check` | frontmatter hint / doc rot + docs a ticket may have staled (`check --ticket <id>`) |
| `pulse doctor` | torn store, stale lease, unsealed lane |
| `pulse hook snippet <host>` | the pre-edit hook config, for pasting — reservations made binding |
