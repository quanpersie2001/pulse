# Review Runtime Appendix

## 4+1 review model

Run five focused passes:

1. behavior correctness
2. regression and boundary integrity
3. security and misuse risk
4. evidence integrity
5. release-readiness synthesis

## Severity policy

- **P1**: mandatory blocker; Gate 4 fails
- **P2**: serious but can proceed only with explicit owner plan
- **P3**: quality debt with bounded risk
- **P4**: minor improvement

## Gate 4 contract

Gate 4 passes only when:

- no P1 remains
- required evidence is fresh and sufficient
- acceptance/UAT posture is explicit

Gate 4 fails when any P1 exists or required evidence is missing.

## Evidence freshness checks

For each closed item, verify:

- evidence file exists at declared `verification_path`
- includes command list and observed outputs
- corresponds to this reviewed change set
- unresolved gaps are explicit

## UAT routing

- default: run acceptance checks
- skip only with explicit instruction and compensating evidence
- UAT failure routes to `pulse:workflow execute` with targeted fix items

## Closeout outputs

Publish:

- Gate 4 decision (pass/fail)
- blocking item IDs (if any)
- severity inventory by owner
- next command recommendation
