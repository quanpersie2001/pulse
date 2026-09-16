//! Thin transport for `pulse serve` (Decision 0023): flags in, hand off
//! to `serve::http`. The server is intentionally outside the usual
//! repo-root resolution — it reads a *workspace* of repos, so it must run
//! from anywhere.

use std::path::Path;

use crate::PulseError;

pub(crate) fn handle(workspace: &Path, port: u16, open: bool) -> Result<(), PulseError> {
    crate::serve::http::run(workspace, port, open)
}
