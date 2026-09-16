//! The `pulse serve` request loop (Decision 0023): tiny_http on
//! 127.0.0.1, GET-only, every route read-only. Routing is a flat match on
//! path segments; all payload building lives in `api.rs`, which is where
//! the tests live too.

use std::path::Path;

use tiny_http::{Header, Response, Server};

use super::api;

/// Serve until the process is killed. There is no shutdown command: the
/// server holds no state, so killing it loses nothing.
///
/// # Errors
/// An io error binding the socket surfaces as `PulseError::io`.
pub fn run(
    registry: Option<&Path>,
    workspace: Option<&Path>,
    port: u16,
    open: bool,
) -> Result<(), crate::PulseError> {
    let server = Server::http(("127.0.0.1", port)).map_err(|error| {
        crate::PulseError::io(
            std::path::PathBuf::from(format!("127.0.0.1:{port}")),
            std::io::Error::other(error.to_string()),
        )
    })?;
    let url = format!("http://127.0.0.1:{port}/");
    match (registry, workspace) {
        (Some(_), Some(w)) => {
            eprintln!(
                "pulse serve: {url} (registry + workspace scan: {})",
                w.display()
            )
        }
        (Some(_), None) => eprintln!("pulse serve: {url} (registry)"),
        (None, Some(w)) => eprintln!("pulse serve: {url} (workspace scan: {})", w.display()),
        (None, None) => {
            eprintln!("pulse serve: {url} (no projects: run pulse init, or pass --workspace)")
        }
    }
    eprintln!("read-only; Ctrl-C to stop");
    if open {
        open_browser(&url);
    }
    for request in server.incoming_requests() {
        let _ = respond(request, registry, workspace);
    }
    Ok(())
}

enum Route<'a> {
    Index,
    Projects,
    Board {
        pid: &'a str,
    },
    Issue {
        pid: &'a str,
        id: &'a str,
    },
    /// `/p/<pid>/evidence/<issue-id>/<relative path under the issue's
    /// evidence dir>`
    Evidence {
        pid: &'a str,
        issue: String,
        relative: String,
    },
    NotFound,
}

fn route(path: &str) -> Route<'_> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        [] => Route::Index,
        ["api", "projects"] => Route::Projects,
        ["api", "p", pid, "board"] => Route::Board { pid },
        ["api", "p", pid, "issue", id] => Route::Issue { pid, id },
        ["p", pid, "evidence", issue, rest @ ..] if !rest.is_empty() => Route::Evidence {
            pid,
            issue: (*issue).to_string(),
            relative: rest.join("/"),
        },
        _ => Route::NotFound,
    }
}

fn respond(
    request: tiny_http::Request,
    registry: Option<&Path>,
    workspace: Option<&Path>,
) -> std::io::Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("").trim_matches('/');

    if method != tiny_http::Method::Get {
        return json_response(request, 405, &serde_json::json!({"error": "GET only"}));
    }

    match route(path) {
        Route::Index => {
            let html = include_str!("../../assets/board/board.html");
            bytes_response(
                request,
                200,
                "text/html; charset=utf-8",
                html.as_bytes().to_vec(),
            )
        }
        Route::Projects => json_response(request, 200, &api::projects_payload(registry, workspace)),
        Route::Board { pid } => {
            match api::with_project(registry, workspace, pid, api::board_payload) {
                Some(payload) => json_response(request, 200, &payload),
                None => json_response(
                    request,
                    404,
                    &serde_json::json!({"error": format!("unknown project {pid}")}),
                ),
            }
        }
        Route::Issue { pid, id } => {
            match api::with_project(registry, workspace, pid, |root| {
                api::issue_payload(root, id)
            }) {
                Some(Some(payload)) => json_response(request, 200, &payload),
                Some(None) => json_response(
                    request,
                    404,
                    &serde_json::json!({"error": format!("no issue {id}")}),
                ),
                None => json_response(
                    request,
                    404,
                    &serde_json::json!({"error": format!("unknown project {pid}")}),
                ),
            }
        }
        Route::Evidence {
            pid,
            issue,
            relative,
        } => {
            let served = api::with_project(registry, workspace, pid, |root| {
                api::evidence_file(root, &issue, &relative)
            });
            match served {
                Some(Some((bytes, kind))) => bytes_response(request, 200, kind, bytes),
                Some(None) => json_response(
                    request,
                    404,
                    &serde_json::json!({"error": "no such evidence file"}),
                ),
                None => json_response(
                    request,
                    404,
                    &serde_json::json!({"error": format!("unknown project {pid}")}),
                ),
            }
        }
        Route::NotFound => json_response(request, 404, &serde_json::json!({"error": "not found"})),
    }
}

fn json_response(
    request: tiny_http::Request,
    status: u16,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    match serde_json::to_vec(value) {
        Ok(bytes) => bytes_response(request, status, "application/json", bytes),
        Err(error) => bytes_response(
            request,
            500,
            "application/json",
            format!("{{\"error\":\"{error}\"}}").into_bytes(),
        ),
    }
}

fn bytes_response(
    request: tiny_http::Request,
    status: u16,
    content_type: &str,
    bytes: Vec<u8>,
) -> std::io::Result<()> {
    let header = Header::from_bytes("Content-Type", content_type)
        .unwrap_or(Header::from_bytes("Content-Type", "application/octet-stream").unwrap());
    let response = Response::from_data(bytes)
        .with_status_code(status)
        .with_header(header);
    request.respond(response)
}

fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "linux") {
        "xdg-open"
    } else {
        return;
    };
    let _ = std::process::Command::new(program).arg(url).spawn();
}
