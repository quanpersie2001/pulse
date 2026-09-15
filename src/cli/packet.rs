//! Thin CLI adapter for `pulse packet|checkpoint` (plan 0022 §6, §9, §10.3).

use std::path::{Path, PathBuf};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::checkpoint::{checkpoint, CheckpointInput};
use crate::kernel::packet::build_packet;
use crate::PulseError;

pub(crate) fn handle_packet(repo_root: &Path, id: &str, json: bool) -> Result<(), PulseError> {
    let packet = build_packet(repo_root, id)?;
    render(json, &packet, format!("packet for {id}"))
}

pub(crate) fn handle_checkpoint(
    repo_root: &Path,
    id: &str,
    from: &PathBuf,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let bytes = std::fs::read(from).map_err(|error| PulseError::io(from, error))?;
    let input: CheckpointInput = serde_json::from_slice(&bytes).map_err(|error| {
        PulseError::kernel(
            "from_file_invalid",
            format!("{} is not a valid checkpoint: {error}", from.display()),
            "--from expects the JSON shape in plan 0022 §4.4/§10.3",
        )
    })?;
    let record = checkpoint(repo_root, &actor, id, input)?;
    render(json, &record, format!("checkpoint recorded for {id}"))
}
