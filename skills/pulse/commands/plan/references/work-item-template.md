# `/pulse plan` Work Item Template

Use this template to normalize every execution work item recorded in `.pulse/workgraph/items.jsonl`.

The point is a stable contract that planning, validating, execution, and review can all rely on.

## Canonical shape

```yaml
id: wi-000
title: Implement ...
type: task
story: <story-id>
priority: 1
dependencies: []
files: []
verify:
  - command: <repo verification command>
    expect: exits 0
verification_evidence:
  - kind: artifact
    path: works/epics/<epic-id>-<epic-slug>/<story-id>-<story-slug>/verification/wi-000.md
    note: Captured output from final verification command
testing_mode: standard
decision_refs: []
learning_refs: []
labels: []
```

## Required fields

- `dependencies`
- `files`
- `verify`
- `verification_evidence`
- `testing_mode`
- `decision_refs`
- `learning_refs`

## Validation rules

1. Required fields must always exist.
2. Empty is allowed. Omission is not.
3. `files` must be explicit paths, not vague phrases.
4. `verify` must be runnable checks with expected outcomes.
5. `verification_evidence` must point to explicit artifacts.
6. `testing_mode` must be `standard` or `tdd-required`.
7. If `testing_mode` is `tdd-required`, include explicit red/green `tdd_steps`.
8. `decision_refs` must point to real locked decision IDs.
9. `learning_refs` should include only relevant recalls.

## Spike work items

Spike work items use the same schema with:

- `type: spike`
- one decisive yes/no `spike_question`
- strict timebox
- proof captured at `.spikes/<story-id>/<work-item-id>/FINDINGS.md`

## Rules

1. Do not rely on prose-only scope or verification.
2. Do not invent alternate field names.
3. If a field is empty, write an empty list.
4. If an item cannot be normalized, split or rewrite it before validating.
