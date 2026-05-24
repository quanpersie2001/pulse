# Product Docs

Product docs are domain-focused product contracts. Name files by real domains that exist in the product or workflow, for example:

- `overview.md`
- `billing.md`
- `workflows.md`
- `permissions.md`
- `api-conventions.md`

Do not create domain files just to fill the folder. Empty structure is better than fake product truth.

## Update Rule

When behavior changes:

1. Update the affected product doc.
2. Update or create the story artifacts under `works/`.
3. Record validation expectations in `plan.md` and validation artifacts.
4. Record a decision if the change affects architecture, scope, risk, source-of-truth hierarchy, or a previously settled product rule.
