//! CLI adapter for `pulse skills install`.
//!
//! This is the one Pulse command that is interactive by design: which
//! coding agents a repository uses is a fact only the person at the
//! terminal has, and detecting a config directory is evidence, not an
//! answer (a `.claude/` left behind by someone else's clone is not
//! consent). So the default path asks, and every non-interactive path
//! requires the hosts to be named explicitly — Pulse never picks a host
//! for you when it cannot ask. No detected host is not an error: the
//! canonical `.agents/skills/` bodies are host-independent and several
//! agents read them directly, so install proceeds and simply links
//! nothing.
//!
//! Scope is the repository, always: there is no `--global`. A skill body
//! is versioned with the Pulse binary that wrote it, and a user-level copy
//! would silently outrank the repo's own on the next `pulse skills
//! install` somewhere else.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use clap::Subcommand;

use crate::cli::output::render;
use crate::error::{PulseError, Result};
use crate::kernel::skills::{self, Host, InstallReport, CANONICAL_DIR};

#[derive(Subcommand)]
pub(crate) enum SkillsCommand {
    /// Write the Pulse skills into `.agents/skills/` and link them into
    /// the coding agents you choose. Asks which agents when run on a
    /// terminal; otherwise `--host` is required.
    Install {
        /// Install for this host without asking. Repeatable. Required
        /// when stdin is not a terminal.
        #[arg(long, value_name = "HOST")]
        host: Vec<String>,
        /// Link into every host detected in this repository, no prompt.
        #[arg(long, default_value_t = false, conflicts_with = "host")]
        all_detected: bool,
        /// Write `.agents/skills/` only, linking into no host.
        #[arg(long, default_value_t = false, conflicts_with_all = ["host", "all_detected"])]
        no_link: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show which known hosts this repository appears to use.
    Hosts {
        #[arg(long)]
        json: bool,
    },
}

pub(crate) fn handle(repo_root: &Path, command: SkillsCommand) -> Result<()> {
    match command {
        SkillsCommand::Install {
            host,
            all_detected,
            no_link,
            json,
        } => install(repo_root, &host, all_detected, no_link, json),
        SkillsCommand::Hosts { json } => hosts(repo_root, json),
    }
}

fn hosts(repo_root: &Path, json: bool) -> Result<()> {
    let detected = skills::detect(repo_root);
    let rows: Vec<serde_json::Value> = skills::HOSTS
        .iter()
        .map(|host| {
            serde_json::json!({
                "host": host.key,
                "label": host.label,
                "skills_dir": host.skills_dir,
                "reads_canonical": host.reads_canonical(),
                "detected": detected.iter().any(|found| found.key == host.key),
            })
        })
        .collect();
    let mut human = String::new();
    for host in &skills::HOSTS {
        let mark = if detected.iter().any(|found| found.key == host.key) {
            "detected"
        } else {
            "not detected"
        };
        human.push_str(&format!(
            "{:<10} {:<15} {:<22} {mark}\n",
            host.key,
            host.label,
            host.skills_dir.unwrap_or("reads .agents/skills"),
        ));
    }
    render(json, &serde_json::json!({"hosts": rows}), human)
}

fn install(
    repo_root: &Path,
    requested: &[String],
    all_detected: bool,
    no_link: bool,
    json: bool,
) -> Result<()> {
    let chosen: Vec<&'static Host> = if no_link {
        Vec::new()
    } else if !requested.is_empty() {
        requested
            .iter()
            .map(|key| skills::host(key))
            .collect::<Result<Vec<_>>>()?
    } else if all_detected {
        // Zero detected links zero hosts — the canonical install still
        // happens: several agents read `.agents/skills/` directly.
        skills::detect(repo_root)
    } else {
        choose_interactively(repo_root)?
    };

    let report = skills::install(repo_root, &chosen)?;
    let value = serde_json::json!({
        "canonical": CANONICAL_DIR,
        "written": report.written,
        "linked": report.linked.iter().map(|(link, target)| {
            serde_json::json!({"link": link, "target": target})
        }).collect::<Vec<_>>(),
        "already_linked": report.already_linked,
        "skipped": report.skipped.iter().map(|(path, why)| {
            serde_json::json!({"path": path, "why": why})
        }).collect::<Vec<_>>(),
    });
    let human = human_report(&report);
    render(json, &value, human)
}

fn human_report(report: &InstallReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} skill(s) written under {CANONICAL_DIR}/\n",
        report.written.len()
    ));
    for (link, target) in &report.linked {
        out.push_str(&format!("linked   {link} -> {target}\n"));
    }
    for link in &report.already_linked {
        out.push_str(&format!("already  {link}\n"));
    }
    for (path, why) in &report.skipped {
        out.push_str(&format!("skipped  {path} ({why} is already there)\n"));
    }
    if report.linked.is_empty() && report.already_linked.is_empty() {
        out.push_str(
            "no host linked — an agent that reads .agents/skills/ directly needs no link\n",
        );
    }
    out
}

/// Ask which detected hosts to link into. Refuses rather than guesses when
/// there is no terminal to ask at.
fn choose_interactively(repo_root: &Path) -> Result<Vec<&'static Host>> {
    if !std::io::stdin().is_terminal() {
        return Err(PulseError::kernel(
            "skills_hosts_unspecified",
            "no terminal to ask which coding agents to install for",
            "name them: `pulse skills install --host claude`, or \
             `--all-detected`, or `--no-link` to write .agents/skills/ only",
        ));
    }
    let detected = skills::detect(repo_root);
    if detected.is_empty() {
        // Nothing to ask about and nothing to refuse: the canonical
        // install serves agents that read `.agents/skills/` directly, and
        // the report says no host was linked.
        println!(
            "No known coding agent config directory found — \
                 writing {CANONICAL_DIR}/ only."
        );
        println!("Link one later with `pulse skills install --host <host>`.\n");
        return Ok(Vec::new());
    }

    let mut stdout = std::io::stdout();
    println!("Coding agents detected in this repository:\n");
    for (index, host) in detected.iter().enumerate() {
        match host.skills_dir {
            Some(dir) => println!("  {}) {:<15} links into {dir}/", index + 1, host.label),
            None => println!(
                "  {}) {:<15} reads .agents/skills itself — nothing to link",
                index + 1,
                host.label
            ),
        }
    }
    println!();
    print!("Install for which? [numbers, `a` for all, empty to skip linking]: ");
    stdout
        .flush()
        .map_err(|error| PulseError::io("stdout", error))?;

    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| PulseError::io("stdin", error))?;

    parse_choice(line.trim(), &detected)
}

/// `a`/`all` takes every detected host, an empty answer links nothing, and
/// anything else is read as 1-based indices separated by spaces or commas.
/// An index outside the list is an error, never a silent drop.
fn parse_choice(answer: &str, detected: &[&'static Host]) -> Result<Vec<&'static Host>> {
    if answer.is_empty() {
        return Ok(Vec::new());
    }
    if answer.eq_ignore_ascii_case("a") || answer.eq_ignore_ascii_case("all") {
        return Ok(detected.to_vec());
    }
    let mut chosen: Vec<&'static Host> = Vec::new();
    for token in answer.split([',', ' ']).filter(|token| !token.is_empty()) {
        let index: usize = token.parse().map_err(|_| bad_choice(token))?;
        let host = detected
            .get(index.wrapping_sub(1))
            .ok_or_else(|| bad_choice(token))?;
        if !chosen.iter().any(|already| already.key == host.key) {
            chosen.push(host);
        }
    }
    Ok(chosen)
}

fn bad_choice(token: &str) -> PulseError {
    PulseError::kernel(
        "skills_choice_invalid",
        format!("`{token}` is not one of the numbers offered"),
        "answer with the numbers listed, `a` for all, or nothing to skip linking",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected() -> Vec<&'static Host> {
        skills::HOSTS.iter().collect()
    }

    #[test]
    fn an_empty_answer_links_nothing() {
        assert!(parse_choice("", &detected()).unwrap().is_empty());
    }

    #[test]
    fn a_or_all_takes_every_detected_host() {
        assert_eq!(
            parse_choice("a", &detected()).unwrap().len(),
            skills::HOSTS.len()
        );
        assert_eq!(
            parse_choice("ALL", &detected()).unwrap().len(),
            skills::HOSTS.len()
        );
    }

    #[test]
    fn numbers_are_one_based_and_deduplicated() {
        let chosen = parse_choice("2, 2 1", &detected()).unwrap();
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0].key, skills::HOSTS[1].key);
        assert_eq!(chosen[1].key, skills::HOSTS[0].key);
    }

    #[test]
    fn an_index_outside_the_list_is_an_error_not_a_silent_drop() {
        // `0` is the trap: 1-based indices mean it must not wrap to the
        // last element through a usize underflow.
        let past_the_end = (skills::HOSTS.len() + 1).to_string();
        for token in ["0", past_the_end.as_str(), "x"] {
            let error = parse_choice(token, &detected()).unwrap_err();
            assert!(
                format!("{error}").contains(token),
                "`{token}` must be refused by name"
            );
        }
    }
}
