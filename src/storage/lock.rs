use crate::error::{PulseError, Result};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_DELAY: Duration = Duration::from_millis(25);

#[derive(Debug)]
pub struct WriteGuard {
    lock_path: PathBuf,
    file: File,
}

impl WriteGuard {
    pub fn acquire(repo_root: &Path) -> Result<Self> {
        Self::acquire_with_timeout(repo_root, DEFAULT_TIMEOUT)
    }

    pub fn acquire_with_timeout(repo_root: &Path, timeout: Duration) -> Result<Self> {
        let lock_dir = repo_root.join(".pulse/runtime/locks");
        fs::create_dir_all(&lock_dir).map_err(|error| PulseError::io(&lock_dir, error))?;
        let lock_path = lock_dir.join("workgraph.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| PulseError::io(&lock_path, error))?;

        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => {
                    return Ok(Self { lock_path, file });
                }
                Err(error) if start.elapsed() >= timeout => {
                    if is_would_block(&error) {
                        return Err(PulseError::LockTimeout { lock_path, timeout });
                    }
                    return Err(PulseError::io(&lock_path, error));
                }
                Err(error) if is_would_block(&error) => thread::sleep(RETRY_DELAY),
                Err(error) => return Err(PulseError::io(&lock_path, error)),
            }
        }
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

fn is_would_block(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}
