use crate::error::{PulseError, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|error| PulseError::io(path, error))?;
    if !canonical.is_dir() {
        return Err(PulseError::Validation {
            message: format!("path is not a directory: {}", canonical.display()),
        });
    }
    Ok(canonical)
}

pub fn resolve_repo_relative(repo_root: &Path, relative_path: impl AsRef<Path>) -> Result<PathBuf> {
    let repo_root = canonicalize_existing_dir(repo_root)?;
    let relative_path = relative_path.as_ref();
    validate_relative_path(relative_path)?;

    let mut current = repo_root.clone();
    let components: Vec<_> = relative_path.components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(part) = component else {
            return Err(PulseError::PathTraversal {
                path: relative_path.to_path_buf(),
            });
        };
        current.push(part);

        if current.exists() {
            let canonical =
                fs::canonicalize(&current).map_err(|error| PulseError::io(&current, error))?;
            ensure_under_root(&repo_root, &canonical, relative_path)?;
            current = canonical;
        } else if let Some(parent) = current.parent() {
            let canonical_parent =
                fs::canonicalize(parent).map_err(|error| PulseError::io(parent, error))?;
            ensure_under_root(&repo_root, &canonical_parent, relative_path)?;
            let remaining = components[index + 1..]
                .iter()
                .map(component_to_path)
                .collect::<Result<Vec<_>>>()?;
            let mut resolved = canonical_parent.join(part);
            for item in remaining {
                resolved.push(item);
            }
            ensure_under_root(&repo_root, &resolved, relative_path)?;
            return Ok(resolved);
        }
    }

    ensure_under_root(&repo_root, &current, relative_path)?;
    Ok(current)
}

pub fn resolve_content_path(repo_root: &Path, content_path: impl AsRef<Path>) -> Result<PathBuf> {
    let content_path = content_path.as_ref();
    let resolved = resolve_repo_relative(repo_root, content_path)?;
    let works_root = match resolve_repo_relative(repo_root, "works") {
        Ok(path) => path,
        Err(PulseError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            let repo_root = canonicalize_existing_dir(repo_root)?;
            repo_root.join("works")
        }
        Err(error) => return Err(error),
    };
    if !resolved.starts_with(&works_root) {
        return Err(PulseError::ContentRootViolation {
            path: content_path.to_path_buf(),
        });
    }
    Ok(resolved)
}

pub fn validate_relative_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        return Err(PulseError::AbsolutePath {
            path: path.to_path_buf(),
        });
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir | Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PulseError::PathTraversal {
                    path: path.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

fn ensure_under_root(repo_root: &Path, candidate: &Path, original: &Path) -> Result<()> {
    if candidate.starts_with(repo_root) {
        Ok(())
    } else {
        Err(PulseError::PathEscape {
            path: original.to_path_buf(),
        })
    }
}

fn component_to_path(component: &Component<'_>) -> Result<PathBuf> {
    match component {
        Component::Normal(part) => Ok(PathBuf::from(part)),
        _ => Err(PulseError::PathTraversal {
            path: PathBuf::from(component.as_os_str()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let error = resolve_repo_relative(tmp.path(), "../escape").unwrap_err();
        assert!(matches!(error, PulseError::PathTraversal { .. }));
    }

    #[test]
    fn rejects_content_outside_works() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("works")).unwrap();
        let error = resolve_content_path(tmp.path(), ".pulse/workgraph").unwrap_err();
        assert!(matches!(error, PulseError::ContentRootViolation { .. }));
    }
}
