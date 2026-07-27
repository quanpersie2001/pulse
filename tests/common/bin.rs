//! Shared resolver for the Pulse binary under test.
//!
//! Used across the `graph`, `docs` and `knowledge` integration-test crates so
//! each crate can resolve the CLI binary without redefining the lookup. This is
//! pure plumbing (a path string) with no process or tempdir behavior to hide.

/// Resolve the Pulse CLI binary path.
///
/// Prefers the `CARGO_BIN_EXE_pulse` path Cargo injects for the `pulse` binary
/// target, falling back to the locally built `target/debug/pulse`.
pub fn bin() -> String {
    std::env::var("CARGO_BIN_EXE_pulse").unwrap_or_else(|_| "target/debug/pulse".to_string())
}
