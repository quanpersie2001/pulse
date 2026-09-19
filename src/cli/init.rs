use std::path::Path;

use crate::cli::output::render;
use crate::kernel::init::{
    initialize_repository_ext, RefreshAction, RefreshTake, RefreshedFile, RepositoryInitStatus,
};
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    refresh: bool,
    no_register: bool,
    take_new: Option<String>,
    keep_mine: Option<String>,
    with_qa_templates: bool,
    json: bool,
) -> Result<(), PulseError> {
    let resolve: Option<(&str, RefreshTake)> = match (&take_new, &keep_mine) {
        (Some(file), _) => Some((file.as_str(), RefreshTake::New)),
        (None, Some(file)) => Some((file.as_str(), RefreshTake::Mine)),
        (None, None) => None,
    };
    let report = initialize_repository_ext(repo_root, refresh, with_qa_templates, resolve)?;
    // Registration is best-effort: a failure to write the user-level
    // registry never fails the repo-local init (Decision 0023).
    let mut register_note = String::new();
    if !no_register {
        match crate::serve::registry::register(repo_root) {
            Ok(()) => register_note = "\nregistered in the user project registry".to_string(),
            Err(error) => {
                register_note = format!("\nregistry registration failed: {error}");
            }
        }
    }
    let status = match report.status {
        RepositoryInitStatus::Initialized => "initialized",
        RepositoryInitStatus::Unchanged => "already initialized",
    };
    let mut human = if report.created.is_empty() {
        format!("repository {status}")
    } else {
        format!("repository {status}: created {}", report.created.join(", "))
    };
    if !report.skipped.is_empty() {
        human.push_str(&format!(
            "\nskipped (already present): {}",
            report.skipped.join(", ")
        ));
    }
    // One line per refreshed file (plan 0025 G2); a conflict still exits 0
    // — it was handled safely — but the last line says how many and where.
    push_refreshed_lines(&mut human, &report.refreshed);
    // One hint line, plan 0025 G1: reservations bind only when a host hook
    // calls the gate. The configuration itself is printed on demand —
    // `pulse init` never writes into a host's settings.
    human.push_str(
        "\n`pulse hook snippet <host>` prints the PreToolUse config that makes reservations binding",
    );
    human.push_str(&register_note);
    render(json, &report, human)
}

// Dogfood 0025, F1 (decided as D1-P1 in the dogfood report §1.1): a resolve
// run re-renders every refreshable unit, so the 0025 dogfood saw four
// `kept — no base` lines re-print on each of four consecutive invocations
// and read them as rejections. The human surface now prints every real
// state change in full, collapses `unchanged` into one count line, and
// prints only the first `kept` in full — its note carries the exact
// copy-paste `pulse init --refresh --take-new <label>` command — with the
// rest collapsed into a count. Each resolve makes the next refresh surface
// the next kept file, so the list converges instead of repeating.
fn push_refreshed_lines(human: &mut String, refreshed: &[RefreshedFile]) {
    let mut conflicts = 0_usize;
    let mut unchanged = 0_usize;
    let mut kept_printed = 0_usize;
    let mut kept_more = 0_usize;
    for file in refreshed {
        match file.action {
            RefreshAction::Unchanged => {
                unchanged += 1;
                continue;
            }
            RefreshAction::Kept => {
                kept_printed += 1;
                if kept_printed > 1 {
                    kept_more += 1;
                    continue;
                }
            }
            RefreshAction::Conflict => conflicts += 1,
            _ => {}
        }
        human.push_str(&format!("\n{}: {}", file.file, label(file.action)));
        if let Some(note) = &file.note {
            human.push_str(&format!(" — {note}"));
        }
    }
    if unchanged > 0 {
        human.push_str(&format!(
            "\nunchanged: {unchanged} file(s) already match the current template"
        ));
    }
    if kept_more > 0 {
        human.push_str(&format!(
            "\n… and {kept_more} more kept file(s) — `--json` lists them; resolve each \
             with `pulse init --refresh --take-new <file>` or `--keep-mine <file>`"
        ));
    }
    if conflicts > 0 {
        human.push_str(&format!(
            "\n{conflicts} conflict(s); your files are untouched — see \
             .pulse/runtime/refresh/ and resolve with `pulse init --refresh \
             --take-new <file>` or `--keep-mine <file>`"
        ));
    }
}

fn label(action: RefreshAction) -> &'static str {
    match action {
        RefreshAction::Created => "created",
        RefreshAction::Unchanged => "unchanged",
        RefreshAction::Updated => "updated",
        RefreshAction::Merged => "merged",
        RefreshAction::Conflict => "CONFLICT",
        RefreshAction::Kept => "kept",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(file: &str, action: RefreshAction, note: Option<&str>) -> RefreshedFile {
        RefreshedFile {
            file: file.to_string(),
            action,
            note: note.map(str::to_string),
        }
    }

    // Dogfood 0025, F1 (D1-P1): four kept files must not print four
    // `kept — no base` lines on every resolve run — the first carries the
    // copy-paste resolve command, the rest collapse into one count line.
    #[test]
    fn kept_files_collapse_to_the_first_plus_a_count() {
        let mut human = String::new();
        push_refreshed_lines(
            &mut human,
            &[
                entry(
                    "agents-block",
                    RefreshAction::Kept,
                    Some("no base to merge against — resolve with `pulse init --refresh --take-new agents-block`"),
                ),
                entry("prompts/worker.md", RefreshAction::Kept, Some("no base")),
                entry("prompts/reconcile.md", RefreshAction::Kept, Some("no base")),
            ],
        );
        assert!(human.contains("agents-block: kept"), "{human}");
        assert!(
            !human.contains("prompts/worker.md: kept"),
            "only the first kept file prints in full: {human}"
        );
        assert!(human.contains("… and 2 more kept file(s)"), "{human}");
        assert!(
            human.contains("--take-new agents-block"),
            "the first kept line must carry the copy-paste command: {human}"
        );
    }

    #[test]
    fn unchanged_files_collapse_into_one_count_line() {
        let mut human = String::new();
        push_refreshed_lines(
            &mut human,
            &[
                entry("prompts/worker.md", RefreshAction::Unchanged, None),
                entry("agents-block", RefreshAction::Unchanged, None),
                entry(
                    "prompts/reconcile.md",
                    RefreshAction::Updated,
                    Some("the user never touched it"),
                ),
            ],
        );
        assert!(human.contains("prompts/reconcile.md: updated"), "{human}");
        assert!(
            !human.contains("prompts/worker.md: unchanged"),
            "individual unchanged lines must not print: {human}"
        );
        assert!(human.contains("unchanged: 2 file(s)"), "{human}");
    }

    #[test]
    fn state_changes_print_in_full_and_conflicts_still_count() {
        let mut human = String::new();
        push_refreshed_lines(
            &mut human,
            &[
                entry("prompts/worker.md", RefreshAction::Merged, None),
                entry("agents-block", RefreshAction::Conflict, None),
            ],
        );
        assert!(human.contains("prompts/worker.md: merged"), "{human}");
        assert!(human.contains("agents-block: CONFLICT"), "{human}");
        assert!(human.contains("1 conflict(s)"), "{human}");
    }
}
