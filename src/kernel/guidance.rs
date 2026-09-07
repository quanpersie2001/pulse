//! The Pulse guidance surface inside a target repository (Decision 0009 §1).
//!
//! Pulse writes two prose files into the repository it serves: a marked block
//! inside `AGENTS.md` that routes a request to the right `pulse` commands, and
//! `PULSE.md` for repository-specific authority notes. The block is the only
//! Pulse-owned region of `AGENTS.md`; everything outside the markers belongs to
//! the repository and is never touched.
//!
//! State touched: `AGENTS.md` and `PULSE.md` at the repository root, both in
//! the source plane. Callers must already hold the repository write guard.
//!
//! Invariant: Pulse overwrites the block only when it still recognises its own
//! rendering. The `BEGIN` marker records the Pulse version and a hash of the
//! body Pulse wrote; when the body on disk no longer hashes to that value a
//! human has edited it, and a refresh reports the drift instead of destroying
//! the edit. There is no sidecar state file — the marker carries everything
//! needed to make that judgement (Decision 0009: no parallel control plane).

use std::fs;
use std::path::Path;

use crate::canonical_json::hash_bytes;
use crate::{PulseError, Result};

/// Body of the routing block, rendered into `AGENTS.md` between the markers.
/// Shared by `pulse init` and its tests so the two cannot drift.
const AGENTS_BLOCK_BODY: &str = include_str!("../../assets/agents-block.md");

/// Seed content for `PULSE.md`. Deliberately close to empty: the routing
/// contract lives in the `AGENTS.md` block, and this file is where a
/// repository records its own authority rules.
const PULSE_MD_SEED: &str = "\
# PULSE.md

Repository-specific authority for agents working through Pulse. `AGENTS.md`
routes a request to the right command; this file records what is true about
*this* repository and who may decide it.

Pulse creates this file once and never rewrites it. Everything below is yours.

## Authority

- Accepted Decisions under `docs/decisions/` and approved docs under
  `docs/product/` are intent.
- Code and tests are implementation.
- Receipts are observation.

## Boundaries

<!-- What must an agent never do in this repository without asking? -->

## Verification

<!-- Which commands prove a change is sound here? -->
";

const BEGIN_PREFIX: &str = "<!-- PULSE:BEGIN";
const END_MARKER: &str = "<!-- PULSE:END -->";

/// What a guidance write did to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuidanceOutcome {
    /// The file did not exist, or carried no Pulse block, and now does.
    Written,
    /// The block was already current; nothing changed.
    Unchanged,
    /// The block was re-rendered from a newer template.
    Refreshed,
    /// A Pulse block exists and was left alone because this was not a refresh.
    Preserved,
    /// The block was hand-edited; the caller must report, not overwrite.
    Modified,
}

/// Render the `BEGIN` marker for `body`, recording the Pulse version that
/// produced it and the hash a later refresh checks the body against.
fn begin_marker(body: &str) -> String {
    format!(
        "{BEGIN_PREFIX} version={} body={} -->",
        env!("CARGO_PKG_VERSION"),
        hash_bytes(body.as_bytes())
    )
}

/// The full block, markers included.
fn render_block(body: &str) -> String {
    format!("{}\n{}{}", begin_marker(body), body, END_MARKER)
}

/// Byte ranges of the block in `content`: the whole block, and the body alone.
///
/// Returns `None` when no complete, well-ordered marker pair is present.
fn locate_block(content: &str) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let begin = content.find(BEGIN_PREFIX)?;
    // The marker is a single HTML comment; its body starts after it closes.
    let begin_close = content[begin..].find("-->")? + begin + "-->".len();
    let end = content[begin_close..].find(END_MARKER)? + begin_close;
    // Skip exactly one newline after the marker, matching how it is rendered.
    let body_start = if content[begin_close..].starts_with('\n') {
        begin_close + 1
    } else {
        begin_close
    };
    Some((begin..end + END_MARKER.len(), body_start..end))
}

/// The `body=` hash recorded in the `BEGIN` marker, when present.
fn recorded_hash(content: &str, block: &std::ops::Range<usize>) -> Option<String> {
    let marker_end = content[block.clone()].find("-->")? + block.start;
    let marker = &content[block.start..marker_end];
    marker
        .split_whitespace()
        .find_map(|field| field.strip_prefix("body="))
        .map(str::to_string)
}

/// Write or refresh the routing block in `AGENTS.md`.
///
/// Without `refresh` an existing block is preserved untouched, so repeated
/// `pulse init` never rewrites prose. With `refresh` the block is re-rendered
/// only when its body still hashes to the value the marker records; otherwise
/// the edit wins and the caller is told.
///
/// Content outside the markers is preserved byte for byte in every case.
///
/// # Errors
///
/// Returns an I/O error when `AGENTS.md` cannot be read or written.
pub(crate) fn write_agents_block(repo_root: &Path, refresh: bool) -> Result<GuidanceOutcome> {
    let path = repo_root.join("AGENTS.md");
    let block = render_block(AGENTS_BLOCK_BODY);

    if !path.exists() {
        fs::write(&path, format!("{block}\n")).map_err(|error| PulseError::io(&path, error))?;
        return Ok(GuidanceOutcome::Written);
    }

    let content = fs::read_to_string(&path).map_err(|error| PulseError::io(&path, error))?;
    let Some((block_range, body_range)) = locate_block(&content) else {
        // An AGENTS.md written before Pulse, or by hand: append the block and
        // leave every existing line where it is.
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        let appended = format!("{content}{separator}\n{block}\n");
        fs::write(&path, appended).map_err(|error| PulseError::io(&path, error))?;
        return Ok(GuidanceOutcome::Written);
    };

    if !refresh {
        return Ok(GuidanceOutcome::Preserved);
    }

    let on_disk_body = &content[body_range];
    let recorded = recorded_hash(&content, &block_range);
    // A marker with no recorded hash predates drift detection; treat it as
    // hand-managed rather than guessing.
    let Some(recorded) = recorded else {
        return Ok(GuidanceOutcome::Modified);
    };
    if hash_bytes(on_disk_body.as_bytes()) != recorded {
        return Ok(GuidanceOutcome::Modified);
    }
    if on_disk_body == AGENTS_BLOCK_BODY {
        return Ok(GuidanceOutcome::Unchanged);
    }

    let refreshed = format!(
        "{}{}{}",
        &content[..block_range.start],
        block,
        &content[block_range.end..]
    );
    fs::write(&path, refreshed).map_err(|error| PulseError::io(&path, error))?;
    Ok(GuidanceOutcome::Refreshed)
}

/// Create `PULSE.md` when it is absent.
///
/// Never rewritten, refresh included: the block in `AGENTS.md` is Pulse's to
/// render, but `PULSE.md` is the repository's own authority file from the
/// moment it exists.
///
/// # Errors
///
/// Returns an I/O error when the file cannot be written.
pub(crate) fn write_pulse_md(repo_root: &Path) -> Result<GuidanceOutcome> {
    let path = repo_root.join("PULSE.md");
    if path.exists() {
        return Ok(GuidanceOutcome::Preserved);
    }
    fs::write(&path, PULSE_MD_SEED).map_err(|error| PulseError::io(&path, error))?;
    Ok(GuidanceOutcome::Written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_a_rendered_block_and_recovers_its_body() {
        let content = format!("# Repo\n\n{}\n", render_block(AGENTS_BLOCK_BODY));
        let (block, body) = locate_block(&content).expect("block is present");
        assert_eq!(&content[body], AGENTS_BLOCK_BODY);
        assert!(content[block].starts_with(BEGIN_PREFIX));
    }

    #[test]
    fn recovers_the_hash_recorded_in_the_marker() {
        let content = render_block(AGENTS_BLOCK_BODY);
        let (block, _) = locate_block(&content).unwrap();
        assert_eq!(
            recorded_hash(&content, &block),
            Some(hash_bytes(AGENTS_BLOCK_BODY.as_bytes()))
        );
    }

    #[test]
    fn an_unterminated_marker_is_not_a_block() {
        let content = format!("{}\nbody without an end\n", begin_marker("x"));
        assert!(locate_block(&content).is_none());
    }
}
