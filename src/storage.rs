use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;
use uuid::Uuid;

use crate::{PulseError, PulseResult};

pub struct WriteGuard {
    file: File,
}

impl WriteGuard {
    pub fn acquire(repo_root: &Path) -> PulseResult<Self> {
        let lock_dir = repo_root.join(".pulse/runtime/locks");
        fs::create_dir_all(&lock_dir).map_err(|e| PulseError::io(&lock_dir, e))?;
        let path = lock_dir.join("workgraph.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| PulseError::io(&path, e))?;
        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file }),
                Err(e) if start.elapsed() < Duration::from_secs(10) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        return Err(PulseError::io(&path, e));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => {
                    return Err(PulseError::validation(
                        "lock_timeout",
                        "timed out acquiring workgraph lock",
                    ));
                }
            }
        }
    }
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> PulseResult<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation("invalid_path", format!("path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|e| PulseError::io(parent, e))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("pulse"),
        Uuid::new_v4()
    ));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)
            .map_err(|e| PulseError::io(&tmp, e))?;
        file.write_all(bytes).map_err(|e| PulseError::io(&tmp, e))?;
        file.sync_all().map_err(|e| PulseError::io(&tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| PulseError::io(path, e))?;
    sync_dir(parent)?;
    Ok(())
}

pub fn create_new(path: &Path, bytes: &[u8]) -> PulseResult<()> {
    let parent = path.parent().ok_or_else(|| {
        PulseError::validation("invalid_path", format!("path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|e| PulseError::io(parent, e))?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| PulseError::io(path, e))?;
    file.write_all(bytes).map_err(|e| PulseError::io(path, e))?;
    file.sync_all().map_err(|e| PulseError::io(path, e))?;
    sync_dir(parent)?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> PulseResult<T> {
    let bytes = fs::read(path).map_err(|e| PulseError::io(path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| PulseError::json(path, e))
}

pub fn safe_repo_relative(path: &str) -> PulseResult<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(PulseError::validation(
            "unsafe_path",
            format!("absolute paths are not allowed: {path}"),
        ));
    }
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            _ => {
                return Err(PulseError::validation(
                    "unsafe_path",
                    format!("path traversal is not allowed: {path}"),
                ));
            }
        }
    }
    Ok(out)
}

fn sync_dir(path: &Path) -> PulseResult<()> {
    #[cfg(unix)]
    {
        let dir = File::open(path).map_err(|e| PulseError::io(path, e))?;
        dir.sync_all().map_err(|e| PulseError::io(path, e))?;
    }
    Ok(())
}
