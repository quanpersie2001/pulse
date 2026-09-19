//! Thin CLI adapter for `pulse handoff|close|close-story` (plan 0022 §6).

use std::path::{Path, PathBuf};

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::completion::{close, close_epic, close_story, handoff, HandoffInput};
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

    // Plan 0025 F3: the reverse doc gate advises at handoff, it does not
    // block — same tier and same wrapping as close's `unclassified_friction`:
    // the field rides on the CLI payload, never on the stored record.
    let staled = crate::docs::stale::for_ticket(repo_root, &record)?;
    let mut value = serde_json::to_value(&record)
        .unwrap_or_else(|_| serde_json::json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut human = format!("{id} handed off");
    if !staled.is_empty() {
        value.insert(
            "docs_maybe_stale".to_string(),
            serde_json::to_value(&staled)?,
        );
        // The event log carries the advisory for the loop to read later; the
        // `docs.` prefix routes the subject through event::infer_subject_kind.
        crate::event::emit_event(
            repo_root,
            "docs.maybe_stale",
            actor.as_kind_id(),
            id,
            serde_json::json!({"docs": staled}),
            chrono::Utc::now(),
        )?;
        human.push_str(&format!(
            "\n{} doc(s) may be stale — `pulse docs check --ticket {id}`",
            staled.len()
        ));
    }
    render(json, &value, human)
}

pub(crate) fn handle_close(
    repo_root: &Path,
    id: &str,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = close(repo_root, &actor, id)?;
    // Plan 0025 E1: close never blocks on unclassified friction (a learning
    // is capped at one per Ticket by the skill), but it reports what the
    // loop left behind. The field rides on the CLI payload, not on the
    // record the kernel returns — the store's shape is unchanged.
    let unclassified = crate::learn::friction::unclassified_for(repo_root, &[id])?;
    let mut value = serde_json::to_value(&record)
        .unwrap_or_else(|_| serde_json::json!({}))
        .as_object()
        .cloned()
        .unwrap_or_default();
    value.insert(
        "unclassified_friction".to_string(),
        serde_json::json!(unclassified
            .iter()
            .map(|friction| serde_json::json!({
                "key": friction.key,
                "text": friction.text,
            }))
            .collect::<Vec<_>>()),
    );
    let mut human = format!("{id} closed");
    if !unclassified.is_empty() {
        human.push_str(&format!(
            "\n{} friction note(s) unclassified — `pulse learn friction {id}`",
            unclassified.len()
        ));
    }
    render(json, &value, human)
}

pub(crate) fn handle_close_epic(
    repo_root: &Path,
    id: &str,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let record = close_epic(repo_root, &actor, id)?;
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
