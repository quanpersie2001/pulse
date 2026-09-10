//! Append-with-fsync for line-oriented logs (Decision 0011).
//!
//! State touched: one file per call, opened for read and write. Every other
//! store in Pulse writes through the atomic temp-and-rename path in
//! [`super::atomic`]; the event log cannot, because rewriting a whole day of
//! events to add one line is what Decision 0011 set out to stop.
//!
//! Invariant: after a successful return the file ends in exactly one complete
//! `\n`-terminated record and its bytes are on disk. A crash mid-append can
//! leave one torn trailing line, so the next append truncates that line before
//! writing. A torn line therefore never sits between two complete records, and
//! a record that returned Ok is never lost.
//!
//! Callers must already hold the repository write lock: the truncate-then-
//! append sequence is not atomic against a concurrent writer.

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::error::{PulseError, Result};

/// Bytes read per step when scanning backwards for the last record boundary.
const SCAN_CHUNK: u64 = 8 * 1024;

/// Append one `\n`-terminated line to `path`, creating the file and its parent
/// when absent.
///
/// A torn trailing line left by an earlier crash is truncated first, so the
/// appended record is always preceded by complete ones. The file is fsynced
/// before returning.
///
/// `line` must not contain `\n`; a caller that splits records by newline would
/// otherwise read one record as several.
///
/// # Errors
/// Returns a validation error when `line` contains a newline, and an I/O error
/// when the parent cannot be created or the open, truncate, write or fsync
/// fails.
pub fn append_line_fsync(path: &Path, line: &[u8]) -> Result<()> {
    if line.contains(&b'\n') {
        return Err(PulseError::validation(
            "append_line_embedded_newline",
            format!(
                "refusing to append a record containing a newline to {}",
                path.display()
            ),
        ));
    }

    let parent = path.parent().ok_or_else(|| {
        PulseError::validation(
            "invalid_path",
            format!("target has no parent directory: {}", path.display()),
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|error| PulseError::io(parent, error))?;
    let existed = path.exists();

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| PulseError::io(path, error))?;

    let len = file
        .metadata()
        .map_err(|error| PulseError::io(path, error))?
        .len();
    if let Some(offset) = torn_tail_offset(&mut file, len, path)? {
        file.set_len(offset)
            .map_err(|error| PulseError::io(path, error))?;
    }

    file.seek(SeekFrom::End(0))
        .map_err(|error| PulseError::io(path, error))?;
    let mut record = Vec::with_capacity(line.len() + 1);
    record.extend_from_slice(line);
    record.push(b'\n');
    file.write_all(&record)
        .map_err(|error| PulseError::io(path, error))?;
    file.flush().map_err(|error| PulseError::io(path, error))?;
    file.sync_all()
        .map_err(|error| PulseError::io(path, error))?;
    drop(file);

    if !existed {
        // A fresh file needs its directory entry on disk too, or the record
        // survives the crash while the name pointing at it does not.
        super::atomic::fsync_dir(parent);
    }
    Ok(())
}

/// Byte offset the file must be truncated to so it ends on a record boundary,
/// or `None` when it already does.
///
/// Returns `Some(0)` for a non-empty file with no newline at all: the whole
/// content is one torn line.
fn torn_tail_offset(file: &mut std::fs::File, len: u64, path: &Path) -> Result<Option<u64>> {
    if len == 0 {
        return Ok(None);
    }
    let mut last = [0_u8; 1];
    file.seek(SeekFrom::Start(len - 1))
        .map_err(|error| PulseError::io(path, error))?;
    file.read_exact(&mut last)
        .map_err(|error| PulseError::io(path, error))?;
    if last[0] == b'\n' {
        return Ok(None);
    }

    let mut end = len;
    while end > 0 {
        let start = end.saturating_sub(SCAN_CHUNK);
        let mut buffer = vec![0_u8; usize::try_from(end - start).unwrap_or(0)];
        file.seek(SeekFrom::Start(start))
            .map_err(|error| PulseError::io(path, error))?;
        file.read_exact(&mut buffer)
            .map_err(|error| PulseError::io(path, error))?;
        if let Some(position) = buffer.iter().rposition(|byte| *byte == b'\n') {
            return Ok(Some(start + position as u64 + 1));
        }
        end = start;
    }
    Ok(Some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> Result<tempfile::TempDir> {
        tempfile::tempdir().map_err(|error| PulseError::io("<tempdir>", error))
    }

    #[test]
    fn appends_terminated_lines_in_order() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("day.jsonl");
        append_line_fsync(&path, b"first")?;
        append_line_fsync(&path, b"second")?;
        let bytes = std::fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        assert_eq!(bytes, b"first\nsecond\n");
        Ok(())
    }

    #[test]
    fn truncates_a_torn_trailing_line_before_appending() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("day.jsonl");
        std::fs::write(&path, b"complete\ntor").map_err(|error| PulseError::io(&path, error))?;
        append_line_fsync(&path, b"next")?;
        let bytes = std::fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        assert_eq!(bytes, b"complete\nnext\n");
        Ok(())
    }

    #[test]
    fn a_file_that_is_one_torn_line_is_replaced_entirely() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("day.jsonl");
        std::fs::write(&path, b"torn").map_err(|error| PulseError::io(&path, error))?;
        append_line_fsync(&path, b"next")?;
        let bytes = std::fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        assert_eq!(bytes, b"next\n");
        Ok(())
    }

    #[test]
    fn torn_tail_longer_than_one_scan_chunk_is_truncated() -> Result<()> {
        let tmp = tempdir()?;
        let path = tmp.path().join("day.jsonl");
        let mut content = b"complete\n".to_vec();
        content.extend_from_slice(&vec![b'x'; (SCAN_CHUNK * 2 + 7) as usize]);
        std::fs::write(&path, &content).map_err(|error| PulseError::io(&path, error))?;
        append_line_fsync(&path, b"next")?;
        let bytes = std::fs::read(&path).map_err(|error| PulseError::io(&path, error))?;
        assert_eq!(bytes, b"complete\nnext\n");
        Ok(())
    }

    #[test]
    fn refuses_a_record_containing_a_newline() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("day.jsonl");
        let error = append_line_fsync(&path, b"one\ntwo").expect_err("newline must be refused");
        assert!(error.to_string().contains("newline"), "{error}");
        assert!(!path.exists());
    }
}
