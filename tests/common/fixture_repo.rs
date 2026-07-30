use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const FIXED_GIT_DATE: &str = "2026-01-01T00:00:00Z";

pub struct TestRepo {
    temp: TempDir,
}

impl TestRepo {
    pub fn from_fixture(name: &str) -> Self {
        let source = fixture_path(name);
        assert!(
            source.is_dir(),
            "target repository fixture does not exist: {}",
            source.display()
        );

        let temp = tempfile::Builder::new()
            .prefix("pulse-target-repo-")
            .tempdir()
            .expect("create target repository tempdir");
        assert_safe_target(temp.path()).expect("temporary target must be outside development repo");
        copy_directory_contents(&source, temp.path()).expect("copy target repository fixture");
        initialize_git_baseline(temp.path());

        Self { temp }
    }

    pub fn path(&self) -> &Path {
        self.temp.path()
    }

    pub fn pulse(&self, args: &[&str]) -> Output {
        assert_safe_target(self.path()).expect("refuse unsafe Pulse target");
        Command::new(pulse_binary())
            .arg("--repo-root")
            .arg(self.path())
            .args(args)
            .output()
            .expect("run Pulse against target repository fixture copy")
    }

    pub fn pulse_ok(&self, args: &[&str]) -> Value {
        let output = self.pulse(args);
        assert_success(&output, "Pulse command");
        serde_json::from_slice(&output.stdout).expect("Pulse JSON stdout")
    }

    pub fn run_verify(&self) -> Output {
        Command::new("node")
            .arg("scripts/verify.mjs")
            .current_dir(self.path())
            .output()
            .expect("run target repository verification")
    }

    pub fn git_head(&self) -> String {
        let output = git(self.path(), &["rev-parse", "HEAD"]);
        assert_success(&output, "git rev-parse HEAD");
        String::from_utf8(output.stdout)
            .expect("UTF-8 Git HEAD")
            .trim()
            .to_string()
    }

    pub fn git_is_clean(&self) -> bool {
        let output = git(self.path(), &["status", "--porcelain"]);
        assert_success(&output, "git status --porcelain");
        output.stdout.is_empty()
    }
}

pub fn fixture_path(name: &str) -> PathBuf {
    assert_valid_fixture_name(name);
    development_repo_root()
        .join("tests/fixtures/target-repos")
        .join(name)
}

pub fn development_repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .expect("canonical Pulse development repository root")
}

pub fn assert_safe_target(target: &Path) -> io::Result<()> {
    let root = development_repo_root();
    let target = target.canonicalize()?;

    if target == root || target.starts_with(&root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to use Pulse development repository as target: {}",
                target.display()
            ),
        ));
    }

    Ok(())
}

pub fn snapshot_tree(root: &Path) -> io::Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut snapshot = Vec::new();
    snapshot_directory(root, root, &mut snapshot)?;
    snapshot.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(snapshot)
}

fn assert_valid_fixture_name(name: &str) {
    let path = Path::new(name);
    assert!(!name.is_empty(), "fixture name must not be empty");
    assert!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "fixture name must be one safe path component: {name}"
    );
    assert_eq!(
        path.file_name(),
        Some(OsStr::new(name)),
        "fixture name must not contain path separators"
    );
}

fn pulse_binary() -> PathBuf {
    crate::common_bin::resolve_pulse_bin()
}

fn initialize_git_baseline(root: &Path) {
    let init = git(root, &["init", "-q"]);
    assert_success(&init, "git init");

    let add = git(root, &["add", "."]);
    assert_success(&add, "git add");

    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", FIXED_GIT_DATE)
        .env("GIT_COMMITTER_DATE", FIXED_GIT_DATE)
        .args([
            "-c",
            "user.name=Pulse Test",
            "-c",
            "user.email=pulse@example.test",
            "commit",
            "-q",
            "-m",
            "fixture baseline",
        ]);
    let commit = command.output().expect("git commit fixture baseline");
    assert_success(&commit, "git commit");
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run git")
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_directory_contents(source: &Path, destination: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());

        if file_type.is_dir() {
            fs::create_dir_all(&target)?;
            copy_directory_contents(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "fixture contains unsupported non-file entry: {}",
                    entry.path().display()
                ),
            ));
        }
    }

    Ok(())
}

fn snapshot_directory(
    root: &Path,
    current: &Path,
    snapshot: &mut Vec<(PathBuf, Vec<u8>)>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();

        if file_type.is_dir() {
            snapshot_directory(root, &path, snapshot)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("snapshot path below root")
                .to_path_buf();
            snapshot.push((relative, fs::read(path)?));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported snapshot entry: {}", path.display()),
            ));
        }
    }

    Ok(())
}
