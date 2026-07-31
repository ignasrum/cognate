use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum QueueStatus {
    #[default]
    Pending,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct QueuedNoteWrite {
    pub rel_path: String,
    pub content: String,
    pub expected_revision: Option<String>,
    #[serde(default)]
    pub status: QueueStatus,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default)]
    pub next_retry_at: Option<u64>,
}

pub(crate) fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(crate) fn retry_delay_seconds(retry_count: u32) -> u64 {
    2_u64.saturating_pow(retry_count.min(9)).min(300)
}

pub(crate) fn queue_path(config_path: &str) -> PathBuf {
    Path::new(config_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".cognate-api-queue.json")
}

pub(crate) fn enqueue(path: &Path, entry: QueuedNoteWrite) -> Result<(), String> {
    let mut entries = read(path)?;
    if let Some(existing) = entries
        .iter_mut()
        .find(|item| item.rel_path == entry.rel_path)
    {
        *existing = entry;
    } else {
        entries.push(entry);
    }
    write(path, &entries)
}

pub(crate) fn read(path: &Path) -> Result<Vec<QueuedNoteWrite>, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|error| format!("failed to parse offline queue: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("failed to read offline queue: {error}")),
    }
}

pub(crate) fn write(path: &Path, entries: &[QueuedNoteWrite]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create queue directory: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("failed to serialize offline queue: {error}"))?;
    std::fs::write(&temporary, format!("{contents}\n"))
        .map_err(|error| format!("failed to write offline queue: {error}"))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| format!("failed to commit offline queue: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to restrict offline queue permissions: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_replaces_pending_write_for_the_same_note() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("queue.json");
        enqueue(
            &path,
            QueuedNoteWrite {
                rel_path: "note".into(),
                content: "first".into(),
                expected_revision: None,
                status: QueueStatus::Pending,
                retry_count: 0,
                next_retry_at: None,
            },
        )
        .unwrap();
        enqueue(
            &path,
            QueuedNoteWrite {
                rel_path: "note".into(),
                content: "latest".into(),
                expected_revision: Some("rev".into()),
                status: QueueStatus::Pending,
                retry_count: 0,
                next_retry_at: None,
            },
        )
        .unwrap();
        assert_eq!(read(&path).unwrap().len(), 1);
        assert_eq!(read(&path).unwrap()[0].content, "latest");
    }

    #[test]
    fn retry_backoff_is_bounded_and_queue_status_round_trips() {
        assert_eq!(retry_delay_seconds(0), 1);
        assert_eq!(retry_delay_seconds(3), 8);
        assert_eq!(retry_delay_seconds(99), 300);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("queue.json");
        write(
            &path,
            &[QueuedNoteWrite {
                rel_path: "note".into(),
                content: "pending".into(),
                expected_revision: Some("revision".into()),
                status: QueueStatus::Conflict,
                retry_count: 4,
                next_retry_at: None,
            }],
        )
        .unwrap();
        let entry = read(&path).unwrap().remove(0);
        assert_eq!(entry.status, QueueStatus::Conflict);
        assert_eq!(entry.retry_count, 4);
    }
}
