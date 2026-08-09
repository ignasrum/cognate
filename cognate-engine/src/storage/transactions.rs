use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STAGED_DELETE_PREFIX: &str = ".cognate_txn_delete_";
const STAGED_DELETE_CLEANUP_GRACE_NANOS: u128 = 5 * 60 * 1_000_000_000;

pub(crate) fn build_staging_path(notebook_path: &Path, rel_path: &str, operation: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sanitized_rel_path = Path::new(rel_path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(component) => Some(component.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<String>>()
        .join("__");

    notebook_path.join(format!(
        ".cognate_txn_{}_{}_{}",
        operation, sanitized_rel_path, timestamp
    ))
}

pub(crate) async fn cleanup_stale_staged_delete_entries(notebook_path: &Path) {
    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut entries = match tokio::fs::read_dir(notebook_path).await {
        Ok(dir) => dir,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with(STAGED_DELETE_PREFIX) {
            continue;
        }
        let timestamp_nanos = match file_name.rsplit('_').next() {
            Some(ts) => match ts.parse::<u128>() {
                Ok(parsed) => parsed,
                Err(_) => continue,
            },
            None => continue,
        };
        if now_nanos.saturating_sub(timestamp_nanos) < STAGED_DELETE_CLEANUP_GRACE_NANOS {
            continue;
        }

        // A stale staged delete may be the only surviving copy after a failed
        // rollback. Quarantine it instead of deleting potentially authoritative
        // note or attachment data.
        let recovery_name = format!(".cognate_recovery_{}", file_name);
        let recovery_path = notebook_path.join(recovery_name);
        let _ = tokio::fs::rename(entry.path(), recovery_path).await;
    }
}
