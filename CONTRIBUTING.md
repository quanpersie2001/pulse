# Contributing

Pulse is developed as one Rust executable with an offline Core and a
host-local daemon. The repository does not package an agent workflow router,
plugin, or standalone agent skills.

## Repository truth

The current implementation and its contracts live in:

- `src/bin/pulse.rs`: minimal executable adapter;
- `src/cli/`: command parsing and output rendering;
- `src/kernel/`: cross-domain Core composition;
- `src/graph/`, `src/docs/`, `src/evidence/`, and `src/knowledge/`: offline domain owners;
- `src/daemon/`: the sole host-local runtime lifecycle authority;
- `tests/`: architecture, contract, integration, recovery, and reliability coverage;
- `PULSE_REBOOT.md` and `pulse-reboot/`: product direction and detailed design owners;
- `AGENTS.md`, `README.md`, and this file: repository operating contracts.

Historical proposals explain accepted slices but do not override current
source, tests, or owning reboot documents.

## Ownership and dependency direction

- The binary delegates to the `pulse::cli` facade.
- CLI owns transport and rendering, not domain or provider semantics.
- Core commands operate without the daemon.
- Daemon owns Project, Workspace, Session, Provider, process, timeline, effect,
  assignment, and recovery runtime state.
- Core never imports daemon.
- Future Orchestration may compose Core and Runtime but may not replace either
  authority.

Preserve stable public paths deliberately. Keep new surfaces private by
default, and update architecture/public-path tests whenever an intentional
contract change requires it.

## Target-repository boundary

This repository develops Pulse but is not enrolled as a Pulse-managed target.
Do not bootstrap or mutate Pulse workgraph, evidence, docs-registry, or
lifecycle state with `--repo-root .`.

Integration tests must copy a tracked target fixture through
`tests/common/fixture_repo.rs::TestRepo::from_fixture` and run Pulse against the
temporary copy. Manual smoke tests follow the same pattern.

## Change workflow

1. Read the owning source, tests, and design document.
2. Keep changes scoped to the owning module and preserve recovery/order
   invariants at effect boundaries.
3. Add focused coverage in the existing domain integration crate.
4. Update contract documentation when public behavior or ownership changes.
5. Run narrow tests first, then the repository reliability gates.

For daemon changes, retain authorization ordering, idempotency checks,
failpoint placement, durable intent before external I/O, and fail-closed
uncertainty handling. Do not introduce a second application facade or state
store.

## Validation

Before handoff, run:

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

`cargo test --all-targets` must pass with default threading. Do not lower test
threading to conceal races or global-state collisions.

Useful focused commands are listed in `AGENTS.md`.

## Documentation rules

- Use repository-relative links for repository files.
- Keep current product/architecture truth in its owning document.
- Treat source, tests, public docs, and design drift as a defect.
- Never commit absolute machine paths, generated caches, runtime state, or
  target-repository mutations.
