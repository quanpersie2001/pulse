# PULSE.md - seeded by `pulse init` (plan 0022 section 8.1). Human-editable.
# Each profile key below is <surface>-<risk> (a Ticket/Story's own
# `surface`/`risk` fields, e.g. a `ui` Ticket with `risk: medium` resolves
# `ui-medium`) except `decision_work`, which every `role: decision_work`
# Ticket uses regardless of its surface or risk.
fence_ignore: []
# Panel (decision 0027): N independent reviewers for one lane, then reconcile.
#   api-high: {lanes: [...], panels: {review-correctness: {count: 3, quorum: 2}}, human: required}
profiles:
  cli-low: {lanes: [review-correctness]}
  lib-low: {lanes: [review-correctness]}
  api-low: {lanes: [review-correctness]}
  ui-low: {lanes: [review-correctness, qa-ui]}
  api-medium: {lanes: [review-correctness, qa-api]}
  ui-medium: {lanes: [review-correctness, qa-ui]}
  api-high: {lanes: [review-correctness, review-adversarial, qa-api], human: required}
  ui-high: {lanes: [review-correctness, review-adversarial, qa-ui, qa-api], human: required}
  docs-low: {lanes: [check-docs]}
  decision_work: {lanes: []}
