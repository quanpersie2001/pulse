# Pulse — Roadmap

> Rewritten at the v3 close-out (P3.4, tag `v0.0.1`). `SPEC.md` says what Pulse is; this file says
> what comes next and, more importantly, what is deliberately not next.

## Shipped

`v0.0.1` — the thin harness (plan
[`0022`](docs/plans/0022-thin-harness.md)): JSONL store, ready gate, worker
loop with checkpoints, lanes with sealed evidence, close gates, learnings,
docs routing, four skills, `pulse serve` (Decision 0023), `pulse doctor`,
persisted run output (Decision 0024). Dogfooded end-to-end in
`~/Workspace/Personal/todolist` through ST-1/ST-2 (golden path twice,
friction/ticket that is a Pulse bug: 0.5 at ST-2) — see
[`docs/plans/0022-dogfood-st1.md`](docs/plans/0022-dogfood-st1.md).

## Now

- **Dogfood continues** on the todolist target: ST-3 (Google OAuth, first
  `*-high` story) is shaped and planned; the run waits on the owner's real
  OAuth credentials and the `human: required` gate. Every friction lands as
  `pulse note --friction`; the learning loop (`pulse-learn`) turns them
  into checks.
- **Metrics distance, recorded not hidden** (see
  [`0022-metrics.md`](docs/plans/0022-metrics.md)): `src/` sits above the
  < 10000 target; surfaced error codes sit at 51 against < 40
  ([audit](docs/plans/0022-error-code-audit.md)). Both need deliberate
  cuts, not sweeps — owner decisions.

## Later (only with evidence)

Everything runs through the plan's stop conditions: no new mechanism
without ≥ 2 frictions of the same kind recorded in the dogfood target's
`.pulse/events`. The frozen list: docs search, parallel worktrees,
knowledge relations, authority grants, receipt signatures, an MCP server,
reviewer ≥ 2 by default, materialization.

## Never (without a new decision)

- A daemon, a network service, or any write path in `pulse serve`.
- Anything that breaks the source fence or the receipts-only completion
  standard — the refusal *is* the product.
- Implementing from `PRODUCT.md` (v2 history) or the deleted v2 design
  (graph module, works/ prose trees, static board).
