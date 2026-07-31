#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cognate_engine::storage::{NoteMetadata, NotebookManager};

pub struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    pub fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cognate_engine_test_{}_{}", name, unique));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub struct NotebookTestHarness {
    temp: TempTestDir,
}

impl NotebookTestHarness {
    pub fn new(name: &str) -> Self {
        Self {
            temp: TempTestDir::new(name),
        }
    }

    pub fn path(&self) -> &Path {
        self.temp.path()
    }

    pub fn manager(&self) -> NotebookManager {
        NotebookManager::new(self.path())
    }

    pub async fn write_metadata(&self, notes: &[NoteMetadata]) {
        self.manager()
            .save_metadata(notes)
            .await
            .expect("Failed to write metadata");
    }

    pub fn inject_rename_fault(&self) {
        std::fs::write(self.path().join(".cognate_fail_atomic_rename"), "fail")
            .expect("Failed to create atomic-rename failure marker");
    }

    pub fn inject_delete_rollback_fault(&self) {
        std::fs::write(self.path().join(".cognate_fail_delete_rollback"), "fail")
            .expect("Failed to create delete-rollback failure marker");
    }

    pub fn inject_move_rollback_fault(&self) {
        std::fs::write(self.path().join(".cognate_fail_move_rollback"), "fail")
            .expect("Failed to create move-rollback failure marker");
    }

    pub fn inject_metadata_trap_directory(&self) {
        std::fs::create_dir(self.path().join("metadata.json"))
            .expect("Failed to create metadata trap directory");
    }
}

pub fn assert_note_md_exists(notebook_path: &Path, rel_path: &str) {
    let note_path = notebook_path.join(rel_path).join("note.md");
    assert!(
        note_path.exists(),
        "Expected note file to exist at '{}'",
        note_path.display()
    );
}

pub fn assert_note_md_not_exists(notebook_path: &Path, rel_path: &str) {
    let note_path = notebook_path.join(rel_path).join("note.md");
    assert!(
        !note_path.exists(),
        "Expected note file to be absent at '{}'",
        note_path.display()
    );
}

pub fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock error")
        .as_nanos()
}

pub async fn load_notes_or_panic(manager: &NotebookManager) -> Vec<NoteMetadata> {
    manager
        .load_metadata()
        .await
        .expect("Expected metadata load to succeed")
        .notes
}
