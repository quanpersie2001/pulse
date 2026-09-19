//! Ready gate (plan 0022 §7.1): `draft -> ready` for a Ticket or Story.
//!
//! Every condition is checked and every violation reported — the gate never
//! stops at the first failure, so one run tells the whole story instead of
//! costing one round trip per condition (the shape Track B hit repeatedly
//! with single-error gates).
//!
//! `ready_description_missing` is a seventh Ticket condition added after the
//! plan: a Ticket worked by an isolated agent needs the how, not only the
//! what, and that how is one free-form markdown `description` field.
//!
//! `ready_touches_missing` is an eighth Ticket condition added for parallel
//! work (decision 0025): a medium/high-risk Ticket declares the files it will
//! edit or create (`touches`), because that list is the parallel-claim key —
//! without it two workers can only run one at a time, and a repo-escaping
//! entry would silently exempt the Ticket from every file reservation.
//!
//! Two error codes here (`ready_outcome_missing`, `ready_rules_or_qa_missing`)
//! are not in the plan's literal list for §7.1, which only names the six
//! codes for an implementation Ticket. Story's gate needs its own codes for
//! the same reason a Ticket's does — reusing `ready_acceptance_missing` for
//! an "outcome" field would misname the violation. Documented here rather
//! than expanding the plan text itself.

use std::path::Path;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyViolation {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadyReport {
    pub violations: Vec<ReadyViolation>,
}

impl ReadyReport {
    pub fn is_ready(&self) -> bool {
        self.violations.is_empty()
    }
}

fn violation(code: &'static str, message: impl Into<String>) -> ReadyViolation {
    ReadyViolation {
        code,
        message: message.into(),
    }
}

fn find<'a>(all_records: &'a [Value], id: &str) -> Option<&'a Value> {
    all_records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
}

/// Evaluate the ready gate for `record` (kind `ticket` or `story`) against
/// every other record in the store, and the repository worktree for anchor
/// existence checks. An unrecognised or already-past-draft kind reports no
/// violations; the caller decides whether that kind has a `ready` gate at
/// all (only ticket and story do, per plan §4.7).
pub fn evaluate(repo_root: &Path, record: &Value, all_records: &[Value]) -> ReadyReport {
    let kind = record.get("kind").and_then(Value::as_str).unwrap_or("");
    let violations = match kind {
        "ticket" => evaluate_ticket(repo_root, record, all_records),
        "story" => evaluate_story(record),
        "epic" => evaluate_epic(record),
        _ => Vec::new(),
    };
    ReadyReport { violations }
}

fn evaluate_ticket(repo_root: &Path, record: &Value, all_records: &[Value]) -> Vec<ReadyViolation> {
    let mut violations = Vec::new();
    let role = record
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("implementation");

    // Condition 3: no blocking (or disposition-less) open question. Applies
    // to both roles.
    check_open_questions_not_blocking(record, &mut violations);
    // Condition 4: every blocked_by dep is done|cancelled. Applies to both
    // roles.
    check_blocked_by_deps(record, all_records, &mut violations);

    if role == "decision_work" {
        check_question_present(record, &mut violations);
    } else {
        check_acceptance(record, &mut violations);
        check_anchors_exist(repo_root, record, &mut violations);
        check_story_and_qa_cases(record, all_records, &mut violations);
        check_classification(record, &mut violations);
        check_description(record, &mut violations);
        check_touches(record, &mut violations);
    }
    violations
}

/// An Epic is the map of an effort (decision 0028): `outcome` is the
/// destination every Story under it orients to, and `success_signals[]` is
/// how anyone tells the destination was reached. Both are required before
/// the map is worth planning against — an Epic readied empty is the grouping
/// label Pulse used to allow, which no gate could read and no planner could
/// use. `out_of_scope[]`/`not_yet_specified[]` stay optional here: an effort
/// may genuinely have no fog yet, and both are read at close (§4.8).
fn evaluate_epic(record: &Value) -> Vec<ReadyViolation> {
    let mut violations = Vec::new();
    let outcome = record.get("outcome").and_then(Value::as_str).unwrap_or("");
    if outcome.trim().is_empty() {
        violations.push(violation(
            "ready_outcome_missing",
            "epic has no outcome; write the destination this effort is heading to",
        ));
    }
    let signals = record
        .get("success_signals")
        .and_then(Value::as_array)
        .map_or(0, |signals| {
            signals
                .iter()
                .filter(|signal| signal.as_str().is_some_and(|text| !text.trim().is_empty()))
                .count()
        });
    if signals == 0 {
        violations.push(violation(
            "ready_success_signals_missing",
            "epic has no success_signals; name how anyone tells the outcome was reached",
        ));
    }
    violations
}

fn evaluate_story(record: &Value) -> Vec<ReadyViolation> {
    let mut violations = Vec::new();
    check_open_questions_not_blocking(record, &mut violations);

    let outcome = record.get("outcome").and_then(Value::as_str).unwrap_or("");
    if outcome.trim().is_empty() {
        violations.push(violation(
            "ready_outcome_missing",
            "story has no outcome; write one before it can be ready",
        ));
    }

    let rule_count = record
        .get("rules")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let qa_case_count = record
        .get("qa_cases")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if rule_count == 0 && qa_case_count == 0 {
        violations.push(violation(
            "ready_rules_or_qa_missing",
            "story needs at least one rule or qa_case before it can be ready",
        ));
    }
    violations
}

// Condition 1.
fn check_acceptance(record: &Value, out: &mut Vec<ReadyViolation>) {
    let acceptance = record
        .get("acceptance")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if acceptance.is_empty() {
        out.push(violation(
            "ready_acceptance_missing",
            "ticket has no acceptance criteria; add at least one",
        ));
        return;
    }
    let mut seen_ids = std::collections::HashSet::new();
    for (index, criterion) in acceptance.iter().enumerate() {
        let id = criterion.get("id").and_then(Value::as_str).unwrap_or("");
        let when = criterion.get("when").and_then(Value::as_str).unwrap_or("");
        let then = criterion.get("then").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() || !seen_ids.insert(id.to_string()) {
            out.push(violation(
                "ready_acceptance_missing",
                format!("acceptance[{index}] has a missing or duplicate id"),
            ));
        }
        if when.trim().is_empty() || then.trim().is_empty() {
            out.push(violation(
                "ready_acceptance_missing",
                format!("acceptance {id} must have non-empty when/then"),
            ));
        }
    }
}

// Condition 2.
fn check_anchors_exist(repo_root: &Path, record: &Value, out: &mut Vec<ReadyViolation>) {
    let anchors = record
        .pointer("/context/anchors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for anchor in anchors {
        let Some(anchor) = anchor.as_str() else {
            continue;
        };
        let path_part = anchor.split(':').next().unwrap_or(anchor);
        if !repo_root.join(path_part).exists() {
            out.push(violation(
                "ready_anchor_missing",
                format!("anchor path does not exist on disk: {path_part}"),
            ));
        }
    }
}

// Condition 3.
fn check_open_questions_not_blocking(record: &Value, out: &mut Vec<ReadyViolation>) {
    let questions = record
        .get("open_questions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for question in questions {
        let disposition = question.get("disposition").and_then(Value::as_str);
        if !matches!(
            disposition,
            Some("resolved" | "rejected" | "delegated" | "deferred")
        ) {
            let text = question
                .get("q")
                .and_then(Value::as_str)
                .unwrap_or("(no question text)");
            out.push(violation(
                "ready_question_blocking",
                format!("open question is blocking or missing a disposition: {text}"),
            ));
        }
    }
}

// Condition 4.
fn check_blocked_by_deps(record: &Value, all_records: &[Value], out: &mut Vec<ReadyViolation>) {
    let deps = record
        .get("deps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for dep in deps {
        if dep.get("type").and_then(Value::as_str) != Some("blocked_by") {
            continue;
        }
        let Some(dep_id) = dep.get("id").and_then(Value::as_str) else {
            continue;
        };
        match find(all_records, dep_id) {
            Some(target) => {
                let status = target.get("status").and_then(Value::as_str).unwrap_or("");
                if status != "done" && status != "cancelled" {
                    out.push(violation(
                        "ready_blocked_by_open",
                        format!("blocked_by {dep_id} is still {status}"),
                    ));
                }
            }
            None => out.push(violation(
                "ready_blocked_by_open",
                format!("blocked_by {dep_id} does not exist"),
            )),
        }
    }
}

// Condition 5.
fn check_story_and_qa_cases(record: &Value, all_records: &[Value], out: &mut Vec<ReadyViolation>) {
    let Some(story_id) = record.get("story").and_then(Value::as_str) else {
        return;
    };
    let Some(story) = find(all_records, story_id) else {
        out.push(violation(
            "ready_qa_case_unresolved",
            format!("story {story_id} does not exist"),
        ));
        return;
    };
    let story_case_ids: std::collections::HashSet<&str> = story
        .get("qa_cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter_map(|case| case.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let ticket_cases = record
        .get("qa_cases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for case in ticket_cases {
        if let Some(case_id) = case.as_str() {
            if !story_case_ids.contains(case_id) {
                out.push(violation(
                    "ready_qa_case_unresolved",
                    format!("qa_case {case_id} is not defined on story {story_id}"),
                ));
            }
        }
    }
}

// Condition 6.
fn check_classification(record: &Value, out: &mut Vec<ReadyViolation>) {
    let risk_ok = matches!(record.get("risk"), Some(Value::String(_)));
    let surface_ok = matches!(record.get("surface"), Some(Value::String(_)));
    if !risk_ok || !surface_ok {
        out.push(violation(
            "ready_classification_missing",
            "risk and surface must both be set (not null) before ready",
        ));
    }
}

// Condition 7. `description` is free-form markdown on purpose: the gate
// checks that the how-to exists, never its shape. A low-risk ticket is the
// one-session quick route (AGENTS.md Pulse block) and may go without one.
fn check_description(record: &Value, out: &mut Vec<ReadyViolation>) {
    if record.get("risk").and_then(Value::as_str) == Some("low") {
        return;
    }
    let description = record
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    if description.trim().is_empty() {
        out.push(violation(
            "ready_description_missing",
            "a medium/high-risk ticket needs a description: markdown on how to implement it, \
             written for a worker that has no other context",
        ));
    }
}

// Condition 8. `touches` is the parallel-claim key (decision 0025): the
// files a worker will edit or create, as repo-relative globs in the
// `source::glob_match` grammar. As with `description`, a low-risk ticket is
// the one-session quick route and may go without one. Existence is
// deliberately not checked — a ticket may create new files.
fn check_touches(record: &Value, out: &mut Vec<ReadyViolation>) {
    if record.get("risk").and_then(Value::as_str) == Some("low") {
        return;
    }
    let entries = record
        .get("touches")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if entries.is_empty() {
        out.push(violation(
            "ready_touches_missing",
            "a medium/high-risk ticket needs touches: the files it will edit or create, \
             as repo-relative globs — the parallel-claim key",
        ));
        return;
    }
    for (index, entry) in entries.iter().enumerate() {
        let Some(path) = entry.as_str() else {
            out.push(violation(
                "ready_touches_missing",
                format!("touches[{index}] is not a string"),
            ));
            continue;
        };
        // The same validity rule `pulse reserve` enforces mid-run — one
        // shared check in kernel::scope (decision 0025 B4), so a touch the
        // ready gate accepted can never be refused by a later reserve, and
        // vice versa.
        if !crate::kernel::scope::valid_touch(path) {
            out.push(violation(
                "ready_touches_missing",
                format!(
                    "touches[{index}] ({path}) must be a non-empty repo-relative path that \
                     stays inside the repo",
                ),
            ));
        }
    }
}

fn check_question_present(record: &Value, out: &mut Vec<ReadyViolation>) {
    let question = record.get("question").and_then(Value::as_str).unwrap_or("");
    if question.trim().is_empty() {
        out.push(violation(
            "ready_acceptance_missing",
            "decision_work ticket needs a non-empty question",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_ticket() -> Value {
        json!({
            "schema": 3,
            "id": "TK-aaaa",
            "kind": "ticket",
            "title": "Do the thing",
            "status": "draft",
            "revision": 1,
            "created_at": "2026-09-16T00:00:00Z",
            "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
            "risk": "low",
            "surface": "cli",
            "acceptance": [
                {"id": "AC-1", "when": "a thing happens", "then": "it works"}
            ],
        })
    }

    fn repo() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn a_fully_specified_ticket_with_no_deps_or_story_is_ready() {
        let repo = repo();
        let report = evaluate(repo.path(), &base_ticket(), &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_1_missing_acceptance_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket.as_object_mut().unwrap().remove("acceptance");
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_acceptance_missing"));
    }

    #[test]
    fn condition_1_empty_when_then_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["acceptance"] = json!([{"id": "AC-1", "when": "", "then": "it works"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_acceptance_missing"));
    }

    #[test]
    fn condition_2_anchor_that_does_not_exist_on_disk_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["context"] = json!({"anchors": ["src/does/not/exist.rs"]});
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_anchor_missing"));
    }

    #[test]
    fn condition_2_anchor_that_exists_passes() {
        let repo = repo();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/lib.rs"), "").unwrap();
        let mut ticket = base_ticket();
        ticket["context"] = json!({"anchors": ["src/lib.rs:some_fn"]});
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_3_blocking_open_question_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["open_questions"] = json!([{"q": "which approach?", "disposition": "blocking"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_question_blocking"));
    }

    #[test]
    fn condition_3_open_question_missing_disposition_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["open_questions"] = json!([{"q": "which approach?"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_question_blocking"));
    }

    #[test]
    fn condition_3_resolved_open_question_passes() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["open_questions"] =
            json!([{"q": "which approach?", "disposition": "resolved", "answer": "A"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_4_open_blocked_by_dep_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["deps"] = json!([{"type": "blocked_by", "id": "TK-bbbb"}]);
        let blocker = json!({
            "schema": 3, "id": "TK-bbbb", "kind": "ticket", "title": "Blocker",
            "status": "active", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
        });
        let report = evaluate(repo.path(), &ticket, &[blocker]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_blocked_by_open"));
    }

    #[test]
    fn condition_4_missing_blocked_by_target_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["deps"] = json!([{"type": "blocked_by", "id": "TK-missing"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_blocked_by_open"));
    }

    #[test]
    fn condition_4_done_blocked_by_dep_passes() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["deps"] = json!([{"type": "blocked_by", "id": "TK-bbbb"}]);
        let blocker = json!({
            "schema": 3, "id": "TK-bbbb", "kind": "ticket", "title": "Blocker",
            "status": "done", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "implementation",
        });
        let report = evaluate(repo.path(), &ticket, &[blocker]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_5_ticket_qa_case_not_on_story_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["story"] = json!("ST-cccc");
        ticket["qa_cases"] = json!(["QA-001"]);
        let story = json!({
            "schema": 3, "id": "ST-cccc", "kind": "story", "title": "Story",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "something", "qa_cases": [],
        });
        let report = evaluate(repo.path(), &ticket, &[story]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_qa_case_unresolved"));
    }

    #[test]
    fn condition_5_story_that_does_not_exist_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["story"] = json!("ST-missing");
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_qa_case_unresolved"));
    }

    #[test]
    fn condition_5_ticket_qa_case_on_story_passes() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["story"] = json!("ST-cccc");
        ticket["qa_cases"] = json!(["QA-001"]);
        let story = json!({
            "schema": 3, "id": "ST-cccc", "kind": "story", "title": "Story",
            "status": "ready", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "something", "qa_cases": [{"id": "QA-001", "intent": "x"}],
        });
        let report = evaluate(repo.path(), &ticket, &[story]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_6_missing_risk_or_surface_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = Value::Null;
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_classification_missing"));
    }

    #[test]
    fn decision_work_ticket_only_checks_open_questions_blocked_by_and_question() {
        let repo = repo();
        let ticket = json!({
            "schema": 3, "id": "TK-dddd", "kind": "ticket", "title": "Decide something",
            "status": "draft", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "decision_work",
            "question": "should we use X or Y?",
        });
        // No acceptance, no risk/surface, no anchors — none of that applies.
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn decision_work_ticket_without_a_question_is_reported() {
        let repo = repo();
        let ticket = json!({
            "schema": 3, "id": "TK-dddd", "kind": "ticket", "title": "Decide something",
            "status": "draft", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "role": "decision_work",
            "question": "",
        });
        let report = evaluate(repo.path(), &ticket, &[]);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].code, "ready_acceptance_missing");
    }

    #[test]
    fn every_violation_is_reported_at_once_not_just_the_first() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket.as_object_mut().unwrap().remove("acceptance");
        ticket["risk"] = Value::Null;
        ticket["open_questions"] = json!([{"q": "x", "disposition": "blocking"}]);
        let report = evaluate(repo.path(), &ticket, &[]);
        let codes: std::collections::HashSet<_> =
            report.violations.iter().map(|v| v.code).collect();
        assert!(codes.contains("ready_acceptance_missing"));
        assert!(codes.contains("ready_classification_missing"));
        assert!(codes.contains("ready_question_blocking"));
    }

    #[test]
    fn story_ready_gate_requires_outcome_and_a_rule_or_qa_case() {
        let repo = repo();
        let story = json!({
            "schema": 3, "id": "ST-eeee", "kind": "story", "title": "Story",
            "status": "draft", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "",
        });
        let report = evaluate(repo.path(), &story, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_outcome_missing"));
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_rules_or_qa_missing"));
    }

    #[test]
    fn story_ready_gate_passes_with_outcome_and_one_rule() {
        let repo = repo();
        let story = json!({
            "schema": 3, "id": "ST-eeee", "kind": "story", "title": "Story",
            "status": "draft", "revision": 1,
            "created_at": "2026-09-16T00:00:00Z", "updated_at": "2026-09-16T00:00:00Z",
            "outcome": "users can do the thing",
            "rules": [{"id": "BR-1", "text": "must be fast"}],
        });
        let report = evaluate(repo.path(), &story, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_7_a_medium_risk_ticket_without_a_description_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = json!("medium");
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_description_missing"));

        ticket["description"] = json!("   ");
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(!report.is_ready(), "whitespace is not a description");
    }

    #[test]
    fn condition_7_any_markdown_description_satisfies_the_gate() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = json!("high");
        ticket["description"] = json!("## How\nExtend `foo()` the way `bar()` does.");
        ticket["touches"] = json!(["src/lib.rs"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_8_a_medium_risk_ticket_without_touches_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = json!("medium");
        ticket["description"] = json!("## How\nDo it.");
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_touches_missing"));

        ticket["touches"] = json!([]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(!report.is_ready(), "an empty touches list claims nothing");

        ticket["touches"] = json!(["src/lib.rs"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_8_an_absolute_or_parent_escaping_touch_is_reported() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = json!("medium");
        ticket["description"] = json!("## How\nDo it.");
        ticket["touches"] = json!(["/etc/passwd"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_touches_missing"));

        ticket["touches"] = json!(["../escape.rs"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_touches_missing"));

        ticket["touches"] = json!(["src/../../outside.rs"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_touches_missing"));

        ticket["touches"] = json!([""]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_touches_missing"));
    }

    #[test]
    fn condition_8_low_risk_ticket_may_omit_touches() {
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["description"] = json!("## How\nDo it.");
        assert_eq!(ticket["risk"], json!("low"));
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }

    #[test]
    fn condition_8_a_glob_touch_that_covers_new_files_passes() {
        // A touch names files that may not exist yet, so unlike an anchor it
        // is never checked against disk — only against the path grammar.
        let repo = repo();
        let mut ticket = base_ticket();
        ticket["risk"] = json!("medium");
        ticket["description"] = json!("## How\nDo it.");
        ticket["touches"] = json!(["src/generated/*.rs", "docs/new-dir/**"]);
        let report = evaluate(repo.path(), &ticket, &[]);
        assert!(report.is_ready(), "{:?}", report.violations);
    }
    #[test]
    fn an_epic_without_outcome_or_success_signals_is_not_ready() {
        let repo = tempfile::tempdir().unwrap();
        let epic = json!({"id": "EP-1111", "kind": "epic", "title": "e"});
        let report = evaluate(repo.path(), &epic, &[]);
        let codes: Vec<&str> = report.violations.iter().map(|v| v.code).collect();
        assert!(codes.contains(&"ready_outcome_missing"), "{codes:?}");
        assert!(
            codes.contains(&"ready_success_signals_missing"),
            "{codes:?}"
        );
    }

    #[test]
    fn an_epic_with_a_destination_and_a_signal_is_ready() {
        let repo = tempfile::tempdir().unwrap();
        let epic = json!({
            "id": "EP-1111", "kind": "epic", "title": "e",
            "outcome": "a user can manage tags",
            "success_signals": ["tag filter used in a real session"],
        });
        assert!(evaluate(repo.path(), &epic, &[]).is_ready());
    }

    #[test]
    fn a_blank_success_signal_does_not_count() {
        let repo = tempfile::tempdir().unwrap();
        let epic = json!({
            "id": "EP-1111", "kind": "epic", "title": "e",
            "outcome": "x", "success_signals": ["   "],
        });
        let report = evaluate(repo.path(), &epic, &[]);
        assert!(report
            .violations
            .iter()
            .any(|v| v.code == "ready_success_signals_missing"));
    }
}
