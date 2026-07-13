use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cognate_engine::EngineError;
use cognate_engine::storage::{NotebookManager, NoteMetadata, AttachmentManager};
use cognate_engine::search::SearchIndexManager;

struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("cognate_engine_test_{}_{}", name, unique));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct NotebookTestHarness {
    temp: TempTestDir,
}

impl NotebookTestHarness {
    fn new(name: &str) -> Self {
        Self {
            temp: TempTestDir::new(name),
        }
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn manager(&self) -> NotebookManager {
        NotebookManager::new(self.path())
    }

    async fn write_metadata(&self, notes: &[NoteMetadata]) {
        self.manager()
            .save_metadata(notes)
            .await
            .expect("Failed to write metadata");
    }

    fn inject_rename_fault(&self) {
        std::fs::write(
            self.path().join(".cognate_fail_atomic_rename"),
            "fail",
        )
        .expect("Failed to create atomic-rename failure marker");
    }

    fn inject_delete_rollback_fault(&self) {
        std::fs::write(
            self.path().join(".cognate_fail_delete_rollback"),
            "fail",
        )
        .expect("Failed to create delete-rollback failure marker");
    }

    fn inject_move_rollback_fault(&self) {
        std::fs::write(
            self.path().join(".cognate_fail_move_rollback"),
            "fail",
        )
        .expect("Failed to create move-rollback failure marker");
    }

    fn inject_metadata_trap_directory(&self) {
        std::fs::create_dir(self.path().join("metadata.json"))
            .expect("Failed to create metadata trap directory");
    }
}

fn assert_note_md_exists(notebook_path: &Path, rel_path: &str) {
    let note_path = notebook_path.join(rel_path).join("note.md");
    assert!(
        note_path.exists(),
        "Expected note file to exist at '{}'",
        note_path.display()
    );
}

fn assert_note_md_not_exists(notebook_path: &Path, rel_path: &str) {
    let note_path = notebook_path.join(rel_path).join("note.md");
    assert!(
        !note_path.exists(),
        "Expected note file to be absent at '{}'",
        note_path.display()
    );
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock error")
        .as_nanos()
}

async fn load_notes_or_panic(manager: &NotebookManager) -> Vec<NoteMetadata> {
    manager
        .load_metadata()
        .await
        .expect("Expected metadata load to succeed")
        .notes
}

#[tokio::test]
async fn create_new_note_creates_file_and_metadata() {
    let harness = NotebookTestHarness::new("create_note");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let created = manager
        .create_note("work/todo", &mut notes)
        .await
        .expect("create_note should succeed");

    assert_eq!(created.rel_path, "work/todo");
    assert!(created.last_updated.is_some());
    assert!(
        !created.last_updated.as_deref().unwrap_or("").contains('.'),
        "last_updated should not include subsecond precision"
    );
    assert_eq!(notes.len(), 1);
    assert!(notes[0].last_updated.is_some());
    assert_note_md_exists(harness.path(), "work/todo");

    let loaded = load_notes_or_panic(&manager).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].rel_path, "work/todo");
    assert!(loaded[0].last_updated.is_some());
}

#[tokio::test]
async fn create_new_note_rejects_invalid_relative_path() {
    let harness = NotebookTestHarness::new("invalid_path");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let result = manager.create_note("../outside", &mut notes).await;

    assert!(result.is_err());
    assert!(notes.is_empty());
}

#[tokio::test]
async fn create_new_note_rejects_duplicate_metadata_path() {
    let harness = NotebookTestHarness::new("duplicate_note");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("dup/note", &mut notes)
        .await
        .expect("Initial note creation should succeed");

    let duplicate = manager.create_note("dup/note", &mut notes).await;

    assert!(duplicate.is_err());
    assert_eq!(notes.len(), 1);
}

#[tokio::test]
async fn create_new_note_allows_double_dot_within_component_names() {
    let harness = NotebookTestHarness::new("component_double_dot");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let created = manager
        .create_note("release..notes/v1", &mut notes)
        .await
        .expect("Component-based validation should allow '..' inside normal path components");

    assert_eq!(created.rel_path, "release..notes/v1");
    assert_note_md_exists(harness.path(), "release..notes/v1");
}

#[tokio::test]
async fn delete_note_removes_file_and_metadata() {
    let harness = NotebookTestHarness::new("delete_note");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager.create_note("alpha", &mut notes).await.expect("Failed to create alpha");
    manager.create_note("beta", &mut notes).await.expect("Failed to create beta");

    manager
        .delete_note("alpha", &mut notes)
        .await
        .expect("delete_note should succeed");

    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].rel_path, "beta");
    assert_note_md_not_exists(harness.path(), "alpha");
    assert_note_md_exists(harness.path(), "beta");

    let loaded = load_notes_or_panic(&manager).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].rel_path, "beta");
}

#[tokio::test]
async fn delete_note_removes_filesystem_item_even_if_missing_from_metadata() {
    let harness = NotebookTestHarness::new("delete_without_metadata");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let external_note_dir = harness.path().join("external/note");
    std::fs::create_dir_all(&external_note_dir).expect("Failed to create external note directory");
    std::fs::write(external_note_dir.join("note.md"), "externally created")
        .expect("Failed to create external note file");

    manager
        .delete_note("external/note", &mut notes)
        .await
        .expect("Deletion should succeed even when metadata entry is missing");

    assert_note_md_not_exists(harness.path(), "external/note");
    assert!(notes.is_empty());
}

#[tokio::test]
async fn delete_note_removes_empty_parent_folders_after_note_delete() {
    let harness = NotebookTestHarness::new("delete_removes_empty_parents");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("only_folder/only_note", &mut notes)
        .await
        .expect("Failed to create only_folder/only_note");

    manager
        .delete_note("only_folder/only_note", &mut notes)
        .await
        .expect("delete_note should succeed");

    assert!(notes.is_empty(), "Metadata entry should be removed");
    assert!(
        !harness.path().join("only_folder").exists(),
        "Parent folder should be removed when it becomes empty"
    );
}

#[tokio::test]
async fn delete_note_rejects_invalid_relative_path() {
    let harness = NotebookTestHarness::new("delete_invalid_path");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let result = manager.delete_note("../outside", &mut notes).await;

    assert!(result.is_err());
    let error = result.expect_err("expected error");
    assert!(
        matches!(error, EngineError::Validation { .. }),
        "Expected invalid-path validation error, got: {:?}",
        error
    );
}

#[tokio::test]
async fn move_note_moves_files_and_updates_metadata() {
    let harness = NotebookTestHarness::new("move_note");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("old/path", &mut notes)
        .await
        .expect("Failed to create note for move");

    let moved_to = manager
        .move_note("old/path", "new/path", &mut notes)
        .await
        .expect("move_note should succeed");

    assert_eq!(moved_to, "new/path");
    assert_note_md_not_exists(harness.path(), "old/path");
    assert_note_md_exists(harness.path(), "new/path");

    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].rel_path, "new/path");

    let loaded = load_notes_or_panic(&manager).await;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].rel_path, "new/path");
}

#[tokio::test]
async fn move_note_fails_when_target_exists() {
    let harness = NotebookTestHarness::new("move_target_exists");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager.create_note("source/note", &mut notes).await.unwrap();
    manager.create_note("target/note", &mut notes).await.unwrap();

    let result = manager.move_note("source/note", "target/note", &mut notes).await;

    assert!(result.is_err());
    assert_note_md_exists(harness.path(), "source/note");
    assert_note_md_exists(harness.path(), "target/note");
}

#[tokio::test]
async fn move_note_rejects_invalid_current_relative_path() {
    let harness = NotebookTestHarness::new("move_invalid_current_path");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let result = manager.move_note("../outside", "target/note", &mut notes).await;

    assert!(result.is_err());
    let error = result.expect_err("expected error");
    assert!(
        matches!(error, EngineError::Validation { .. }),
        "Expected invalid current-path validation error, got: {:?}",
        error
    );
}

#[tokio::test]
async fn create_new_note_rolls_back_when_metadata_save_fails() {
    let harness = NotebookTestHarness::new("create_rollback_metadata_failure");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    harness.inject_metadata_trap_directory();

    let result = manager.create_note("rollback/create", &mut notes).await;

    assert!(result.is_err());
    assert!(notes.is_empty(), "In-memory metadata should be rolled back");
    assert_note_md_not_exists(harness.path(), "rollback/create");
    assert!(
        !harness.path().join("rollback/create").exists(),
        "Created note directory should be rolled back on metadata failure"
    );
}

#[tokio::test]
async fn delete_note_rolls_back_when_metadata_save_fails() {
    let harness = NotebookTestHarness::new("delete_rollback_metadata_failure");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
        rel_path: "rollback/delete".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];

    let note_dir = harness.path().join("rollback/delete");
    std::fs::create_dir_all(&note_dir).expect("Failed to create note directory");
    std::fs::write(note_dir.join("note.md"), "rollback").expect("Failed to create note file");

    harness.inject_metadata_trap_directory();

    let result = manager.delete_note("rollback/delete", &mut notes).await;

    assert!(result.is_err());
    assert_eq!(notes.len(), 1, "Metadata should be restored on rollback");
    assert_eq!(notes[0].rel_path, "rollback/delete");
    assert_note_md_exists(harness.path(), "rollback/delete");
}

#[tokio::test]
async fn move_note_rolls_back_when_metadata_save_fails() {
    let harness = NotebookTestHarness::new("move_rollback_metadata_failure");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
        rel_path: "rollback/source".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];

    let source_dir = harness.path().join("rollback/source");
    std::fs::create_dir_all(&source_dir).expect("Failed to create source note directory");
    std::fs::write(source_dir.join("note.md"), "rollback")
        .expect("Failed to create source note file");

    harness.inject_metadata_trap_directory();

    let result = manager.move_note("rollback/source", "rollback/destination", &mut notes).await;

    assert!(result.is_err());
    assert_eq!(notes.len(), 1, "Metadata should be restored on rollback");
    assert_eq!(notes[0].rel_path, "rollback/source");
    assert_note_md_exists(harness.path(), "rollback/source");
    assert_note_md_not_exists(harness.path(), "rollback/destination");
}

#[tokio::test]
async fn save_note_content_creates_parent_directories_and_persists_text_without_metadata_rewrite() {
    let harness = NotebookTestHarness::new("save_content");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager.create_note("new/note", &mut notes).await.unwrap();

    notes[0].last_updated = Some("2000-01-01T00:00:00Z".to_string());
    manager.save_metadata(&notes).await.unwrap();

    manager
        .save_note_content("new/note", "hello from test")
        .await
        .expect("save_note_content should succeed");

    let content = std::fs::read_to_string(harness.path().join("new/note/note.md"))
        .expect("Failed to read saved note content");

    assert_eq!(content, "hello from test");

    let persisted_before_load =
        std::fs::read_to_string(harness.path().join("metadata.json"))
            .expect("Failed to read metadata after content save");
    assert!(
        persisted_before_load.contains("\"last_updated\": \"2000-01-01T00:00:00Z\""),
        "save_note_content should not rewrite metadata immediately"
    );

    let loaded = load_notes_or_panic(&manager).await;
    assert_eq!(loaded.len(), 1);
    assert_ne!(
        loaded[0].last_updated.as_deref(),
        Some("2000-01-01T00:00:00Z"),
        "load_metadata should reconcile stale last_updated from note file mtime"
    );
}

#[tokio::test]
async fn save_note_content_does_not_update_last_updated_when_content_is_unchanged() {
    let harness = NotebookTestHarness::new("save_content_no_change");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager.create_note("same/note", &mut notes).await.unwrap();

    std::fs::write(
        harness.path().join("same/note/note.md"),
        "same content",
    )
    .expect("Failed to seed note content");

    notes[0].last_updated = Some("2000-01-01T00:00:00Z".to_string());
    manager.save_metadata(&notes).await.unwrap();

    manager
        .save_note_content("same/note", "same content")
        .await
        .expect("save_note_content should succeed");

    let persisted_metadata =
        std::fs::read_to_string(harness.path().join("metadata.json"))
            .expect("Expected metadata file to remain readable after no-op save");
    assert!(
        persisted_metadata.contains("\"last_updated\": \"2000-01-01T00:00:00Z\""),
        "save_note_content should not rewrite metadata when content is unchanged"
    );
}

#[tokio::test]
async fn load_notes_metadata_errors_for_invalid_json_without_backup() {
    let harness = NotebookTestHarness::new("invalid_metadata");
    let manager = harness.manager();
    std::fs::write(
        harness.path().join("metadata.json"),
        "{ not_valid_json ",
    )
    .expect("Failed to write invalid metadata");

    let load_result = manager.load_metadata().await;

    assert!(load_result.is_err());
    let error = load_result.expect_err("Expected invalid metadata load to fail");
    assert!(
        matches!(error, EngineError::Recovery { .. }),
        "Expected Recovery error, got: {:?}",
        error
    );
}

#[tokio::test]
async fn load_notes_metadata_errors_when_primary_and_backup_are_corrupted() {
    let harness = NotebookTestHarness::new("invalid_metadata_and_backup");
    let manager = harness.manager();
    std::fs::write(
        harness.path().join("metadata.json"),
        "{ invalid_primary_json ",
    )
    .expect("Failed to write corrupted primary metadata");
    std::fs::write(
        harness.path().join("metadata.json.bak"),
        "{ invalid_backup_json ",
    )
    .expect("Failed to write corrupted backup metadata");

    let load_result = manager.load_metadata().await;

    assert!(load_result.is_err());
    let error = load_result.expect_err("Expected metadata recovery to fail");
    assert!(
        matches!(error, EngineError::Recovery { .. }),
        "Expected Recovery error, got: {:?}",
        error
    );
}

#[tokio::test]
async fn load_notes_metadata_backfills_missing_last_updated() {
    let harness = NotebookTestHarness::new("backfill_last_updated");
    let manager = harness.manager();
    let note_dir = harness.path().join("legacy/note");

    std::fs::create_dir_all(&note_dir).expect("Failed to create legacy note directory");
    std::fs::write(note_dir.join("note.md"), "legacy").expect("Failed to create legacy note file");
    std::fs::write(
        harness.path().join("metadata.json"),
        r#"{
  "notes": [
    {
      "rel_path": "legacy/note",
      "labels": ["legacy"]
    }
  ]
}"#,
    )
    .expect("Failed to write legacy metadata");

    let loaded = load_notes_or_panic(&manager).await;

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].rel_path, "legacy/note");
    assert!(loaded[0].last_updated.is_some());
    assert!(
        !loaded[0]
            .last_updated
            .as_deref()
            .unwrap_or("")
            .contains('.'),
        "backfilled last_updated should not include subsecond precision"
    );

    let persisted = std::fs::read_to_string(harness.path().join("metadata.json"))
        .expect("Failed to read metadata after backfill");
    assert!(persisted.contains("last_updated"));
}

#[tokio::test]
async fn load_notes_metadata_refreshes_stale_last_updated_from_note_file_mtime() {
    let harness = NotebookTestHarness::new("refresh_stale_last_updated");
    let manager = harness.manager();
    let note_dir = harness.path().join("stale/note");

    std::fs::create_dir_all(&note_dir).expect("Failed to create stale note directory");
    std::fs::write(note_dir.join("note.md"), "stale").expect("Failed to write stale note file");
    std::fs::write(
        harness.path().join("metadata.json"),
        r#"{
  "notes": [
    {
      "rel_path": "stale/note",
      "labels": ["legacy"],
      "last_updated": "2000-01-01T00:00:00Z"
    }
  ]
}"#,
    )
    .expect("Failed to write stale metadata");

    let loaded = load_notes_or_panic(&manager).await;

    assert_eq!(loaded.len(), 1);
    assert_ne!(
        loaded[0].last_updated.as_deref(),
        Some("2000-01-01T00:00:00Z"),
        "Expected stale last_updated to be refreshed from note file mtime"
    );

    let persisted = std::fs::read_to_string(harness.path().join("metadata.json"))
        .expect("Failed to read refreshed metadata");
    assert!(
        !persisted.contains("\"last_updated\": \"2000-01-01T00:00:00Z\""),
        "Expected refreshed metadata to replace the stale timestamp"
    );
}

#[tokio::test]
async fn load_notes_metadata_cleans_up_stale_staged_delete_entries() {
    let harness = NotebookTestHarness::new("cleanup_stale_staged_delete");
    let manager = harness.manager();
    let stale_stage = harness.path().join(".cognate_txn_delete_rollback__note_1");
    std::fs::create_dir_all(stale_stage.join("nested"))
        .expect("Failed to create stale staged delete directory");
    std::fs::write(stale_stage.join("nested").join("note.md"), "stale")
        .expect("Failed to populate stale staged delete directory");

    let _ = manager.load_metadata().await;

    assert!(
        !stale_stage.exists(),
        "Expected stale staged delete directory to be cleaned up"
    );
}

#[tokio::test]
async fn load_notes_metadata_keeps_recent_staged_delete_entries() {
    let harness = NotebookTestHarness::new("keep_recent_staged_delete");
    let manager = harness.manager();
    let recent_stage = harness.path().join(format!(
        ".cognate_txn_delete_rollback__note_{}",
        now_nanos()
    ));
    std::fs::create_dir_all(recent_stage.join("nested"))
        .expect("Failed to create recent staged delete directory");
    std::fs::write(recent_stage.join("nested").join("note.md"), "recent")
        .expect("Failed to populate recent staged delete directory");

    let _ = manager.load_metadata().await;

    assert!(
        recent_stage.exists(),
        "Expected recent staged delete directory to remain for in-flight safety"
    );
}

#[tokio::test]
async fn search_notes_finds_matches_in_path_label_and_content() {
    let harness = NotebookTestHarness::new("search_notes");
    let manager = harness.manager();
    let mut search_index = SearchIndexManager::new(harness.path());

    let mut notes: Vec<NoteMetadata> = Vec::new();
    manager.create_note("work/todo", &mut notes).await.expect("Failed to create work/todo");
    manager.create_note("ideas/brainstorm", &mut notes).await.expect("Failed to create ideas/brainstorm");

    if let Some(first_note) = notes.iter_mut().find(|note| note.rel_path == "work/todo") {
        first_note.labels.push("urgent".to_string());
    }

    std::fs::write(
        harness.path().join("ideas/brainstorm/note.md"),
        "Need to build an indexing strategy for search results.",
    )
    .expect("Failed to write brainstorm note content");

    let path_results = search_index
        .search("work", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(path_results.len(), 1);
    assert_eq!(path_results[0].rel_path, "work/todo");
    assert_eq!(path_results[0].snippet, "Path match");

    let label_results = search_index
        .search("urgent", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(label_results.len(), 1);
    assert_eq!(label_results[0].rel_path, "work/todo");
    assert!(
        label_results[0].snippet.contains("Label match"),
        "Expected snippet to indicate label match, got: {}",
        label_results[0].snippet
    );

    let content_results = search_index
        .search("indexing", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(content_results.len(), 1);
    assert_eq!(content_results[0].rel_path, "ideas/brainstorm");
    assert!(
        content_results[0]
            .snippet
            .to_lowercase()
            .contains("indexing"),
        "Expected snippet to include matching content"
    );
}

#[tokio::test]
async fn move_folder_updates_nested_note_paths() {
    let harness = NotebookTestHarness::new("move_folder");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager.create_note("folder/note_a", &mut notes).await.expect("Failed to create folder/note_a");
    manager.create_note("folder/sub/note_b", &mut notes).await.expect("Failed to create folder/sub/note_b");

    let moved_to = manager
        .move_note("folder", "renamed", &mut notes)
        .await
        .expect("move_note for folder should succeed");

    assert_eq!(moved_to, "renamed");
    assert_note_md_not_exists(harness.path(), "folder/note_a");
    assert_note_md_not_exists(harness.path(), "folder/sub/note_b");
    assert_note_md_exists(harness.path(), "renamed/note_a");
    assert_note_md_exists(harness.path(), "renamed/sub/note_b");

    let mut rel_paths: Vec<String> = notes.iter().map(|n| n.rel_path.clone()).collect();
    rel_paths.sort();
    assert_eq!(
        rel_paths,
        vec![
            "renamed/note_a".to_string(),
            "renamed/sub/note_b".to_string()
        ]
    );
}

#[tokio::test]
async fn load_notes_metadata_recovers_from_backup_when_primary_is_corrupted() {
    let harness = NotebookTestHarness::new("metadata_recovery_from_backup");
    let manager = harness.manager();

    std::fs::write(
        harness.path().join("metadata.json.bak"),
        r#"{
  "notes": [
    {
      "rel_path": "recovered/note",
      "labels": ["restored"],
      "last_updated": "2024-01-01T00:00:00Z"
    }
  ]
}"#,
    )
    .expect("Failed to write metadata backup fixture");
    std::fs::write(
        harness.path().join("metadata.json"),
        "{ corrupt_primary_json ",
    )
    .expect("Failed to write corrupted metadata.json fixture");

    let load_result = manager
        .load_metadata()
        .await
        .expect("Expected metadata load to recover from backup");

    assert_eq!(load_result.notes.len(), 1);
    assert_eq!(load_result.notes[0].rel_path, "recovered/note");
    assert!(
        load_result.warning.is_some(),
        "Recovery path should surface a warning"
    );

    let restored_primary = std::fs::read_to_string(harness.path().join("metadata.json"))
        .expect("Expected metadata.json to be restored from backup");
    assert!(
        restored_primary.contains("recovered/note"),
        "Primary metadata should be restored from backup contents"
    );
}

#[tokio::test]
async fn save_metadata_keeps_last_known_good_copy_and_preserves_primary_when_atomic_rename_fails() {
    let harness = NotebookTestHarness::new("metadata_backup_and_atomic_failure");
    let manager = harness.manager();
    let initial_notes = vec![NoteMetadata {
        rel_path: "stable/note".to_string(),
        labels: vec!["v1".to_string()],
        last_updated: Some("2024-01-01T00:00:00Z".to_string()),
    }];
    harness.write_metadata(&initial_notes).await;

    harness.inject_rename_fault();

    let updated_notes = vec![NoteMetadata {
        rel_path: "stable/note".to_string(),
        labels: vec!["v2".to_string()],
        last_updated: Some("2024-01-02T00:00:00Z".to_string()),
    }];
    let save_result = manager.save_metadata(&updated_notes).await;

    assert!(
        save_result.is_err(),
        "Expected save to fail on simulated rename error"
    );

    let primary_after_failure = std::fs::read_to_string(harness.path().join("metadata.json"))
        .expect("Failed to read metadata.json after simulated rename failure");
    assert!(
        primary_after_failure.contains("\"v1\""),
        "Atomic save should keep the previous metadata.json when rename fails"
    );
    assert!(
        !primary_after_failure.contains("\"v2\""),
        "Failed atomic save must not partially apply new metadata"
    );

    let backup_after_failure = std::fs::read_to_string(harness.path().join("metadata.json.bak"))
        .expect("Failed to read metadata.json.bak after simulated rename failure");
    assert!(
        backup_after_failure.contains("\"v1\""),
        "Backup should preserve the last known-good metadata snapshot"
    );
}

#[tokio::test]
async fn delete_note_surfaces_failed_rollback_when_rollback_rename_fails() {
    let harness = NotebookTestHarness::new("delete_rollback_failure_surface");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
        rel_path: "rollback/failure".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];

    let note_dir = harness.path().join("rollback/failure");
    std::fs::create_dir_all(&note_dir).expect("Failed to create rollback target note directory");
    std::fs::write(note_dir.join("note.md"), "rollback failure")
        .expect("Failed to write rollback failure note");

    harness.inject_metadata_trap_directory();
    harness.inject_delete_rollback_fault();

    let delete_result = manager.delete_note("rollback/failure", &mut notes).await;

    assert!(delete_result.is_err());
    let error = delete_result.expect_err("Expected delete to fail");
    assert!(
        matches!(error, EngineError::Recovery { .. }),
        "Expected Recovery error, got: {:?}",
        error
    );
    assert!(
        error
            .to_string()
            .contains("Rollback failed while restoring filesystem state"),
        "Expected explicit rollback failure message, got: {}",
        error
    );

    let staged_entries: Vec<PathBuf> = std::fs::read_dir(harness.path())
        .expect("Failed to scan notebook directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(".cognate_txn_delete_"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        !staged_entries.is_empty(),
        "Expected failed rollback to leave a staged delete entry for manual recovery"
    );
}

#[tokio::test]
async fn move_note_surfaces_failed_rollback_when_rollback_rename_fails() {
    let harness = NotebookTestHarness::new("move_rollback_failure_surface");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
        rel_path: "rollback/source".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];

    let source_dir = harness.path().join("rollback/source");
    std::fs::create_dir_all(&source_dir).expect("Failed to create source note directory");
    std::fs::write(source_dir.join("note.md"), "rollback failure")
        .expect("Failed to write source note file");

    harness.inject_metadata_trap_directory();
    harness.inject_move_rollback_fault();

    let move_result = manager.move_note("rollback/source", "rollback/destination", &mut notes).await;

    assert!(move_result.is_err());
    let error = move_result.expect_err("Expected move to fail");
    assert!(
        matches!(error, EngineError::Recovery { .. }),
        "Expected Recovery error, got: {:?}",
        error
    );
    assert!(
        error
            .to_string()
            .contains("Rollback failed while restoring filesystem state"),
        "Expected explicit rollback failure message, got: {}",
        error
    );

    assert!(
        harness.path().join("rollback/source").join("note.md").exists()
            || harness.path().join("rollback/destination").join("note.md").exists(),
        "Failed move rollback should leave recoverable note data on disk"
    );
}

#[tokio::test]
async fn test_async_attachments() {
    let temp = TempTestDir::new("attachments");
    let manager = NotebookManager::new(temp.path());

    let mut notes = Vec::new();
    manager.create_note("work/notes", &mut notes).await.unwrap();

    let dummy_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    let rel_image_path = AttachmentManager::save_image_from_base64(
        temp.path(),
        "work/notes",
        dummy_base64,
    )
    .await
    .expect("Failed to save image attachment");

    assert!(rel_image_path.starts_with("images/"));
    let full_image_path = temp.path().join("work/notes").join(&rel_image_path);
    assert!(full_image_path.exists());

    let notebook_rel_image_path = format!("work/notes/{}", rel_image_path);

    let bytes = AttachmentManager::read_image_bytes(
        temp.path(),
        &notebook_rel_image_path,
    )
    .await
    .expect("Failed to read image bytes");

    assert!(!bytes.is_empty());
    assert_eq!(&bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]);

    AttachmentManager::delete_attachment(
        temp.path(),
        &notebook_rel_image_path,
    )
    .await
    .expect("Failed to delete attachment");

    assert!(!temp.path().join(&notebook_rel_image_path).exists());
}

#[tokio::test]
async fn test_search_cache_operations() {
    let temp = TempTestDir::new("search_cache");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    manager.create_note("ideas/project-a", &mut notes).await.unwrap();
    manager.save_note_content("ideas/project-a", "We should build an antigravity application").await.unwrap();

    let results = search_index
        .search("antigravity", &notes, Duration::from_secs(60))
        .await
        .expect("Search failed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].rel_path, "ideas/project-a");

    search_index.cache_upsert("ideas/project-a", "Brand new content containing teleportation", None);
    let results2 = search_index
        .search("teleportation", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(results2.len(), 1);

    search_index.cache_rename("ideas/project-a", "ideas/project-b");
    notes[0].rel_path = "ideas/project-b".to_string();
    let results3 = search_index
        .search("teleportation", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(results3.len(), 1);
    assert_eq!(results3[0].rel_path, "ideas/project-b");

    search_index.cache_remove("ideas/project-b");
    notes.clear();
    let results4 = search_index
        .search("teleportation", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert!(results4.is_empty());
}

#[tokio::test]
async fn test_search_index_stale_refresh() {
    let temp = TempTestDir::new("stale_refresh");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    let _ = manager.create_note("note", &mut notes).await.unwrap();
    manager.save_note_content("note", "original text").await.unwrap();

    let results = search_index
        .search("original", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    let file_path = temp.path().join("note/note.md");
    tokio::fs::write(&file_path, "modified external text").await.unwrap();

    let future_mtime = FileTime::from_system_time(SystemTime::now() + Duration::from_secs(5));
    filetime::set_file_mtime(&file_path, future_mtime).unwrap();

    let results2 = search_index
        .search("external", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(results2.len(), 1);
    assert_eq!(results2[0].rel_path, "note");
}

use filetime::FileTime;
