# Pulse Target Repository Fixtures

These directories are tracked, read-only templates that model repositories on
which the Pulse harness operates.

Tests must never run Pulse mutations directly against a fixture directory.
Copy a fixture into a temporary directory first, initialize the copy as a Git
repository, and pass that temporary path as `--repo-root`.

Rust integration tests should use `tests/common/fixture_repo.rs`:

```rust
mod common;

use common::fixture_repo::TestRepo;

let repo = TestRepo::from_fixture("minimal-service");
let output = repo.pulse_ok(&["graph", "bootstrap", "--json"]);
```

The fixture source must remain free of generated `.pulse/` state and nested
`.git/` directories.
