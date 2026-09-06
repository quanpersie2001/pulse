# Friction log — Track B dogfood

Every entry is a real friction hit while running Tickets TK-003..TK-008
through Pulse. One line each: what hurt, where, and what would fix it.
Feeds the v0.2 backlog (PRODUCT.md §11) after the round.

## 2026-09-06 (shaping phase)

- `work sync` rejects any wrapped line under `## Open questions` with
  `ticket_brief_open_question_disposition_missing`. The error names the
  disposition rule, but the actual problem is the continuation line of a
  wrapped bullet. Cost: one failed sync per ticket (6/6 failed on first
  sync). Fix shape: parser should skip/merge continuation lines, or the
  error should point at the offending line.
- `work ready --profile` looks like it takes a PULSE.md verification
  profile (`module-change`); it actually wants a readiness profile and
  only `contract_readiness` exists. The flag collides with a documented
  concept that lives elsewhere. Fix shape: rename the flag or support the
  verification profile namespace it implies.
- `authority.json` must be byte-canonical (sorted keys, pretty, exact
  trailing newline) or every readiness check reports
  `readiness_policy_invalid` with no hint about canonicality. Nothing in
  the CLI writes or repairs this file; it drifted by ordinary editing.
  Fix shape: `pulse policy normalize` command, or a reason code that says
  "not canonical" (the code exists: `readiness_policy_not_canonical` —
  it just never surfaced to me).
- Actor syntax is implicit: `--actor quanpersie2001` silently becomes
  `system:quanpersie2001` and fails grant checks; the working form is
  `--actor human:quanpersie2001`. The error shows the normalized actor,
  which is how I guessed the prefix. Fix shape: parse `kind:id` strictly
  and reject bare ids, or default bare ids to the developer principal.
- `draft -> shaped` needs `--reason-code` AND `--reason` together but the
  first attempt with only `--reason` fails with `missing_status_reason`
  after I already spent a revision bump cycle on `--expected-revision`.
  Friction is small but the flag interdependency is undocumented.
