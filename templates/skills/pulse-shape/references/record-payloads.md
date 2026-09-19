# Record payloads

Every payload `pulse-shape` writes, in creation order. Write each to a temp
file and seed from it; record updates merge shallowly, so send complete
arrays, never diffs. Every mutating command carries an actor
(`--actor human:<name>`, or `PULSE_ACTOR` in the environment).

```text
pulse work new decision "<title>" --from decision.json --actor human:quan --json
pulse work new epic  "<title>" --from epic.json  --actor human:quan --json
pulse work new story "<title>" --epic EP-<id> --risk medium --surface api --from story.json --actor human:quan --json
```

## Decision

One per decision that outlives the interview, created **first** so the
Story can cite its `DEC-…` id. A decision that was merely a clarification
needs no record — it lands in the rule it settled.

```json
{"question":"…","options":["…","…"],"decision":"…",
 "consequences":"…","context":"markdown: what forced the question"}
```

Each one also gets `docs/decisions/<DEC-id>-<slug>.md`; `pulse close-story`
refuses a Story citing a decision no file there names.

## Epic

Only when the work does not fit under an existing Epic. The ready gate
requires `outcome` and at least one `success_signals[]` entry.

```json
{"outcome":"the destination this effort is heading to",
 "success_signals":["how anyone tells the destination was reached"],
 "out_of_scope":["ruled out; closed, never graduates back in"],
 "not_yet_specified":["in-scope fog you can see but cannot shape yet"]}
```

`not_yet_specified[]` is a debt, not a note: `pulse close-epic` refuses
while any item remains. Each graduates into a Story that shaped it, or
moves to `out_of_scope[]` when the effort decided against it.

## Story

```json
{"outcome":"…",
 "rules":[{"id":"TAG-BR-1","text":"…"}],
 "exceptions":[{"id":"TAG-E-1","text":"…"}],
 "context":{"decisions":["DEC-ab12"]},
 "approach":"tracer-bullet sketch; SHOULD exist when risk >= medium",
 "qa_cases":[{"id":"QA-001","intent":"…","surface":"api","priority":"high",
              "steps":["…"],"expected":["…"]}],
 "open_questions":[{"q":"…","disposition":"resolved","answer":"…","ref":"DEC-ab12"}]}
```

Field rules:

- **`rules[].id` / `exceptions[].id`** are capability-scoped
  (`TAG-BR-1`, `TASK-E-2`), numbered within the capability across every
  Story, so an id is unique in the repo and stays true after this Story
  closes. Read `docs/product/<capability>.md` first and continue its
  numbers. A Story that changes an existing rule carries that rule's id
  with the new text.
- Every rule/exception id must, by story close, appear in at least one file
  listed on the Story's `docs_written` (`docs/**`) — the gate refuses a
  Story whose rules live only in `issues.jsonl`, so plan the doc that will
  carry them.
- **`qa_cases[].priority: high`** gates `close-story`: a high case must be
  covered by a passing story-scope `qa-*` receipt.
- **`qa_cases[].check`** (`{"argv":[…],"assert":[{"exit_code":0}]}`) is
  written only when a mechanical oracle already exists, or is a small
  `scripts/qa/cases/` script — the QA oracle is harness, not product code,
  and a check beats an interview. Otherwise omit it and let the qa lane's
  agent cover the case.
- **`open_questions[].disposition`** is one of
  `resolved|rejected|delegated|deferred|blocking`. A `blocking` question
  means the shape is not done and the ready gate refuses the Story.

## Fix-ups

```text
pulse work update <id> --from fix.json --actor …
```

`revision` moves under you, so re-read with `pulse work show <id> --json`
before every update.
