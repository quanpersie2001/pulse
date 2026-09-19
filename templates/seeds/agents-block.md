## Pulse

Pulse is the local CLI truth layer for work in this repository: one JSONL
store (`.pulse/issues.jsonl`) for Epic/Story/Ticket/Decision records,
receipts as the only proof of completion, and an append-only event log.
Pulse runs no agents and has no daemon. **You** dispatch the work, Pulse
decides what counts.

### The path

Answers, explanations, reviews of a diff and status reports are read-only:
no record, no claim. Everything else walks this path, entering at the step
that fits the request.

1. **Shape** — the work is bigger than one bounded change, or two people
   would both deliver it reasonably and differently. The skill
   `.agents/skills/pulse-shape/` runs one interview and writes the Story.
   An effort whose route is not yet visible gets an **Epic** first: the
   Epic is the map (destination, success signals, what is out of scope,
   what is still fog), its Stories are the legs.
2. **Plan** — a shaped Story becomes Tickets:
   `.agents/skills/pulse-plan/`. Each Ticket is one tracer bullet with its
   `touches`, `verify[]` and acceptance, taken through `pulse work ready
   <id>`.
   A small, well-understood change skips 1–2 entirely: `pulse work new
   ticket "<title>" --risk <low|medium|high> --surface
   <cli|api|ui|lib|docs>`, then `pulse work ready <id>`.
3. **Work** — `pulse claim <id>`, read `pulse packet <id>` (the one input),
   implement, `pulse verify <id>`, `pulse handoff <id>`. Contract:
   `.pulse/prompts/worker.md`.
4. **Review** — every lane in the Ticket's `<surface>-<risk>` profile
   (`PULSE.md`) is its own agent: `pulse lane input <id> <lane>` gives it a
   bounded input, `pulse lane seal <id> <lane>` records its verdict.
   Contract: `.pulse/prompts/<lane>.md`. Work and review are never the same
   session — that is the whole point of the evidence gate.
5. **Close** — `pulse close <id>` is the only way to `done`: it reads
   receipts sealed by an actor other than the one that handed off. Commit
   that Ticket's files right away, then take the next from `pulse
   frontier`.
6. **Finish** — `pulse close-story <id>` once the Story's qa lanes ran
   against the committed tree; `pulse close-epic <id>` once its Stories are
   done and its fog has graduated into a Story or out of scope.
7. **Learn** — `.agents/skills/pulse-learn/` turns the frictions recorded
   along the way into one learning and one intervention. Run it after a
   Ticket reaches `done`, a Story closes, or work is cancelled with
   lessons.

Dispatching workers and lanes, review panels, parallel Tickets and the full
command table: `.pulse/prompts/host.md`. Read it before you orchestrate; a
worker or a lane does not need it.

### Along the way

- Hit friction — a Pulse bug, an unclear doc, a missing check? `pulse note
  <id> "<what happened>" --friction`, and keep going. Working around it
  silently is how the same cost gets paid twice.
- Context filling up mid-Ticket? `pulse checkpoint <id> --from
  .pulse/runtime/cp-tk-<id>.json`, then stop. The next session resumes from
  `pulse packet <id>`.
- Store, lease or lane looks wrong? `pulse doctor`.
- No skills under `.agents/skills/`? `pulse skills install`.
