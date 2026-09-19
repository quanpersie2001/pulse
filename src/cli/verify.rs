//! Thin CLI adapter for `pulse verify <id>` (decision 0026).
//!
//! Renders what the kernel observed, then turns a failed run into a non-zero
//! exit code. The receipt is sealed by the kernel before this returns an
//! error — a failed verify is evidence, not something swallowed — so the
//! only job here is reporting.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::cli::output::render;
use crate::identity::actor::resolve_actor;
use crate::kernel::verify;
use crate::PulseError;

pub(crate) fn handle(
    repo_root: &Path,
    id: &str,
    timeout_seconds: u64,
    actor: Option<&str>,
    json: bool,
) -> Result<(), PulseError> {
    let actor = resolve_actor(repo_root, actor)?;
    let report = verify::verify(repo_root, &actor, id, Duration::from_secs(timeout_seconds))?;
    render(json, &report, render_human(&report))?;
    if report["passed"].as_bool() != Some(true) {
        return Err(PulseError::kernel(
            "verify_failed",
            format!("{id}: one or more declared verify commands failed"),
            "read the logs under .pulse/evidence/<id>/verify/",
        ));
    }
    Ok(())
}

/// One line per command: `ok|FAIL|TIMEOUT  <name>  <exit>  <ms>  <log path>`.
/// A command with no exit code (killed, or never spawned) prints `-`; its log
/// file says which.
fn render_human(report: &Value) -> String {
    let empty: Vec<Value> = Vec::new();
    let results = report["results"].as_array().unwrap_or(&empty);
    results
        .iter()
        .map(|result| {
            let status = if result["timed_out"].as_bool() == Some(true) {
                "TIMEOUT"
            } else if result["exit"].as_i64() == Some(0) {
                "ok"
            } else {
                "FAIL"
            };
            let exit = result["exit"]
                .as_i64()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "-".to_string());
            format!(
                "{status:<7} {}  {exit}  {}ms  {}",
                result["name"].as_str().unwrap_or("?"),
                result["duration_ms"].as_u64().unwrap_or(0),
                result["log"].as_str().unwrap_or("?"),
            )
        })
        .collect::<Vec<String>>()
        .join("\n")
}
