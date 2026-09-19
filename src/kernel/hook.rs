//! The pre-edit gate (plan 0025 G1): the one place Pulse can make its file
//! reservations bind on an edit that never runs a `pulse` command.
//!
//! A host hook (Claude Code's PreToolUse) calls [`pre_edit`] before a
//! file-writing tool fires. The decision is made from the PATH alone, never
//! from a declared identity: at edit time no host reports reliably which
//! subagent is calling, and `--actor` is a self-declared string (decision
//! 0026, "Không giải quyết"). Where an actor is present it sharpens two
//! outcomes (who holds what); where it is absent the rules still bind —
//! an edit must fall inside some active ticket's `touches`, and a lane may
//! only write evidence. The hook does not stop a session that declares
//! itself `human:`; the shipped snippet passes no actor at all, so nothing
//! hinges on that claim.
//!
//! Pure reads: one call = one store read + one `PULSE.md` parse. No lock,
//! no mutation, no event — it runs on every edit and must stay cheap and
//! silent. Internal failures (a torn store, a broken `PULSE.md`) return
//! `Err`, which the CLI maps to exit 1: a torn store must never lock every
//! edit (`pulse doctor` is where a torn store is reported), while a `Deny`
//! is always a deliberate decision the host shows the agent.
//!
//! Honest limits, restated in ARCHITECTURE.md: the hook only sees the
//! host's edit tools. A file written through a shell (`sed -i`, `>`)
//! bypasses it entirely — `handoff_unreserved_changes` (decision 0025 B6)
//! is the second net.

use std::path::Path;

use chrono::Utc;
use serde_json::Value;

use crate::error::Result;
use crate::identity::actor::{ActorKind, ActorRef};
use crate::kernel::profile;
use crate::kernel::reservation::{lease_actor, lease_is_live, touches_of};
use crate::kernel::{roles, scope};
use crate::source;
use crate::store::issues;

/// What the pre-edit gate decided for one path. `Allow` carries the reason
/// it stayed silent (for tests and debug logging); `Deny` carries the
/// message the host hands back to the editing agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditDecision {
    Allow { why: &'static str },
    Deny { message: String },
}

/// Decide whether one file edit may proceed (plan 0025 G1). `path` is the
/// file about to be written, repo-relative or absolute; `actor` is the
/// self-declared caller, usually `None` (the shipped snippet passes no
/// actor) or the worker's `PULSE_ACTOR`.
///
/// The rules, in order — a fenced-out path is decided at rule 2, so rules
/// 4–8 only ever see source paths:
/// 1. not enrolled (no `.pulse/issues.jsonl`) → allow;
/// 2. fenced out (`.pulse/**`, root `PULSE.md`/`AGENTS.md`, `fence_ignore`)
///    → allow — unless the actor is a review/qa lane and the path is not
///    under `.pulse/evidence/`: a lane writes only its evidence;
/// 3. a lane and a source path → deny (lanes review, they do not edit);
/// 4. no active ticket with a live lease → per `hook.unclaimed` in
///    `PULSE.md` (default `allow`);
/// 5. an active ticket without `touches` is exclusive: another actor is
///    denied, everyone else allowed;
/// 6. the path is covered by an active ticket's `touches`: another actor
///    is denied, its holder (and the actor-less case) allowed;
/// 7. the path is covered by a `verifying` ticket → denied (under review);
/// 8. anything else → denied: outside every active ticket's `touches`.
///
/// # Errors
/// `pulse_md_invalid` when `PULSE.md` exists but does not parse — a broken
/// config is an internal failure (exit 1), never a silent allow or deny.
/// Propagates a torn-store error from reading `issues.jsonl` for the same
/// reason.
pub fn pre_edit(repo_root: &Path, path: &str, actor: Option<&ActorRef>) -> Result<EditDecision> {
    let Some(rel) = repo_relative(repo_root, path) else {
        return Ok(EditDecision::Allow {
            why: "outside repo",
        });
    };
    if !repo_root.join(".pulse/issues.jsonl").exists() {
        return Ok(EditDecision::Allow {
            why: "not enrolled",
        });
    }
    let config = profile::load(repo_root)?;
    let records = issues::read_all(repo_root)?;
    let now = Utc::now();
    // Lane-ness is a property of an agent id (decision 0025 B3): the same
    // prefix table authorization uses, read from the other side.
    let lane =
        actor.is_some_and(|actor| actor.kind == ActorKind::Agent && roles::is_lane_role(&actor.id));

    if source::is_fenced_out(&rel, &config.fence_ignore) {
        if lane && !rel.starts_with(".pulse/evidence/") {
            return Ok(EditDecision::Deny {
                message: format!(
                    "{rel} is not evidence: a lane writes only under \
                     .pulse/evidence/<id>/"
                ),
            });
        }
        return Ok(EditDecision::Allow { why: "fenced out" });
    }
    if lane {
        let who = actor.map(ActorRef::as_kind_id).unwrap_or_default();
        return Ok(EditDecision::Deny {
            message: format!(
                "{who} is a review lane: lanes review and write only under \
                 .pulse/evidence/, they do not edit source"
            ),
        });
    }

    let held: Vec<&Value> = records
        .iter()
        .filter(|record| {
            record.get("status").and_then(Value::as_str) == Some("active")
                && lease_is_live(record, now)
        })
        .collect();
    if held.is_empty() {
        return Ok(match config.hook.unclaimed {
            profile::UnclaimedPolicy::Allow => EditDecision::Allow {
                why: "no ticket is active",
            },
            profile::UnclaimedPolicy::Deny => EditDecision::Deny {
                message: "claim a ticket first: `pulse frontier`, then `pulse claim <id>`"
                    .to_string(),
            },
        });
    }

    // Rule 5: a ticket without `touches` claims the whole tree exclusively
    // (decision 0025), so it decides every source path by itself.
    for ticket in &held {
        if !touches_of(ticket).is_empty() {
            continue;
        }
        let id = record_id(ticket);
        return Ok(match actor {
            Some(actor) if Some(actor.as_kind_id().as_str()) != lease_actor(ticket) => {
                EditDecision::Deny {
                    message: format!("{id} holds the whole tree exclusively"),
                }
            }
            _ => EditDecision::Allow {
                why: "an active ticket holds the whole tree",
            },
        });
    }

    // Rule 6: inside a held ticket's `touches`.
    for ticket in &held {
        if !scope::covers(&touches_of(ticket), &rel) {
            continue;
        }
        let id = record_id(ticket);
        return Ok(match actor {
            Some(actor) if Some(actor.as_kind_id().as_str()) != lease_actor(ticket) => {
                let holder = lease_actor(ticket).unwrap_or("?");
                EditDecision::Deny {
                    message: format!("{rel} is reserved by {id} ({holder})"),
                }
            }
            _ => EditDecision::Allow {
                why: "inside the active ticket's touches",
            },
        });
    }

    // Rule 7: files stay held through review (decision 0025) — an edit
    // under a verifying ticket would stale its close forever.
    for record in records
        .iter()
        .filter(|record| record.get("status").and_then(Value::as_str) == Some("verifying"))
    {
        if scope::covers(&touches_of(record), &rel) {
            return Ok(EditDecision::Deny {
                message: format!(
                    "{rel} is under review in {}; wait for it to close",
                    record_id(record)
                ),
            });
        }
    }

    Ok(EditDecision::Deny {
        message: format!(
            "{rel} is outside every active ticket's `touches`. If it is yours: \
             `pulse reserve <your ticket> {rel}`. If not: leave it."
        ),
    })
}

/// The id of a store record, for messages only (`?` keeps a malformed
/// record from panicking the gate).
fn record_id(record: &Value) -> &str {
    record.get("id").and_then(Value::as_str).unwrap_or("?")
}

/// Normalize an edit path to repo-relative, or `None` when the hook has no
/// business deciding it: an absolute path outside the repo (another
/// project, a temp file), or a relative path that escapes lexically. The
/// file itself may not exist yet (PreToolUse), so only the repo root and
/// the path's parent are canonicalized — never the leaf. Both spellings
/// are compared because macOS hands the process a canonicalized cwd
/// (`/private/tmp/…`) while hosts happily send `/tmp/…` paths; the
/// reference harness resolves exactly the same parent-only way.
fn repo_relative(repo_root: &Path, raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let candidate = Path::new(raw);
    if candidate.is_absolute() {
        let mut roots = vec![repo_root.to_path_buf()];
        if let Ok(resolved) = repo_root.canonicalize() {
            roots.push(resolved);
        }
        let mut spellings = vec![candidate.to_path_buf()];
        if let Some(parent) = candidate.parent() {
            if let (Ok(resolved_parent), Some(file_name)) =
                (parent.canonicalize(), candidate.file_name())
            {
                spellings.push(resolved_parent.join(file_name));
            }
        }
        for spelling in &spellings {
            for root in &roots {
                if let Ok(rel) = spelling.strip_prefix(root) {
                    return to_rel_string(rel);
                }
            }
        }
        return None;
    }
    // Repo-relative already; refuse anything that climbs out lexically.
    if candidate
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return None;
    }
    to_rel_string(candidate)
}

fn to_rel_string(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    // The store's path grammar (touches patterns, fence entries, receipts)
    // spells repo-relative paths with `/` on every platform; Windows
    // `\` separators are normalized so `src\deep\x.rs` decides against
    // `src/**` exactly as it does on Unix.
    path.to_str().map(|text| text.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    fn agent(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Agent,
            id: id.to_string(),
        }
    }

    fn human(id: &str) -> ActorRef {
        ActorRef {
            kind: ActorKind::Human,
            id: id.to_string(),
        }
    }

    /// An enrolled repo: `.pulse/issues.jsonl` exists, `PULSE.md` parses.
    /// Records are written straight into the store the way the
    /// reservation tests do.
    fn enrolled(records: &[Value]) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".pulse")).unwrap();
        std::fs::write(repo.path().join(".pulse/issues.jsonl"), b"").unwrap();
        std::fs::write(
            repo.path().join("PULSE.md"),
            "fence_ignore: []\nprofiles:\n  cli-low: {lanes: [review-correctness]}\n",
        )
        .unwrap();
        crate::store::issues::mutate(repo.path(), |mut all| {
            all.extend(records.iter().cloned());
            Ok(all)
        })
        .unwrap();
        repo
    }

    fn active_ticket(id: &str, touches: &[&str], worker: &str) -> Value {
        let mut ticket = json!({
            "schema": 3, "id": id, "kind": "ticket", "title": "t",
            "status": "active", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
        });
        if !touches.is_empty() {
            ticket["touches"] = json!(touches);
        }
        let expires = (Utc::now() + Duration::hours(1)).to_rfc3339();
        ticket["lease"] = json!({
            "role": "worker", "actor": format!("agent:{worker}"),
            "run_id": "run_1", "expires_at": expires,
        });
        ticket
    }

    fn verifying_ticket(id: &str, touches: &[&str]) -> Value {
        let mut ticket = active_ticket(id, touches, "worker-1");
        ticket["status"] = json!("verifying");
        // Handoff dropped the lease; the files stay held by status alone.
        ticket["lease"] = json!(null);
        ticket
    }

    fn decide(repo: &Path, path: &str, actor: Option<&ActorRef>) -> EditDecision {
        pre_edit(repo, path, actor).unwrap()
    }

    // --- rule 1 + path normalization ---

    #[test]
    fn an_unenrolled_repo_allows_everything() {
        let repo = tempfile::tempdir().unwrap();
        assert_eq!(
            decide(repo.path(), "src/lib.rs", None),
            EditDecision::Allow {
                why: "not enrolled"
            }
        );
    }

    #[test]
    fn a_path_outside_the_repo_is_not_ours_to_judge() {
        let repo = enrolled(&[]);
        // A second tempdir is outside the repo and absolute on every
        // platform — "/etc/passwd" is not absolute on Windows, where it
        // would resolve inside the repo instead of outside it.
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("x.rs");
        assert_eq!(
            decide(repo.path(), &outside_file.to_string_lossy(), None),
            EditDecision::Allow {
                why: "outside repo"
            }
        );
        assert_eq!(
            decide(repo.path(), "../sibling/x.rs", None),
            EditDecision::Allow {
                why: "outside repo"
            }
        );
    }

    #[test]
    fn an_absolute_path_is_resolved_against_the_repo_root() {
        let repo = enrolled(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
        let absolute = repo.path().join("src/deep/x.rs");
        let absolute = absolute.to_str().unwrap();
        assert_eq!(
            decide(repo.path(), absolute, Some(&agent("worker-1"))),
            EditDecision::Allow {
                why: "inside the active ticket's touches"
            }
        );
        // The same file spelled relatively decides the same way.
        assert_eq!(
            decide(repo.path(), "src/deep/x.rs", Some(&agent("worker-1"))),
            decide(repo.path(), absolute, Some(&agent("worker-1")))
        );
    }

    #[test]
    fn a_host_path_spelled_through_a_symlink_still_lands_in_the_repo() {
        // The OS hands the process a canonicalized cwd (`/private/tmp/…`)
        // while hosts send `/tmp/…` spellings; the parent-only resolution
        // must reconcile them (found live on macOS: /tmp -> /private/tmp).
        let repo = enrolled(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
        std::fs::create_dir_all(repo.path().join("src/deep")).unwrap();
        let resolved = repo.path().canonicalize().unwrap();
        let host_spelling = repo.path().join("src/deep/x.rs");
        let host_spelling = host_spelling.to_str().unwrap();
        assert_eq!(
            decide(&resolved, host_spelling, Some(&agent("worker-1"))),
            EditDecision::Allow {
                why: "inside the active ticket's touches"
            }
        );
    }

    // --- rule 2: fenced out, lane evidence exception ---

    #[test]
    fn fenced_paths_are_allowed_for_non_lanes() {
        let repo = enrolled(&[]);
        for path in ["PULSE.md", "AGENTS.md", ".pulse/runtime/scratch.json"] {
            assert_eq!(
                decide(repo.path(), path, None),
                EditDecision::Allow { why: "fenced out" },
                "{path}"
            );
        }
    }

    #[test]
    fn a_lane_may_not_edit_the_harness_config_but_may_write_evidence() {
        let repo = enrolled(&[]);
        let lane = agent("review-correctness");
        assert!(matches!(
            decide(repo.path(), "PULSE.md", Some(&lane)),
            EditDecision::Deny { .. }
        ));
        assert!(matches!(
            decide(repo.path(), ".pulse/issues.jsonl", Some(&lane)),
            EditDecision::Deny { .. }
        ));
        assert_eq!(
            decide(
                repo.path(),
                ".pulse/evidence/TK-aaaa/review-correctness.json",
                Some(&lane)
            ),
            EditDecision::Allow { why: "fenced out" }
        );
    }

    #[test]
    fn a_fence_ignore_entry_is_allowed() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".pulse")).unwrap();
        std::fs::write(repo.path().join(".pulse/issues.jsonl"), b"").unwrap();
        std::fs::write(
            repo.path().join("PULSE.md"),
            "fence_ignore: [generated/**]\nprofiles: {}\n",
        )
        .unwrap();
        assert_eq!(
            decide(repo.path(), "generated/out.rs", None),
            EditDecision::Allow { why: "fenced out" }
        );
    }

    // --- rule 3: lanes do not edit source ---

    #[test]
    fn a_lane_is_denied_on_source_even_when_nothing_is_held() {
        let repo = enrolled(&[]);
        let err_free = decide(repo.path(), "src/lib.rs", Some(&agent("qa-ui")));
        assert!(matches!(err_free, EditDecision::Deny { .. }));
        // A human id that happens to start with a lane prefix is a human,
        // not a lane — identity kind is part of the actor, not the string.
        assert_eq!(
            decide(repo.path(), "src/lib.rs", Some(&human("review-boss"))),
            EditDecision::Allow {
                why: "no ticket is active"
            }
        );
    }

    // --- rule 4: nothing active ---

    #[test]
    fn with_nothing_active_the_default_policy_allows() {
        let repo = enrolled(&[]);
        assert_eq!(
            decide(repo.path(), "src/lib.rs", None),
            EditDecision::Allow {
                why: "no ticket is active"
            }
        );
    }

    #[test]
    fn with_nothing_active_unclaimed_deny_asks_for_a_claim() {
        let repo = enrolled(&[]);
        std::fs::write(
            repo.path().join("PULSE.md"),
            "hook: {unclaimed: deny}\nfence_ignore: []\nprofiles: {}\n",
        )
        .unwrap();
        let decision = decide(repo.path(), "src/lib.rs", None);
        assert!(matches!(decision, EditDecision::Deny { .. }));
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(message.contains("pulse frontier"), "{message}");
        assert!(message.contains("pulse claim"), "{message}");
    }

    // --- rule 5: an exclusive (touches-less) holder ---

    #[test]
    fn an_exclusive_holder_denies_every_other_actor() {
        let repo = enrolled(&[active_ticket("TK-aaaa", &[], "worker-1")]);
        let decision = decide(repo.path(), "src/anything.rs", Some(&agent("worker-2")));
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(message.contains("TK-aaaa"), "{message}");
        assert!(message.contains("exclusively"), "{message}");
    }

    #[test]
    fn an_exclusive_holder_allows_the_actor_less_edit() {
        // The hook decides by path; with no actor to compare there is
        // nothing to attribute the edit against, so the claim stands.
        let repo = enrolled(&[active_ticket("TK-aaaa", &[], "worker-1")]);
        assert_eq!(
            decide(repo.path(), "src/anything.rs", None),
            EditDecision::Allow {
                why: "an active ticket holds the whole tree"
            }
        );
        assert_eq!(
            decide(repo.path(), "src/anything.rs", Some(&agent("worker-1"))),
            EditDecision::Allow {
                why: "an active ticket holds the whole tree"
            }
        );
    }

    // --- rule 6: inside a held scope ---

    #[test]
    fn a_held_scope_allows_its_holder_and_denies_another_actor() {
        let repo = enrolled(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
        assert_eq!(
            decide(repo.path(), "src/deep/x.rs", Some(&agent("worker-1"))),
            EditDecision::Allow {
                why: "inside the active ticket's touches"
            }
        );
        let decision = decide(repo.path(), "src/deep/x.rs", Some(&agent("worker-2")));
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(message.contains("src/deep/x.rs"), "{message}");
        assert!(message.contains("TK-aaaa"), "{message}");
        assert!(message.contains("agent:worker-1"), "{message}");
    }

    #[test]
    fn a_disjoint_scope_stays_denied_for_outside_editors() {
        // The path is nobody's: not the holder's scope, so rule 8 decides
        // even though some ticket is active.
        let repo = enrolled(&[active_ticket("TK-aaaa", &["src/api/**"], "worker-1")]);
        let decision = decide(repo.path(), "web/app.js", Some(&agent("worker-1")));
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(message.contains("pulse reserve"), "{message}");
    }

    // --- rule 7: under review ---

    #[test]
    fn a_verifying_ticket_holds_its_files_against_everyone() {
        let repo = enrolled(&[
            active_ticket("TK-aaaa", &["web/**"], "worker-2"),
            verifying_ticket("TK-bbbb", &["src/**"]),
        ]);
        let decision = decide(repo.path(), "src/lib.rs", Some(&agent("worker-2")));
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(message.contains("under review in TK-bbbb"), "{message}");
    }

    // --- rule 8: outside every touches ---

    #[test]
    fn an_outside_path_names_pulse_reserve() {
        let repo = enrolled(&[active_ticket("TK-aaaa", &["src/**"], "worker-1")]);
        let decision = decide(repo.path(), "docs/notes.md", None);
        let EditDecision::Deny { message } = decision else {
            unreachable!()
        };
        assert!(
            message.contains("is outside every active ticket's `touches`"),
            "{message}"
        );
        assert!(
            message.contains("`pulse reserve <your ticket> docs/notes.md`"),
            "{message}"
        );
    }

    #[test]
    fn an_expired_lease_holds_nothing() {
        let repo = enrolled(&[{
            let mut ticket = active_ticket("TK-aaaa", &["src/**"], "worker-1");
            let expired = (Utc::now() - Duration::seconds(1)).to_rfc3339();
            ticket["lease"]["expires_at"] = json!(expired);
            ticket
        }]);
        // Nothing is held any more, so the default unclaimed policy decides.
        assert_eq!(
            decide(repo.path(), "src/lib.rs", None),
            EditDecision::Allow {
                why: "no ticket is active"
            }
        );
    }

    // --- internal failures ---

    #[test]
    fn a_broken_pulse_md_is_an_error_not_a_decision() {
        let repo = enrolled(&[]);
        std::fs::write(repo.path().join("PULSE.md"), "profiles: [not,a,map]\n").unwrap();
        assert!(pre_edit(repo.path(), "src/lib.rs", None).is_err());
    }

    #[test]
    fn a_torn_store_is_an_error_not_a_decision() {
        let repo = enrolled(&[]);
        std::fs::write(repo.path().join(".pulse/issues.jsonl"), b"{not json\n").unwrap();
        assert!(pre_edit(repo.path(), "src/lib.rs", None).is_err());
    }

    #[test]
    fn an_unknown_unclaimed_value_is_a_broken_config() {
        let repo = enrolled(&[]);
        std::fs::write(
            repo.path().join("PULSE.md"),
            "hook: {unclaimed: maybe}\nprofiles: {}\n",
        )
        .unwrap();
        let err = pre_edit(repo.path(), "src/lib.rs", None).unwrap_err();
        assert_eq!(err.code(), "pulse_md_invalid");
        assert!(err.hint().is_some());
    }
}
