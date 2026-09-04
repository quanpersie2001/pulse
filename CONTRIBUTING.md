# Contributing

Pulse is one Rust executable: a local truth layer for developers using coding
agents. See [`PRODUCT.md`](PRODUCT.md) for scope and target design, and
[`AGENTS.md`](AGENTS.md) for operating rules.

## Repository truth

- `PRODUCT.md`: product definition, target design, golden path, code triage.
- `docs/decisions/`: accepted decisions. 0008 is the current scope decision.
- `src/` and `tests/`: current implementation and its contracts.
- `AGENTS.md`, `README.md`, this file: repository operating contracts. They
  describe only what has code and tests.
- `design/archive/`: historical proposals and retired design. Not a contract.
- `examples/todolist/`: dogfood target repository; Pulse runs there for real.

## Ownership and dependency direction

- The binary delegates to the `pulse::cli` facade.
- CLI owns transport and rendering, not domain semantics.
- `kernel/` composes domains; domains (`graph`, `docs`, `evidence`, `qa`,
  `knowledge`) do not import each other's stores except through documented
  narrow seams.
- `graph/` layers bottom-up: model → validation → read → store.

Preserve stable public paths deliberately. Keep new surfaces private by
default and update architecture/public-path tests when a contract changes on
purpose.

## Target-repository boundary

Never run Pulse mutations with `--repo-root .` at this repository's root.
Run Pulse for real only against `examples/todolist/`. Integration tests copy a
tracked fixture through `tests/common/fixture_repo.rs::TestRepo::from_fixture`
and run Pulse against the temporary copy.

## Change workflow

1. Read `PRODUCT.md` for the feature's intended shape, then the owning source
   and tests.
2. Keep changes scoped to the owning module. Preserve lock ordering, atomic
   write and recovery invariants at storage boundaries.
3. Add focused coverage in the existing domain integration crate.
4. Update `README.md` or `AGENTS.md` when public behavior or ownership
   changes. Update `PRODUCT.md` only through a decision.
5. Run narrow tests first, then the repository gates.

## Validation

```bash
cargo fmt --check
cargo clippy --all-targets --quiet -- -D warnings
cargo test --all-targets
```

`cargo test --all-targets` must pass with default threading.

## Documentation rules

- Repository-relative links only.
- Current truth lives in its owning document; do not duplicate it.
- Do not describe unimplemented features as existing. Target design belongs
  in `PRODUCT.md`.
- Never commit absolute machine paths, caches, runtime state or mutations of
  a test fixture.
