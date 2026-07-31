//! Durable API write queue used when the remote backend is unavailable.
//!
//! Queue writes are coalesced per note and committed through a temporary file
//! rename. Conflict entries are retained and paused until the UI resolves
//! them; transport failures remain retryable with bounded exponential backoff.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) struct QueueLock {
    file: File,
}

impl Drop for QueueLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) enum QueueStatus {
    #[default]
    Pending,
    Conflict,
    /// The server acknowledged the write; local removal is still pending.
    RemoteCommitConfirmed,
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
    let lock = lock(path)?;
    enqueue_locked(&lock, path, entry)
}

fn enqueue_locked(_lock: &QueueLock, path: &Path, entry: QueuedNoteWrite) -> Result<(), String> {
    let mut entries = read_unlocked(path)?;
    if let Some(existing) = entries
        .iter_mut()
        .find(|item| item.rel_path == entry.rel_path)
    {
        *existing = entry;
    } else {
        entries.push(entry);
    }
    write_unlocked(path, &entries)
}

pub(crate) fn read(path: &Path) -> Result<Vec<QueuedNoteWrite>, String> {
    let _lock = lock(path)?;
    read_unlocked(path)
}

pub(crate) fn read_locked(path: &Path) -> Result<Vec<QueuedNoteWrite>, String> {
    read_unlocked(path)
}

fn read_unlocked(path: &Path) -> Result<Vec<QueuedNoteWrite>, String> {
    recover_orphaned_queue(path)?;
    match std::fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|error| format!("failed to parse offline queue: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("failed to read offline queue: {error}")),
    }
}

#[cfg(test)]
pub(crate) fn write(path: &Path, entries: &[QueuedNoteWrite]) -> Result<(), String> {
    let _lock = lock(path)?;
    write_unlocked(path, entries)
}

pub(crate) fn write_locked(
    _lock: &QueueLock,
    path: &Path,
    entries: &[QueuedNoteWrite],
) -> Result<(), String> {
    write_unlocked(path, entries)
}

fn write_unlocked(path: &Path, entries: &[QueuedNoteWrite]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create queue directory: {error}"))?;
    let temporary = path.with_extension(format!(
        "json.tmp-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let contents = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("failed to serialize offline queue: {error}"))?;
    let mut file = File::create(&temporary)
        .map_err(|error| format!("failed to create offline queue temporary file: {error}"))?;
    file.write_all(format!("{contents}\n").as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to flush offline queue: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to restrict offline queue permissions: {error}"))?;
    }
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

fn recover_orphaned_queue(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = format!(
        "{}.json.tmp-",
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("queue")
    );
    let mut candidates = std::fs::read_dir(parent)
        .map_err(|error| format!("failed to inspect offline queue directory: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });

    for candidate in candidates {
        let candidate_path = candidate.path();
        let contents = match std::fs::read_to_string(&candidate_path) {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        if serde_json::from_str::<Vec<QueuedNoteWrite>>(&contents).is_err() {
            continue;
        }
        if std::fs::rename(&candidate_path, path).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(
                    |error| {
                        format!("failed to restrict recovered offline queue permissions: {error}")
                    },
                )?;
            }
            break;
        }
    }
    Ok(())
}

pub(crate) fn lock(path: &Path) -> Result<QueueLock, String> {
    let lock_path = path.with_extension("lock");
    let parent = lock_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create queue lock directory: {error}"))?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| format!("failed to open queue lock: {error}"))?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => {
                return Ok(QueueLock { file });
            }
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("failed to lock offline queue: {error}")),
        }
    }
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

    #[test]
    fn concurrent_enqueues_for_different_notes_are_not_lost() {
        let directory = tempfile::tempdir().unwrap();
        let path = std::sync::Arc::new(directory.path().join("queue.json"));
        let mut workers = Vec::new();
        for index in 0..8 {
            let path = path.clone();
            workers.push(std::thread::spawn(move || {
                enqueue(
                    &path,
                    QueuedNoteWrite {
                        rel_path: format!("note-{index}"),
                        content: format!("content-{index}"),
                        expected_revision: None,
                        status: QueueStatus::Pending,
                        retry_count: 0,
                        next_retry_at: None,
                    },
                )
                .unwrap();
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }

        let entries = read(&path).unwrap();
        assert_eq!(entries.len(), 8);
    }
}
