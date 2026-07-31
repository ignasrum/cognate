use crate::EngineError;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub fn validate_relative_path(
    path_kind: &'static str,
    value: &str,
) -> Result<PathBuf, EngineError> {
    if value.trim().is_empty() {
        return Err(EngineError::validation(
            "path validation",
            format!("Invalid {} '{}': path cannot be empty.", path_kind, value),
        ));
    }

    let mut has_normal_component = false;
    for component in Path::new(value).components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': '.' path components are not allowed.",
                        path_kind, value
                    ),
                ));
            }
            Component::ParentDir => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': '..' path components are not allowed.",
                        path_kind, value
                    ),
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(EngineError::validation(
                    "path validation",
                    format!(
                        "Invalid {} '{}': absolute paths are not allowed.",
                        path_kind, value
                    ),
                ));
            }
        }
    }

    if !has_normal_component {
        return Err(EngineError::validation(
            "path validation",
            format!(
                "Invalid {} '{}': path must contain at least one normal component.",
                path_kind, value
            ),
        ));
    }

    Ok(PathBuf::from(value))
}

pub async fn ensure_path_within_notebook_if_canonicalizable(
    notebook_path: &Path,
    target_path: &Path,
    rel_path: &str,
    outside_error_prefix: &str,
) -> Result<(), EngineError> {
    let canonical_notebook_path =
        tokio::fs::canonicalize(notebook_path)
            .await
            .map_err(|error| {
                EngineError::storage(
                    "path containment",
                    format!(
                        "Failed to resolve notebook root '{}': {error}",
                        notebook_path.display()
                    ),
                )
            })?;

    let mut existing_path = target_path.to_path_buf();
    let canonical_target_path = loop {
        match tokio::fs::canonicalize(&existing_path).await {
            Ok(path) => break path,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let Some(parent) = existing_path.parent() else {
                    return Err(EngineError::storage(
                        "path containment",
                        format!(
                            "Failed to resolve path '{}': no existing parent",
                            target_path.display()
                        ),
                    ));
                };
                if parent == existing_path {
                    return Err(EngineError::storage(
                        "path containment",
                        format!(
                            "Failed to resolve path '{}': reached filesystem root",
                            target_path.display()
                        ),
                    ));
                }
                existing_path = parent.to_path_buf();
            }
            Err(error) => {
                return Err(EngineError::storage(
                    "path containment",
                    format!(
                        "Failed to resolve path '{}': {error}",
                        target_path.display()
                    ),
                ));
            }
        }
    };

    if !canonical_target_path.starts_with(&canonical_notebook_path) {
        return Err(EngineError::validation(
            "path containment",
            format!("{} '{}'", outside_error_prefix, rel_path),
        ));
    }

    reject_symlink_components(notebook_path, target_path, rel_path).await?;
    Ok(())
}

async fn reject_symlink_components(
    notebook_path: &Path,
    target_path: &Path,
    rel_path: &str,
) -> Result<(), EngineError> {
    let relative = target_path.strip_prefix(notebook_path).map_err(|_| {
        EngineError::validation(
            "path containment",
            format!("Path is not relative to notebook: '{rel_path}'"),
        )
    })?;
    let mut current = notebook_path.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(EngineError::validation(
                    "path containment",
                    format!("Symlink path components are not allowed: '{rel_path}'"),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => break,
            Err(error) => {
                return Err(EngineError::storage(
                    "path containment",
                    format!("Failed to inspect path '{}': {error}", current.display()),
                ));
            }
        }
    }
    Ok(())
}

pub fn build_atomic_temp_path(target_path: &Path) -> Result<PathBuf, std::io::Error> {
    let parent = target_path.parent().ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "Cannot atomically write '{}': target has no parent directory.",
                target_path.display()
            ),
        )
    })?;

    let target_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cognate_tmp");
    let timestamp_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    Ok(parent.join(format!(
        ".{}.cognate_tmp_{}_{}",
        target_name,
        std::process::id(),
        timestamp_nanos
    )))
}

pub async fn atomic_rename(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = to.parent()
        && tokio::fs::try_exists(&parent.join(".cognate_fail_atomic_rename"))
            .await
            .unwrap_or(false)
        && to.file_name().and_then(|name| name.to_str()) != Some("metadata.json.bak")
    {
        return Err(std::io::Error::other(format!(
            "Simulated atomic rename failure for '{}'",
            to.display()
        )));
    }

    tokio::fs::rename(from, to).await
}

pub async fn atomic_write_string(target_path: &Path, content: &str) -> Result<(), std::io::Error> {
    let temp_path = build_atomic_temp_path(target_path)?;
    let mut file = tokio::fs::File::create(&temp_path).await?;
    file.write_all(content.as_bytes()).await?;
    file.sync_all().await?;
    restrict_temp_permissions(&temp_path).await?;

    if let Err(rename_error) = atomic_rename(&temp_path, target_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(rename_error);
    }

    sync_parent_directory(target_path).await?;
    Ok(())
}

pub async fn write_text_file_atomically(
    target_path: &Path,
    content: &str,
) -> Result<(), EngineError> {
    if let Some(parent) = target_path.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(EngineError::storage(
            "atomic write",
            format!(
                "Failed to create parent directory for '{}': {}",
                target_path.display(),
                error
            ),
        ));
    }

    atomic_write_string(target_path, content)
        .await
        .map_err(|error| {
            EngineError::storage(
                "atomic write",
                format!(
                    "Failed to atomically write '{}': {}",
                    target_path.display(),
                    error
                ),
            )
        })
}

pub async fn atomic_write_bytes(target_path: &Path, content: &[u8]) -> Result<(), std::io::Error> {
    let temp_path = build_atomic_temp_path(target_path)?;
    let mut file = tokio::fs::File::create(&temp_path).await?;
    file.write_all(content).await?;
    file.sync_all().await?;
    restrict_temp_permissions(&temp_path).await?;

    if let Err(rename_error) = atomic_rename(&temp_path, target_path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(rename_error);
    }

    sync_parent_directory(target_path).await?;
    Ok(())
}

async fn restrict_temp_permissions(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    Ok(())
}

async fn sync_parent_directory(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let parent = parent.to_path_buf();
        tokio::task::spawn_blocking(move || std::fs::File::open(parent)?.sync_all())
            .await
            .map_err(|error| {
                std::io::Error::other(format!("directory sync task failed: {error}"))
            })??;
    }
    Ok(())
}

pub async fn write_bytes_file_atomically(
    target_path: &Path,
    content: &[u8],
) -> Result<(), EngineError> {
    if let Some(parent) = target_path.parent()
        && let Err(error) = tokio::fs::create_dir_all(parent).await
    {
        return Err(EngineError::storage(
            "atomic write",
            format!(
                "Failed to create parent directory for '{}': {}",
                target_path.display(),
                error
            ),
        ));
    }

    atomic_write_bytes(target_path, content)
        .await
        .map_err(|error| {
            EngineError::storage(
                "atomic write",
                format!(
                    "Failed to atomically write '{}': {}",
                    target_path.display(),
                    error
                ),
            )
        })
}

pub async fn remove_empty_parent_directories(notebook_path: &Path, deleted_note_dir_path: &Path) {
    let mut current_parent = deleted_note_dir_path.parent().map(Path::to_path_buf);

    while let Some(parent_path) = current_parent {
        if let Ok(canonical_notebook) = tokio::fs::canonicalize(notebook_path).await
            && let Ok(canonical_parent) = tokio::fs::canonicalize(&parent_path).await
            && canonical_parent != canonical_notebook
            && canonical_parent.starts_with(&canonical_notebook)
        {
            match tokio::fs::remove_dir(&parent_path).await {
                Ok(_) => {
                    current_parent = parent_path.parent().map(Path::to_path_buf);
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    current_parent = parent_path.parent().map(Path::to_path_buf);
                }
                Err(_) => {
                    break;
                }
            }
        } else {
            break;
        }
    }
}

pub async fn rollback_rename(
    from: &Path,
    to: &Path,
    notebook_path: &Path,
    simulated_failure_marker: &str,
) -> Result<(), EngineError> {
    if tokio::fs::try_exists(&notebook_path.join(simulated_failure_marker))
        .await
        .unwrap_or(false)
    {
        return Err(EngineError::recovery(
            "rollback rename",
            "Simulated recovery failure",
        ));
    }

    tokio::fs::rename(from, to).await.map_err(|err| {
        EngineError::recovery(
            "rollback rename",
            format!(
                "Failed to restore original folder '{}' from '{}': {}",
                to.display(),
                from.display(),
                err
            ),
        )
    })
}
