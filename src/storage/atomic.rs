use crate::error::{PulseError, Result};
use rand::RngCore;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicWriteReport {
    pub path: PathBuf,
    pub temp_path: PathBuf,
    pub parent_fsync_attempted: bool,
    pub parent_fsync_succeeded: bool,
    pub durability_note: Option<String>,
}

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<AtomicWriteReport> {
    let parent = prepare_parent(path)?;
    let temp_path = write_synced_temp(path, bytes)?;

    let result = replace_file(&temp_path, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result?;

    let (attempted, succeeded, note) = fsync_parent_dir(parent);
    Ok(AtomicWriteReport {
        path: path.to_path_buf(),
        temp_path,
        parent_fsync_attempted: attempted,
        parent_fsync_succeeded: succeeded,
        durability_note: note,
    })
}

pub fn atomic_create_new(path: &Path, bytes: &[u8]) -> Result<AtomicWriteReport> {
    let parent = prepare_parent(path)?;
    let temp_path = write_synced_temp(path, bytes)?;

    let result = create_file_from_temp(&temp_path, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result?;
    let _ = fs::remove_file(&temp_path);

    let (attempted, succeeded, note) = fsync_parent_dir(parent);
    Ok(AtomicWriteReport {
        path: path.to_path_buf(),
        temp_path,
        parent_fsync_attempted: attempted,
        parent_fsync_succeeded: succeeded,
        durability_note: note,
    })
}

fn prepare_parent(path: &Path) -> Result<&Path> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation(
            "invalid_path",
            format!("target has no parent directory: {}", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    Ok(parent)
}

fn write_synced_temp(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let temp_path = unique_temp_path(path);
    let mut temp = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| PulseError::io(&temp_path, error))?;

    let result = (|| -> Result<()> {
        temp.write_all(bytes)
            .map_err(|error| PulseError::io(&temp_path, error))?;
        temp.flush()
            .map_err(|error| PulseError::io(&temp_path, error))?;
        temp.sync_all()
            .map_err(|error| PulseError::io(&temp_path, error))?;
        Ok(())
    })();
    drop(temp);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result?;
    Ok(temp_path)
}

pub fn remove_file(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation(
            "invalid_path",
            format!("target has no parent directory: {}", path.display()),
        )
    })?;
    fs::remove_file(path).map_err(|error| PulseError::io(path, error))?;
    let _ = fsync_parent_dir(parent);
    Ok(())
}

pub fn cleanup_orphan_temps(directory: &Path) -> Result<usize> {
    if !directory.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(directory).map_err(|error| PulseError::io(directory, error))? {
        let entry = entry.map_err(|error| PulseError::io(directory, error))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".pulse-tmp-"))
        {
            fs::remove_file(&path).map_err(|error| PulseError::io(&path, error))?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(unix)]
fn replace_file(temp_path: &Path, target: &Path) -> Result<()> {
    fs::rename(temp_path, target).map_err(|error| PulseError::io(target, error))
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, target: &Path) -> Result<()> {
    if target.exists() {
        fs::remove_file(target).map_err(|error| PulseError::io(target, error))?;
    }
    fs::rename(temp_path, target).map_err(|error| PulseError::io(target, error))
}

fn create_file_from_temp(temp_path: &Path, target: &Path) -> Result<()> {
    fs::hard_link(temp_path, target).map_err(|error| PulseError::io(target, error))
}

#[cfg(unix)]
fn fsync_parent_dir(parent: &Path) -> (bool, bool, Option<String>) {
    match File::open(parent).and_then(|dir| dir.sync_all()) {
        Ok(()) => (true, true, None),
        Err(error) => (
            true,
            false,
            Some(format!(
                "parent directory fsync failed; atomic name replace happened but crash durability \
                 is filesystem-dependent: {error}"
            )),
        ),
    }
}

#[cfg(windows)]
fn fsync_parent_dir(_parent: &Path) -> (bool, bool, Option<String>) {
    (
        false,
        false,
        Some(
            "parent directory fsync is not implemented on Windows; atomic replace \
             is best-effort and crash durability is filesystem-dependent"
                .to_string(),
        ),
    )
}

fn unique_temp_path(target: &Path) -> PathBuf {
    let mut random = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("target");
    target.with_file_name(format!(".{name}.pulse-tmp-{}", hex::encode(random)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replace_replaces_existing_file() -> Result<()> {
        let tmp = tempfile::tempdir().map_err(|error| PulseError::io("<tempdir>", error))?;
        let path = tmp.path().join("file.json");
        fs::write(&path, b"old").map_err(|error| PulseError::io(&path, error))?;
        let report = atomic_replace(&path, b"new")?;
        assert_eq!(
            fs::read(&path).map_err(|error| PulseError::io(&path, error))?,
            b"new"
        );
        assert!(!report.temp_path.exists());
        Ok(())
    }
}
