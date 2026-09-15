//! Thin CLI adapter for `pulse handoff|close|close-story` (plan 0022 §6).

use std::path::{Path, PathBuf};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::completion::{close, close_story, handoff, HandoffInput};
use crate::PulseError;

pub(crate) fn handle_handoff(
    repo_root: &Path,
    id: &str,
    from: &PathBuf,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let bytes = std::fs::read(from).map_err(|error| PulseError::io(from, error))?;
    let input: HandoffInput = serde_json::from_slice(&bytes).map_err(|error| {
        PulseError::kernel(
            "from_file_invalid",
            format!("{} is not a valid handoff: {error}", from.display()),
            "--from expects the JSON shape in plan 0022 §7.2 (summary, changed_files, \
             acceptance, verify_results, docs_updated, learnings_used, friction, open_risks)",
        )
    })?;
    let record = handoff(repo_root, &actor, id, input)?;
    render(json, &record, format!("{id} handed off"))
}

pub(crate) fn handle_close(
    repo_root: &Path,
    id: &str,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = close(repo_root, &actor, id)?;
    render(json, &record, format!("{id} closed"))
}

pub(crate) fn handle_close_story(
    repo_root: &Path,
    id: &str,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = close_story(repo_root, &actor, id)?;
    render(json, &record, format!("{id} closed"))
}
