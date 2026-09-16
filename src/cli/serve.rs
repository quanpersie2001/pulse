//! Thin transport for `pulse serve` (Decision 0023): flags in, hand off
//! to `serve::http`. The server is intentionally outside the usual
//! repo-root resolution — it reads a *workspace* of repos, so it must run
//! from anywhere.

use std::path::Path;

use crate::PulseError;

pub(crate) fn handle(workspace: Option<&Path>, port: u16, open: bool) -> Result<(), PulseError> {
    // The user registry path is resolved once at startup; file contents
    // are still re-read per request.
    let registry = crate::serve::registry::registry_path().ok();
    crate::serve::http::run(registry.as_deref(), workspace, port, open)
}
