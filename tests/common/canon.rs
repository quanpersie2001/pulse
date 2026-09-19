//! Shared canonical-JSON writer for test fixtures.
//!
//! Writes a serializable value as canonical bytes. Wired only into crates that
//! build canonical-JSON fixture files.

use std::path::Path;

/// Write `value` to `path` as canonical JSON bytes.
pub fn write_json(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(
        path,
        pulse::canonical_json::to_canonical_bytes(value).unwrap(),
    )
    .unwrap();
}
