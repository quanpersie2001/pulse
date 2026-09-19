# Docs map

Hand-maintained index of durable docs, kept short on purpose.
`pulse docs check` finds broken links and stale generated sections, and
confirms every path listed below still exists; with `--ticket <id>` it also
reports docs a Ticket's edits may have staled.

## What goes where

| Directory | Holds | Never holds |
|---|---|---|
| `product/` | How the product behaves **now**, one file per capability: purpose, business rules and exceptions by id (`TAG-BR-1`, `TAG-E-1`), user and caller flows. | Story/Ticket ids, decision history, QA cases, "what changed in ST-x" — the backlog lives in `.pulse/issues.jsonl`. |
| `decisions/` | Why: one file per Decision record, named `<DEC-id>-<slug>.md` — question, options, decision, consequences. `pulse close-story` refuses a Story citing a decision no file here names. | Behavior. A decision points at the rule it shaped; the rule's text lives in `product/`. |
| `architecture/` | How the system is built: `overview.md` (context, components and allowed dependencies, main flows, invariants) plus one file per area (`data-model.md`, `api-conventions.md`, …). | Product rules, run instructions. |
| `domain/` | `glossary.md` — one line per term. | |
| `operations/` | How to run, migrate and deploy. `run.md` is read by the `qa-*` lanes. | |

Frontmatter `applies_to` on a doc means "this doc DESCRIBES this code":
Pulse compares it against the files a Ticket changed and warns at handoff
when described code moved but the doc did not (`docs_maybe_stale`, plan
0025 F3). Keep the globs pointed at the exact files the doc explains — a
directory-wide glob (`api/**`) makes every ticket touching that directory
warn about a doc it never needed (the 0025 dogfood measured 0 correct out
of 3 warnings, all from one `api/**` doc), while a file-level glob warns
only when the described file really moved. Its secondary
use — `pulse docs applicable <id>` suggesting the doc for a Ticket's
anchors/tags — is just that, a hint that is often empty and never
exhaustive: grep/glob `docs/` first.

## Index

- (add entries as `- path/to/doc.md` plus a one-line why)
- docs/architecture/overview.md — the map of the system; start here
- docs/operations/run.md — lane qa-* reads this file; format in
  scripts/qa/README.md
