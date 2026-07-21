use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::fs_utils::validate_relative_path;
use crate::EngineError;

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Owns the cross-process locks for one notebook.
#[derive(Debug, Clone)]
pub struct ConcurrencyManager {
    notebook_path: PathBuf,
    lock_dir: PathBuf,
}

/// A held advisory OS file lock. The lock is released when this guard is dropped.
#[derive(Debug)]
pub struct FileLockGuard {
    file: File,
}

fn try_lock(file: &File) -> Result<(), std::io::Error> {
    match file.try_lock() {
        Ok(()) => Ok(()),
        Err(std::fs::TryLockError::WouldBlock) => Err(std::io::Error::new(
            std::io::ErrorKind::WouldBlock,
            "lock is held by another process",
        )),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

fn unlock(file: &File) {
    let _ = file.unlock();
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

impl ConcurrencyManager {
    pub fn new(notebook_path: &Path) -> Self {
        Self {
            notebook_path: notebook_path.to_path_buf(),
            lock_dir: notebook_path.join(".cognate_locks"),
        }
    }

    pub async fn acquire_notebook(&self) -> Result<FileLockGuard, EngineError> {
        self.acquire("notebook", self.lock_dir.join("notebook.lock"))
            .await
    }

    pub async fn acquire_note(&self, rel_path: &str) -> Result<FileLockGuard, EngineError> {
        let normalized = validate_relative_path("note path", rel_path)?;
        let resource = normalized.to_string_lossy().replace(['/', '\\'], "__");
        self.acquire(
            &format!("note '{rel_path}'"),
            self.lock_dir.join(format!("note_{resource}.lock")),
        )
        .await
    }

    async fn acquire(
        &self,
        resource: &str,
        lock_path: PathBuf,
    ) -> Result<FileLockGuard, EngineError> {
        let lock_dir = self.lock_dir.clone();
        let notebook_path = self.notebook_path.display().to_string();
        let resource = resource.to_string();

        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&lock_dir).map_err(|error| {
                EngineError::storage(
                    "create lock directory",
                    format!(
                        "Failed to create lock directory for '{}': {error}",
                        notebook_path
                    ),
                )
            })?;

            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&lock_path)
                .map_err(|error| {
                    EngineError::storage(
                        "open lock file",
                        format!("Failed to open '{}': {error}", lock_path.display()),
                    )
                })?;

            let deadline = Instant::now() + LOCK_TIMEOUT;
            loop {
                match try_lock(&file) {
                    Ok(()) => return Ok(FileLockGuard { file }),
                    Err(error) if Instant::now() < deadline => {
                        if error.kind() == std::io::ErrorKind::WouldBlock {
                            std::thread::sleep(RETRY_INTERVAL);
                        } else {
                            return Err(EngineError::storage(
                                "acquire file lock",
                                format!("Failed to lock '{}': {error}", lock_path.display()),
                            ));
                        }
                    }
                    Err(error) => {
                        return Err(EngineError::lock_unavailable(
                            "acquire file lock",
                            resource,
                            format!("Timed out after {}ms: {error}", LOCK_TIMEOUT.as_millis()),
                        ));
                    }
                }
            }
        })
        .await
        .map_err(|error| EngineError::storage("acquire file lock", error.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_notebook() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cognate-lock-test-{suffix}"))
    }

    #[tokio::test]
    async fn same_note_lock_times_out_while_held() {
        let root = temp_notebook();
        let first = ConcurrencyManager::new(&root);
        let second = ConcurrencyManager::new(&root);
        let _guard = first.acquire_note("folder/note").await.unwrap();

        let result = second.acquire_note("folder/note").await;
        assert!(matches!(result, Err(EngineError::LockUnavailable { .. })));
    }

    #[tokio::test]
    async fn different_note_locks_can_be_held_together() {
        let root = temp_notebook();
        let manager = ConcurrencyManager::new(&root);
        let _first = manager.acquire_note("one").await.unwrap();
        let _second = manager.acquire_note("two").await.unwrap();
    }

    #[tokio::test]
    async fn dropping_a_guard_releases_the_lock_for_another_client() {
        let root = temp_notebook();
        let first = ConcurrencyManager::new(&root);
        let second = ConcurrencyManager::new(&root);

        {
            let _guard = first.acquire_notebook().await.unwrap();
        }

        let guard = second.acquire_notebook().await;
        assert!(
            guard.is_ok(),
            "lock should be released when its guard is dropped"
        );
    }

    #[tokio::test]
    async fn note_and_notebook_locks_use_distinct_resources() {
        let root = temp_notebook();
        let manager = ConcurrencyManager::new(&root);
        let _notebook = manager.acquire_notebook().await.unwrap();
        let _note = manager.acquire_note("folder/note").await.unwrap();

        assert!(root.join(".cognate_locks/notebook.lock").exists());
        assert!(root.join(".cognate_locks/note_folder__note.lock").exists());
    }

    #[tokio::test]
    async fn invalid_note_paths_are_rejected_before_lock_files_are_created() {
        let root = temp_notebook();
        let manager = ConcurrencyManager::new(&root);

        for path in ["", ".", "../outside", "/absolute"] {
            let result = manager.acquire_note(path).await;
            assert!(matches!(result, Err(EngineError::Validation { .. })));
        }

        assert!(!root.join(".cognate_locks").exists());
    }
}
