# Discovery Probe Bank

Use these probes during `pulse:workflow explore` to identify evidence needs and decision surfaces for later solution design.

Explore probes produce research questions and findings. They do **not** produce final design decisions.

Select only probes that are genuinely relevant to the confirmed work boundary.

---

## ARTIFACTS — Story and workflow context

- What does `intake.md` say about boundary, lane, risk flags, affected surfaces, and routing?
- What direction, constraints, and assumptions does `work-brief.md` approve?
- Do existing story artifacts contradict the current direction?
- Are prior references or discovery outputs still valid?
- What artifact obligations must later workflow honor?

---

## CODE — Existing implementation behavior

- What entry points implement related behavior today?
- Which modules/classes/functions are reusable surfaces versus risky coupling?
- What established patterns appear repeatedly?
- Where are similar flows implemented?
- Which call sites would be affected by likely solution options?
- What code evidence contradicts the story direction?

---

## TESTS — Verification landscape

- What existing tests cover related behavior?
- Which fixtures, helpers, or harness commands are relevant?
- What behavior is currently untested?
- What regression evidence will later design need to account for?
- Are tests documenting existing behavior that conflicts with the proposed direction?

---

## DATA — State, schema, and ownership

- What entities or persistence shapes already exist?
- What ownership or tenant boundary exists today?
- What migration history constrains future design?
- Which indexes/constraints/public data contracts matter?
- What data-loss, backfill, privacy, or isolation risks are visible?
- Is external research needed for partitioning, tenancy, retention, or compliance patterns?

---

## RUNTIME — Operations and workflow behavior

- What daemon posture, CLI command, or process currently owns the behavior?
- What state mirrors or graph projections must remain aligned?
- What failure/recovery paths exist today?
- What concurrency, locking, reservation, or idempotency behavior matters?
- What operational evidence is missing?

---

## DOCS / PUBLIC CONTRACTS

- What documented behavior exists today?
- Are docs, README, API reference, or product contracts stale or contradictory?
- What public compatibility constraints must later design honor?
- Are examples or usage snippets part of the accepted contract?

---

## EXTERNAL — Deep-research triggers

Invoke deep research when these cannot be answered from repo evidence:

- Provider/API behavior, limits, retries, auth, or error semantics
- Library/framework trade-offs or current best practice
- Security/privacy/compliance requirements
- Architecture/scaling strategies
- Multi-tenant/data partitioning patterns
- Domain/product conventions or current market state

For each external research need, define:

- research question
- why it matters for this story
- expected output file: `references/<topic-slug>.md`

---

## Decision Surface Prompts

Use these after evidence collection:

- What later design decision would be unsafe without this evidence?
- Which candidate options are supported or weakened by the evidence?
- What contradictions must design resolve?
- What questions block solution design?
- What questions can safely defer beyond design?
- What evidence confidence should design know: high, medium, or low?

---

## Red Flags

- Probe output states a final solution decision instead of a finding.
- Probe output contains task breakdown or implementation sequence.
- External claims lack citations or saved reference reports.
- Repo claims lack file paths or concrete artifacts.
- A design-critical contradiction is hidden as an assumption.
