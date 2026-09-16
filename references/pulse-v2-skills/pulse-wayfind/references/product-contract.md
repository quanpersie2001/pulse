# Product contract

Use one contract for one coherent product area. Split only when different owners or release boundaries would otherwise make one file ambiguous.

## Required Markdown shape

```markdown
# <Product area>

## Requirement Overview

### Destination
<One or two sentences describing the observable end state.>

### Context
<Why this matters and who experiences it.>

### In scope
- <Included behavior>

### Out of scope
- <Excluded behavior and why it is outside this destination>

### Glossary
- `<term>` — <meaning here, or a reference to DOC-GLOSSARY>

## Business Rules

- BR-01: <One testable rule that resolves to exactly one authoritative behavior.>

## Exception Scenarios

- E-01
  - Trigger: <observable condition>
  - User-visible outcome: <message or behavior>
  - Error code: `<stable-code>`

## Open Questions

### Open
- (blocking; owner: <actor>; type: grilling|prototype|research|task) <question or still-imprecise fog>

### Decisions so far
- <Decision or closed decision-work title and reference> — <one-line gist only>

### Ruled out
- <excluded option or closed question and reference> — <why it is outside the destination>
```

Keep these four `##` sections even when a subsection is empty. This makes product contracts predictable without turning their prose into another lifecycle system.

## Stable IDs

- Assign `BR-*` and `E-*` identifiers monotonically within the document.
- Never reuse or renumber an identifier after another artifact may have cited it.
- Mark a dead entry `withdrawn` and preserve its number.
- Make each rule independently testable. Split a rule that contains unrelated outcomes.
- Make each exception name its trigger, user-visible outcome, and stable error code.
- R2/R3 Ticket acceptance will cite the relevant `BR-*` and `E-*`; R0/R1 work does not inherit that ceremony.

## One owner for detail

The product contract is an index at low resolution:

- state product behavior in `BR-*` and `E-*`;
- keep trade-off detail in an accepted Decision;
- keep investigation evidence in the owning research file;
- keep execution acceptance in the Ticket;
- link and gist those owners under Decisions so far instead of copying them.

A contradiction between an accepted Decision and an approved product contract is not resolved by editing whichever file is easier. Surface it to the human because both claim intent authority.

## Registration record

Create a temporary JSON file matching this shape, replace every placeholder, then register it through the command in `SKILL.md`:

```json
{
  "id": "DOC-PRODUCT-<AREA>",
  "revision": 1,
  "path": "docs/product/<area>.md",
  "kind": "product",
  "status": "draft",
  "owner": "<actor>",
  "summary": "<one sentence describing the behavior contract>",
  "scope": {
    "paths": []
  },
  "tags": [],
  "generated": null,
  "superseded_by": null
}
```

Use `approved` only after the human confirms the destination and current rules. An approved document may retain explicitly deferred questions, but it must not hide a question that could change an existing rule or exception.
