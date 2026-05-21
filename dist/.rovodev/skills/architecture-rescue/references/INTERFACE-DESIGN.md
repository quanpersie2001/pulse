# INTERFACE DESIGN

Use this checklist when proposing interface changes during architecture rescue.

## Interface quality checks

1. **Small caller contract**
   - Callers should need minimal sequencing, invariants, and failure-policy knowledge.
2. **High-level operations**
   - Prefer intent-level operations over procedural step exposure.
3. **Stable semantics**
   - Keep names and behavior aligned to domain concepts.
4. **Error surface clarity**
   - Make failure modes explicit and actionable.
5. **Migration fit**
   - Define a safe incremental adoption path for existing callers.

## Questions to ask before proposing a new interface

- What caller knowledge does this remove?
- Which repeated policy does this centralize?
- How will tests become simpler at the seam?
- What existing call paths or adapters will need migration?
- What is the smallest deployable step?

## Output shape for interface proposals

- Interface sketch (plain language)
- Responsibilities owned behind the interface
- Caller responsibilities that remain
- Compatibility/migration plan
- Validation notes (what would prove this was better)
