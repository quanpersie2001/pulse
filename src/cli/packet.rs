//! Thin CLI adapter for `pulse packet|checkpoint` (plan 0022 §6, §9, §10.3).

use std::path::{Path, PathBuf};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::checkpoint::{checkpoint, CheckpointInput};
use crate::kernel::packet::build_packet;
use crate::PulseError;

// Dogfood 0025, F8: `pulse packet` printed only a 19-byte header ("packet
// for TK-…") unless `--json` was passed, so two independent workers both had
// to rebuild their packet from `work show --json` + `learn applicable`. The
// packet IS a JSON document — print it in both modes; `--json` stays
// accepted so existing host scripts do not break.
pub(crate) fn handle_packet(repo_root: &Path, id: &str, _json: bool) -> Result<(), PulseError> {
    let packet = build_packet(repo_root, id)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&packet)
            .map_err(|e| PulseError::validation("json_serialize_error", e.to_string()))?
    );
    Ok(())
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
