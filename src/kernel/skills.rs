//! `pulse skills install`: put the guidance skills where the coding agents
//! working in this repository will find them.
//!
//! State touched: `.agents/skills/<name>/SKILL.md` (the canonical copy,
//! rewritten from the embedded template on every install) and one symlink
//! per chosen host, `<host skills dir>/<name>` -> the canonical directory.
//! Nothing outside the repository root is ever written: skills are a
//! per-project surface, never a user-level one, so a repo carries its own
//! version of them and two repos on different Pulse versions do not fight.
//!
//! Why a symlink and not a copy: a copy is a second source of truth that
//! goes stale silently the moment a skill is refreshed, and a host that
//! reads a stale copy is worse than one that reads nothing. The symlink
//! makes `.agents/skills/` the only place a skill body lives.
//!
//! Invariant: a host is only ever offered when its directory convention
//! was verified, the same rule `kernel::hook` applies to hook snippets — an
//! invented path would write a link no agent ever reads. The interactive
//! choice itself belongs to the CLI layer; this module decides what is
//! installable and performs the writes.

use std::path::{Path, PathBuf};

use crate::error::{PulseError, Result};

/// Where the canonical skill bodies live inside a target repo. Chosen to
/// match the vendor-neutral convention (`.agents/`) rather than any one
/// host's directory, so the copy that is read by everything is not owned
/// by whichever agent happened to be installed first.
pub const CANONICAL_DIR: &str = ".agents/skills";

/// One file inside a shipped skill: path relative to the skill directory,
/// and its body.
struct SkillFile {
    rel: &'static str,
    body: &'static str,
}

/// One shipped skill: directory name and every file it carries. A skill is
/// a directory, not a single `SKILL.md` — `SKILL.md` holds the decisions
/// and links to `references/*.md` that are read only when needed, so the
/// installer must copy the tree, not one file.
struct Skill {
    name: &'static str,
    files: &'static [SkillFile],
}

const SKILLS: [Skill; 3] = [
    Skill {
        name: "pulse-shape",
        files: &[
            SkillFile {
                rel: "SKILL.md",
                body: include_str!("../../templates/skills/pulse-shape/SKILL.md"),
            },
            SkillFile {
                rel: "references/record-payloads.md",
                body: include_str!(
                    "../../templates/skills/pulse-shape/references/record-payloads.md"
                ),
            },
            SkillFile {
                rel: "references/prose-homes.md",
                body: include_str!("../../templates/skills/pulse-shape/references/prose-homes.md"),
            },
        ],
    },
    Skill {
        name: "pulse-plan",
        files: &[
            SkillFile {
                rel: "SKILL.md",
                body: include_str!("../../templates/skills/pulse-plan/SKILL.md"),
            },
            SkillFile {
                rel: "references/ticket-payload.md",
                body: include_str!(
                    "../../templates/skills/pulse-plan/references/ticket-payload.md"
                ),
            },
        ],
    },
    Skill {
        name: "pulse-learn",
        files: &[
            SkillFile {
                rel: "SKILL.md",
                body: include_str!("../../templates/skills/pulse-learn/SKILL.md"),
            },
            SkillFile {
                rel: "references/learning-record.md",
                body: include_str!(
                    "../../templates/skills/pulse-learn/references/learning-record.md"
                ),
            },
            SkillFile {
                rel: "references/loop-hygiene.md",
                body: include_str!("../../templates/skills/pulse-learn/references/loop-hygiene.md"),
            },
        ],
    },
];

/// A coding agent whose project-level skill directory Pulse knows.
///
/// `probe` is what proves the agent is actually used in this repository —
/// its own config directory. `skills_dir` is where that agent looks for
/// project skills, and is `None` for a host that already reads
/// [`CANONICAL_DIR`] itself: writing the canonical copy *is* installing
/// for that host, and a symlink would be a second path to the same bytes.
/// Both are repo-relative.
#[derive(Debug)]
pub struct Host {
    pub key: &'static str,
    pub label: &'static str,
    pub probe: &'static str,
    pub skills_dir: Option<&'static str>,
}

impl Host {
    /// True when the host finds the canonical copy without a link.
    pub fn reads_canonical(&self) -> bool {
        self.skills_dir.is_none()
    }
}

/// Hosts whose project-level skills directory is a verified convention.
///
/// Every path here was read out of a reference implementation that
/// inventories each host's real search paths
/// (`references/better-harness/scripts/agent-customize/providers/*.mjs`),
/// not out of documentation or a guess: a link under an invented path is
/// dead weight no agent ever reads. A host missing from this table is
/// refused by name.
///
/// The `None` rows are the important ones. `.agents/skills` is the
/// emerging cross-host convention, and for those hosts Pulse has nothing
/// to link — it reports them as already covered instead of inventing work.
pub const HOSTS: [Host; 7] = [
    Host {
        key: "claude",
        label: "Claude Code",
        probe: ".claude",
        skills_dir: Some(".claude/skills"),
    },
    Host {
        key: "copilot",
        label: "GitHub Copilot",
        probe: ".github",
        skills_dir: None,
    },
    Host {
        key: "qoder",
        label: "Qoder",
        probe: ".qoder",
        skills_dir: None,
    },
    Host {
        key: "grok",
        label: "Grok",
        probe: ".grok",
        skills_dir: None,
    },
    Host {
        key: "dsh",
        label: "DSH",
        probe: ".dsh",
        skills_dir: None,
    },
    Host {
        key: "kimi",
        label: "Kimi Code",
        probe: ".kimi-code",
        skills_dir: Some(".kimi-code/skills"),
    },
    Host {
        key: "opencode",
        label: "opencode",
        probe: ".opencode",
        skills_dir: Some(".opencode/skill"),
    },
];

/// The hint every "which host?" error carries. Static because
/// [`PulseError::kernel`] hints are `&'static str`;
/// [`tests::the_known_hosts_hint_names_every_host`] keeps it from drifting
/// away from [`HOSTS`].
pub const KNOWN_HOSTS_HINT: &str =
    "known hosts: claude, copilot, qoder, grok, dsh, kimi, opencode. \
     Several read `.agents/skills` themselves, so installing needs no host at all";

pub fn host(key: &str) -> Result<&'static Host> {
    HOSTS.iter().find(|host| host.key == key).ok_or_else(|| {
        PulseError::kernel(
            "skills_host_unknown",
            format!("no known skills directory for host `{key}`"),
            KNOWN_HOSTS_HINT,
        )
    })
}

/// Hosts whose config directory exists in this repository — the ones worth
/// offering by default. A host is *detected*, never assumed: an empty
/// result means the caller asks rather than picking for the user.
pub fn detect(repo_root: &Path) -> Vec<&'static Host> {
    HOSTS
        .iter()
        .filter(|host| repo_root.join(host.probe).exists())
        .collect()
}

/// What one install did, for the report the CLI renders.
#[derive(Debug, PartialEq, Eq)]
pub struct InstallReport {
    /// Canonical `SKILL.md` paths written or rewritten.
    pub written: Vec<String>,
    /// Links created, as `(path, target)`.
    pub linked: Vec<(String, String)>,
    /// Links that already pointed at the canonical directory.
    pub already_linked: Vec<String>,
    /// Paths left alone because something that is not our link sits there.
    pub skipped: Vec<(String, String)>,
}

/// Write the canonical skill bodies, then link them into each host's
/// skills directory.
///
/// Rewriting `.agents/skills/**/SKILL.md` on every run is deliberate: the
/// skill body is Pulse's to own (unlike a prompt, which `pulse init
/// --refresh` three-way merges because a repo is expected to edit it). A
/// host path that exists and is *not* our symlink is never replaced — a
/// real directory there is someone's own skill, and clobbering it would
/// trade a missing skill for a lost one.
///
/// # Errors
/// `io_error` if the repository tree cannot be written.
pub fn install(repo_root: &Path, hosts: &[&Host]) -> Result<InstallReport> {
    let mut report = InstallReport {
        written: Vec::new(),
        linked: Vec::new(),
        already_linked: Vec::new(),
        skipped: Vec::new(),
    };

    for skill in &SKILLS {
        let dir = repo_root.join(CANONICAL_DIR).join(skill.name);
        for file in skill.files {
            let path = dir.join(file.rel);
            let parent = path.parent().expect("a skill file always has a parent");
            std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
            std::fs::write(&path, file.body).map_err(|error| PulseError::io(&path, error))?;
            report
                .written
                .push(format!("{CANONICAL_DIR}/{}/{}", skill.name, file.rel));
        }
    }

    for host in hosts {
        let Some(skills_dir) = host.skills_dir else {
            // Nothing to do: this host reads CANONICAL_DIR itself, and the
            // canonical bodies were just written above.
            report
                .already_linked
                .push(format!("{} reads {CANONICAL_DIR} directly", host.label));
            continue;
        };
        let host_dir = repo_root.join(skills_dir);
        std::fs::create_dir_all(&host_dir).map_err(|error| PulseError::io(&host_dir, error))?;
        for skill in &SKILLS {
            let link = host_dir.join(skill.name);
            let target = link_target(skills_dir, skill.name);
            let shown = format!("{skills_dir}/{}", skill.name);
            match std::fs::symlink_metadata(&link) {
                Ok(metadata) => {
                    if metadata.is_symlink() && std::fs::read_link(&link).ok() == Some(target) {
                        report.already_linked.push(shown);
                    } else {
                        let kind = if metadata.is_symlink() {
                            "a symlink pointing somewhere else"
                        } else if metadata.is_dir() {
                            "a real directory"
                        } else {
                            "a file"
                        };
                        report.skipped.push((shown, kind.to_string()));
                    }
                }
                Err(_) => {
                    symlink(&target, &link)?;
                    report
                        .linked
                        .push((shown, target.to_string_lossy().into_owned()));
                }
            }
        }
    }

    Ok(report)
}

/// The relative path from a host's skills directory back to one canonical
/// skill directory. Relative so the link survives the repository being
/// moved or cloned to another machine.
fn link_target(host_skills_dir: &str, skill_name: &str) -> PathBuf {
    let depth = host_skills_dir.split('/').filter(|s| !s.is_empty()).count();
    let mut target = PathBuf::new();
    for _ in 0..depth {
        target.push("..");
    }
    target.push(CANONICAL_DIR);
    target.push(skill_name);
    target
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).map_err(|error| PulseError::io(link, error))
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::windows::fs::symlink_dir(target, link).map_err(|error| PulseError::io(link, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped_file_count() -> usize {
        SKILLS.iter().map(|skill| skill.files.len()).sum()
    }

    #[test]
    fn link_target_climbs_out_of_the_host_directory() {
        assert_eq!(
            link_target(".claude/skills", "pulse-plan"),
            PathBuf::from("../../.agents/skills/pulse-plan")
        );
        assert_eq!(
            link_target(".opencode/skill", "pulse-shape"),
            PathBuf::from("../../.agents/skills/pulse-shape")
        );
    }

    #[test]
    fn the_known_hosts_hint_names_every_host() {
        for host in &HOSTS {
            assert!(
                KNOWN_HOSTS_HINT.contains(host.key),
                "the hint must name `{}` — a host nobody is told about is unreachable",
                host.key
            );
        }
        assert!(KNOWN_HOSTS_HINT.contains(CANONICAL_DIR));
    }

    #[test]
    fn detect_reports_only_hosts_whose_config_dir_exists() {
        let repo = tempfile::tempdir().unwrap();
        assert!(detect(repo.path()).is_empty());
        std::fs::create_dir_all(repo.path().join(".claude")).unwrap();
        let found = detect(repo.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].key, "claude");
    }

    #[test]
    fn an_unknown_host_is_refused_with_the_known_list() {
        let error = host("emacs").unwrap_err();
        let rendered = format!("{error}");
        assert!(rendered.contains("emacs"), "{rendered}");
    }

    #[test]
    fn install_writes_the_canonical_bodies_and_links_the_host() {
        let repo = tempfile::tempdir().unwrap();
        let report = install(repo.path(), &[host("claude").unwrap()]).unwrap();
        assert_eq!(report.written.len(), shipped_file_count());
        assert_eq!(report.linked.len(), SKILLS.len());
        assert!(report.skipped.is_empty());

        let canonical = repo.path().join(".agents/skills/pulse-plan/SKILL.md");
        assert!(canonical.is_file());
        // The link resolves to the canonical body, which is what proves the
        // relative target is right rather than merely well-formed.
        let through_link =
            std::fs::read_to_string(repo.path().join(".claude/skills/pulse-plan/SKILL.md"))
                .expect("the host link must resolve to the canonical skill");
        assert_eq!(through_link, std::fs::read_to_string(&canonical).unwrap());
    }

    #[test]
    fn installing_twice_reports_the_links_as_already_present() {
        let repo = tempfile::tempdir().unwrap();
        let claude = host("claude").unwrap();
        install(repo.path(), &[claude]).unwrap();
        let second = install(repo.path(), &[claude]).unwrap();
        assert!(second.linked.is_empty(), "{second:?}");
        assert_eq!(second.already_linked.len(), SKILLS.len());
    }

    #[test]
    fn a_real_directory_in_the_host_path_is_never_replaced() {
        let repo = tempfile::tempdir().unwrap();
        let theirs = repo.path().join(".claude/skills/pulse-plan");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("SKILL.md"), "mine, not Pulse's\n").unwrap();

        let report = install(repo.path(), &[host("claude").unwrap()]).unwrap();
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].0.ends_with("pulse-plan"), "{report:?}");
        assert_eq!(
            std::fs::read_to_string(theirs.join("SKILL.md")).unwrap(),
            "mine, not Pulse's\n"
        );
    }

    #[test]
    fn install_with_no_host_still_writes_the_canonical_copy() {
        let repo = tempfile::tempdir().unwrap();
        let report = install(repo.path(), &[]).unwrap();
        assert_eq!(report.written.len(), shipped_file_count());
        assert!(report.linked.is_empty());
        assert!(repo
            .path()
            .join(".agents/skills/pulse-shape/SKILL.md")
            .is_file());
    }
}
