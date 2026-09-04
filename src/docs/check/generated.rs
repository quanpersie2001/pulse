//! Execution of declared generated-document freshness checks.
//!
//! Commands run as direct argv with the target repository as working directory.
//! No shell expansion is available, and outcomes are cached per exact command
//! during one validation run.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use crate::docs::model::{DocsRegistry, DocumentRecord, DocumentStatus};
use crate::docs::validate::DocsFinding;

const MAX_COMMAND_OUTPUT_BYTES: usize = 4096;

pub(super) fn validate_generated_freshness(
    repo_root: &Path,
    registry: &DocsRegistry,
    errors: &mut Vec<DocsFinding>,
) -> usize {
    let documents: Vec<_> = registry
        .documents
        .iter()
        .filter(|document| document.status != DocumentStatus::Retired)
        .filter_map(|document| {
            document
                .generated
                .as_ref()
                .map(|contract| (document, contract))
        })
        .collect();
    let mut outcomes: BTreeMap<String, Result<Output, String>> = BTreeMap::new();
    for (_, contract) in &documents {
        outcomes
            .entry(contract.freshness_check.clone())
            .or_insert_with(|| run_freshness_check(repo_root, &contract.freshness_check));
    }
    for (document, contract) in &documents {
        match outcomes.get(&contract.freshness_check) {
            Some(Ok(output)) if output.status.success() => {}
            Some(Ok(output)) => errors.push(finding(
                "docs_generated_stale",
                failed_check_message(&contract.freshness_check, output),
                document,
            )),
            Some(Err(message)) => errors.push(finding(
                "docs_generated_freshness_check_failed",
                message.clone(),
                document,
            )),
            None => unreachable!("every generated document has a cached check outcome"),
        }
    }
    documents.len()
}

fn run_freshness_check(repo_root: &Path, declared: &str) -> Result<Output, String> {
    let argv = parse_argv(declared)
        .map_err(|message| format!("invalid generated freshness_check {declared:?}: {message}"))?;
    let Some((executable, arguments)) = argv.split_first() else {
        return Err("generated freshness_check must not be empty".to_string());
    };
    Command::new(executable)
        .args(arguments)
        .current_dir(repo_root)
        .output()
        .map_err(|error| {
            format!("failed to execute generated freshness_check {declared:?}: {error}")
        })
}

fn parse_argv(command: &str) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' && quote != Some('\'') {
            let escapable = characters.peek().is_some_and(|next| {
                *next == '\\'
                    || *next == '"'
                    || (quote.is_none() && (*next == '\'' || next.is_whitespace()))
            });
            if escapable {
                current.push(characters.next().expect("peeked character exists"));
            } else {
                current.push(character);
            }
            started = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
                started = true;
                continue;
            }
            if quote.is_none() {
                quote = Some(character);
                started = true;
                continue;
            }
        }
        if character.is_whitespace() && quote.is_none() {
            if started {
                argv.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(character);
            started = true;
        }
    }
    if quote.is_some() {
        return Err("unterminated quote".to_string());
    }
    if started {
        argv.push(current);
    }
    if argv.is_empty() {
        return Err("empty command".to_string());
    }
    Ok(argv)
}

fn failed_check_message(declared: &str, output: &Output) -> String {
    let status = output.status.code().map_or_else(
        || "terminated by signal".to_string(),
        |code| format!("exit {code}"),
    );
    let detail = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    let detail = String::from_utf8_lossy(&detail[..detail.len().min(MAX_COMMAND_OUTPUT_BYTES)]);
    if detail.trim().is_empty() {
        format!("generated freshness_check {declared:?} reported stale output ({status})")
    } else {
        format!(
            "generated freshness_check {declared:?} reported stale output ({status}): {}",
            detail.trim()
        )
    }
}

fn finding(
    code: impl Into<String>,
    message: impl Into<String>,
    document: &DocumentRecord,
) -> DocsFinding {
    DocsFinding {
        code: code.into(),
        message: message.into(),
        document_id: Some(document.id.clone()),
        path: Some(document.path.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_argv;

    #[test]
    fn argv_parser_preserves_quoted_arguments_without_shell_expansion() {
        assert_eq!(
            parse_argv(r#"tool --path "docs/generated api" '$HOME' && next"#).unwrap(),
            vec![
                "tool",
                "--path",
                "docs/generated api",
                "$HOME",
                "&&",
                "next"
            ]
        );
        assert!(parse_argv("tool 'unterminated").is_err());
        assert_eq!(
            parse_argv(r#""C:\Program Files\Pulse\pulse.exe" docs validate"#).unwrap(),
            vec![r"C:\Program Files\Pulse\pulse.exe", "docs", "validate"]
        );
    }
}
