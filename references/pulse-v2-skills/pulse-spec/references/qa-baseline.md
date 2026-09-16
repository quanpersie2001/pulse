# QA baseline

The contract `works/_drafts/<slug>/qa.md` must satisfy, the field vocabulary the parser accepts, and what makes a case survive a refactor.

This file is parsed, not read. `pulse qa baseline <story-id>` turns it into the input `pulse run qa` executes, and hashes both the whole file and each case section into the `qa_checkpoint` receipt. A heading or key outside the vocabulary below is a refusal with a reason code, not a style note.

Nothing validates the file while it lives under `works/_drafts/`: the command resolves a Story *node's* canonical baseline, and during spec no node exists. Check it by hand.

## Document shape

```markdown
# <Story subject> QA baseline — <behavior>

## Scope
<what behavior this baseline protects, in one or two sentences>

## Posture
automated

## Risks
- RISK-LEAK: <what going wrong would look like>

## Exit criteria
- <condition under which the Story may close>

## Cases

### QA-001 <case title>
- Intent: <what an observer outside the system sees>
- Surface: api
- Priority: high
- Risks: RISK-LEAK
- Preconditions:
  - <state that must hold before the steps>
- Steps:
  1. <action>
- Expected:
  - <observable result>
- Evidence: stdout
```

`## Posture` takes exactly one of `automated`, `hybrid`, `manual_structured`, `static_proof`, `not_applicable`.

`required` is **not** a posture. It is the Ticket-level QA *impact* value written in `ticket.md`, and putting it here is the most common way this file gets refused.

## Case field vocabulary

Every field line is `Key: value`, optionally continued by indented lines beneath it. The keys are fixed:

| Field | Required | Notes |
|---|---|---|
| `Intent` | yes | What is observed, in behavior terms |
| `Surface` | yes | `cli`, `api`, `ui`, `job` or `docs` |
| `Priority` | — | `high`, `medium`, `low` |
| `Applicability` | — | `required` by default; `not_applicable` needs a `Reason` |
| `Reason` | when not applicable | Why this case does not apply |
| `Risks` | — | `RISK-*` identifiers declared in `## Risks` |
| `Preconditions` | — | State before the steps |
| `Steps` | yes | Actions, in order |
| `Expected` | yes | Observable results |
| `Evidence` | — | What the run should leave behind |

Anything else fails as `qa_baseline_unknown_field`, naming the line. A helpful extra key is a parse error, so put the extra thought into `Intent` instead of inventing a field for it.

A case missing its title, `Intent`, `Steps` or `Expected` is refused.

## The `pulse-check` block

An optional fenced block that makes a case mechanically provable. Without it, a script runner returns `inconclusive` for that case rather than guessing.

````markdown
```pulse-check
run: node scripts/qa/refresh-expired.mjs
assert:
  - exit_code: 0
  - stdout_json_path: {path: "$.code", equals: "TokenExpired"}
```
````

Rules the parser enforces:

- At most **one** block per case, and only on surface `cli` or `api`.
- `run:` is one line, split into argv with quote handling. **No shell.** A `|`, `&&`, `;` or redirect is refused as `qa_check_shell_operator` — split it into separate cases or move the composition into the script.
- At least one assertion is required.
- Optional keys: `cwd`, `env`, `stdin`, `timeout_seconds`.

The assertion vocabulary is closed:

| Assertion | Form |
|---|---|
| `exit_code` | integer |
| `stdout_line` | string |
| `stdout_contains` | string |
| `stderr_contains` | string |
| `file_unchanged` | path |
| `stdout_json_path` | `{path: "$.code", equals: "TokenExpired"}` |
| `file_contains` | `{path: "…", text: "…"}` |

`file_unchanged` is the one to reach for when the point of the case is that something was *not* touched — a case that only asserts success cannot catch a fix that works by overwriting a neighbour.

## Writing cases that survive

Cases come from the success signals in `story.md`, not from the approach. The approach will be refactored; the behavior is what the Story promised.

- **Name what an outsider sees.** A status code, an error identifier, a file that appeared, a row that did not. Not a function, a class, a selector or an internal structure.
- **One behavior per case.** A case that asserts four unrelated things reports one failure and hides three.
- **Expected is observable.** "Works correctly" cannot fail. "HTTP 401 with `body.code = TokenExpired`" can.
- **Write the negative case.** The behavior that must *not* change is the half that catches a fix which passes by breaking something adjacent.
- **Cover the exceptions.** Each `E-*` the Story cites should have a case, or a stated reason it has none.

## The boundary with Ticket verification

| | `qa.md` | `ticket.md` `## Verify` |
|---|---|---|
| Question | Does the behavior this Story promised still hold on this snapshot? | Does this Ticket satisfy its technical contract? |
| Owner | Story | Ticket |
| Written by | `pulse-spec` | `pulse-planning` |
| Receipt | `qa_checkpoint`, `story_close` | `handoff`, `verification` |

The same command can serve both. They differ in intent, actor and what the receipt binds — and only the first belongs in this file.
