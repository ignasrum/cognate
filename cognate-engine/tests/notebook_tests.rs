mod common;

use std::path::PathBuf;

use cognate_engine::EngineError;
use cognate_engine::storage::{ConcurrencyManager, NoteMetadata, NotebookManager};
use common::{
    NotebookTestHarness, TempTestDir, assert_note_md_exists, assert_note_md_not_exists,
    load_notes_or_panic, now_nanos,
};

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

    manager
        .create_note("alpha", &mut notes)
        .await
        .expect("Failed to create alpha");
    manager
        .create_note("beta", &mut notes)
        .await
        .expect("Failed to create beta");

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
async fn delete_note_preserves_child_notes_when_parent_has_same_named_folder() {
    let harness = NotebookTestHarness::new("delete_parent_note_with_children");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("Test", &mut notes)
        .await
        .expect("Failed to create parent note");
    manager
        .create_note("Test/child", &mut notes)
        .await
        .expect("Failed to create child note");

    manager
        .delete_note("Test", &mut notes)
        .await
        .expect("Deleting parent note should succeed");

    assert!(!harness.path().join("Test/note.md").exists());
    assert!(harness.path().join("Test/child/note.md").exists());
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].rel_path, "Test/child");
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
async fn load_metadata_skips_invalid_relative_paths_from_disk() {
    let harness = NotebookTestHarness::new("load_metadata_skips_invalid_paths");
    std::fs::write(
        harness.path().join("metadata.json"),
        r#"{
  "notes": [
    { "rel_path": "valid/note", "labels": ["ok"] },
    { "rel_path": "../outside", "labels": ["bad"] }
  ]
}"#,
    )
    .expect("Failed to write metadata fixture");
    std::fs::create_dir_all(harness.path().join("valid/note"))
        .expect("Failed to create valid note directory");
    std::fs::write(harness.path().join("valid/note/note.md"), "safe content")
        .expect("Failed to write valid note");

    let loaded = harness
        .manager()
        .load_metadata()
        .await
        .expect("Expected metadata load to succeed");

    assert_eq!(loaded.notes.len(), 1);
    assert_eq!(loaded.notes[0].rel_path, "valid/note");
    assert!(
        loaded
            .warning
            .as_deref()
            .unwrap_or("")
            .contains("Skipped invalid metadata entry '../outside'"),
        "Expected warning about sanitized invalid metadata path"
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

    manager
        .create_note("source/note", &mut notes)
        .await
        .unwrap();
    manager
        .create_note("target/note", &mut notes)
        .await
        .unwrap();

    let result = manager
        .move_note("source/note", "target/note", &mut notes)
        .await;

    assert!(result.is_err());
    assert_note_md_exists(harness.path(), "source/note");
    assert_note_md_exists(harness.path(), "target/note");
}

#[tokio::test]
async fn move_note_into_descendant_preserves_source_folder_and_children() {
    let harness = NotebookTestHarness::new("move_note_into_itself");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("test/readme", &mut notes)
        .await
        .expect("Failed to create source note");
    manager
        .create_note("test/readme/existing", &mut notes)
        .await
        .expect("Failed to create existing child note");

    manager
        .move_note("test/readme", "test/readme/note", &mut notes)
        .await
        .expect("Nested note move should succeed");

    assert!(!harness.path().join("test/readme/note.md").exists());
    assert!(harness.path().join("test/readme/note/note.md").exists());
    assert!(harness.path().join("test/readme/existing/note.md").exists());
    assert!(notes.iter().any(|note| note.rel_path == "test/readme/note"));
    assert!(
        notes
            .iter()
            .any(|note| note.rel_path == "test/readme/existing")
    );
}

#[tokio::test]
async fn move_note_rejects_invalid_current_relative_path() {
    let harness = NotebookTestHarness::new("move_invalid_current_path");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    let result = manager
        .move_note("../outside", "target/note", &mut notes)
        .await;

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

    let result = manager
        .move_note("rollback/source", "rollback/destination", &mut notes)
        .await;

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

    let persisted_before_load = std::fs::read_to_string(harness.path().join("metadata.json"))
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

    std::fs::write(harness.path().join("same/note/note.md"), "same content")
        .expect("Failed to seed note content");

    notes[0].last_updated = Some("2000-01-01T00:00:00Z".to_string());
    manager.save_metadata(&notes).await.unwrap();

    manager
        .save_note_content("same/note", "same content")
        .await
        .expect("save_note_content should succeed");

    let persisted_metadata = std::fs::read_to_string(harness.path().join("metadata.json"))
        .expect("Expected metadata file to remain readable after no-op save");
    assert!(
        persisted_metadata.contains("\"last_updated\": \"2000-01-01T00:00:00Z\""),
        "save_note_content should not rewrite metadata when content is unchanged"
    );
}

#[tokio::test]
async fn note_content_save_releases_all_locks_after_completion() {
    let harness = NotebookTestHarness::new("save_releases_locks");
    let manager = harness.manager();

    manager
        .save_note_content("ai/summary", "generated summary")
        .await
        .expect("note content save should succeed");

    let concurrency = ConcurrencyManager::new(harness.path());
    assert!(concurrency.acquire_notebook().await.is_ok());
    assert!(concurrency.acquire_note("ai/summary").await.is_ok());
}

#[tokio::test]
async fn concurrent_note_saves_from_separate_managers_preserve_complete_files() {
    let harness = NotebookTestHarness::new("concurrent_note_saves");
    let first = harness.manager();
    let second = harness.manager();

    let (first_result, second_result) = tokio::join!(
        first.save_note_content("one", "first complete content"),
        second.save_note_content("two", "second complete content"),
    );

    first_result.expect("first concurrent note save should succeed");
    second_result.expect("second concurrent note save should succeed");
    assert_eq!(
        std::fs::read_to_string(harness.path().join("one/note.md")).unwrap(),
        "first complete content"
    );
    assert_eq!(
        std::fs::read_to_string(harness.path().join("two/note.md")).unwrap(),
        "second complete content"
    );
}

#[tokio::test]
async fn load_notes_metadata_errors_for_invalid_json_without_backup() {
    let harness = NotebookTestHarness::new("invalid_metadata");
    let manager = harness.manager();
    std::fs::write(harness.path().join("metadata.json"), "{ not_valid_json ")
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

    let stale_file = harness.path().join(".cognate_txn_delete_rollback__note_2");
    std::fs::write(&stale_file, "stale file contents")
        .expect("Failed to create stale staged delete file");

    let _ = manager.load_metadata().await;

    assert!(
        !stale_stage.exists(),
        "Expected stale staged delete directory to be cleaned up"
    );
    assert!(
        !stale_file.exists(),
        "Expected stale staged delete file to be cleaned up"
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
async fn move_folder_updates_nested_note_paths() {
    let harness = NotebookTestHarness::new("move_folder");
    let manager = harness.manager();
    let mut notes: Vec<NoteMetadata> = Vec::new();

    manager
        .create_note("folder/note_a", &mut notes)
        .await
        .expect("Failed to create folder/note_a");
    manager
        .create_note("folder/sub/note_b", &mut notes)
        .await
        .expect("Failed to create folder/sub/note_b");

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
async fn load_notes_metadata_recovers_from_backup_when_primary_is_missing() {
    let harness = NotebookTestHarness::new("metadata_missing_primary_recovery");
    let manager = harness.manager();
    let backup = r#"{
  "notes": [
    {
      "rel_path": "recovered/missing-primary",
      "labels": ["restored"],
      "last_updated": "2024-01-01T00:00:00Z"
    }
  ]
}"#;
    std::fs::write(harness.path().join("metadata.json.bak"), backup)
        .expect("Failed to write metadata backup fixture");

    let load_result = manager
        .load_metadata()
        .await
        .expect("Expected missing primary metadata to recover from backup");

    assert_eq!(load_result.notes[0].rel_path, "recovered/missing-primary");
    assert!(load_result.warning.is_some());
    assert_eq!(
        std::fs::read_to_string(harness.path().join("metadata.json")).unwrap(),
        backup
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
async fn note_content_write_preserves_previous_content_when_storage_is_full() {
    let harness = NotebookTestHarness::new("note_storage_full");
    let manager = harness.manager();
    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();
    manager.save_note_content("note", "before").await.unwrap();

    harness.inject_storage_full_fault();
    let result = manager.save_note_content("note", "after").await;
    assert!(result.is_err());
    assert_eq!(manager.load_note_content("note").await.unwrap(), "before");

    std::fs::remove_file(harness.path().join(".cognate_fail_atomic_write")).unwrap();
    manager.save_note_content("note", "after").await.unwrap();
    assert_eq!(manager.load_note_content("note").await.unwrap(), "after");
}

#[tokio::test]
async fn metadata_write_preserves_previous_snapshot_when_storage_is_full() {
    let harness = NotebookTestHarness::new("metadata_storage_full");
    let manager = harness.manager();
    let initial = vec![NoteMetadata {
        rel_path: "note".to_string(),
        labels: vec!["before".to_string()],
        last_updated: None,
    }];
    harness.write_metadata(&initial).await;

    harness.inject_storage_full_fault();
    let updated = vec![NoteMetadata {
        rel_path: "note".to_string(),
        labels: vec!["after".to_string()],
        last_updated: None,
    }];
    assert!(manager.save_metadata(&updated).await.is_err());
    assert_eq!(manager.load_metadata().await.unwrap().notes, initial);

    std::fs::remove_file(harness.path().join(".cognate_fail_atomic_write")).unwrap();
    manager.save_metadata(&updated).await.unwrap();
    assert_eq!(manager.load_metadata().await.unwrap().notes, updated);
}

#[tokio::test]
async fn fresh_manager_recovers_metadata_and_note_content_after_restart() {
    let harness = NotebookTestHarness::new("manager_restart_recovery");
    let first = harness.manager();
    let mut notes = Vec::new();
    first.create_note("restart/note", &mut notes).await.unwrap();
    first
        .save_note_content("restart/note", "durable content")
        .await
        .unwrap();
    drop(first);

    let second = harness.manager();
    let loaded = second.load_metadata().await.unwrap();
    assert_eq!(loaded.notes, notes);
    assert_eq!(
        second.load_note_content("restart/note").await.unwrap(),
        "durable content"
    );
    assert!(
        second
            .load_engine_state()
            .await
            .unwrap()
            .search_index
            .documents
            .contains_key("restart/note")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unwritable_notebook_reports_error_without_losing_existing_note() {
    use std::os::unix::fs::PermissionsExt;

    let harness = NotebookTestHarness::new("readonly_notebook");
    let manager = harness.manager();
    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();
    manager.save_note_content("note", "before").await.unwrap();

    let original_mode = std::fs::metadata(harness.path())
        .unwrap()
        .permissions()
        .mode();
    let mut read_only = std::fs::metadata(harness.path()).unwrap().permissions();
    read_only.set_mode(original_mode & !0o222);
    std::fs::set_permissions(harness.path(), read_only).unwrap();

    let result = manager.save_note_content("note", "after").await;

    let mut restored = std::fs::metadata(harness.path()).unwrap().permissions();
    restored.set_mode(original_mode);
    std::fs::set_permissions(harness.path(), restored).unwrap();

    if result.is_ok() {
        eprintln!("skipping read-only assertion: test process can write as privileged user");
        return;
    }
    assert_eq!(manager.load_note_content("note").await.unwrap(), "before");
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

    let move_result = manager
        .move_note("rollback/source", "rollback/destination", &mut notes)
        .await;

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
        harness
            .path()
            .join("rollback/source")
            .join("note.md")
            .exists()
            || harness
                .path()
                .join("rollback/destination")
                .join("note.md")
                .exists(),
        "Failed move rollback should leave recoverable note data on disk"
    );
}

#[tokio::test]
async fn test_create_note_directory_already_exists() {
    let harness = NotebookTestHarness::new("dir_exists");
    let manager = harness.manager();
    let mut notes = Vec::new();

    let note_dir = harness.path().join("existing/dir");
    std::fs::create_dir_all(&note_dir).unwrap();

    let res = manager.create_note("existing/dir", &mut notes).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_move_note_to_same_path() {
    let harness = NotebookTestHarness::new("move_same");
    let manager = harness.manager();
    let mut notes = Vec::new();

    manager.create_note("note", &mut notes).await.unwrap();

    let res = manager.move_note("note", "note", &mut notes).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_save_metadata_empty_list() {
    let harness = NotebookTestHarness::new("empty_meta");
    let manager = harness.manager();

    let res = manager.save_metadata(&[]).await;
    assert!(res.is_ok());

    let loaded = manager.load_metadata().await.unwrap();
    assert!(loaded.notes.is_empty());
}

#[tokio::test]
#[cfg(unix)]
async fn test_delete_note_symbolic_link_escape() {
    let harness = NotebookTestHarness::new("symlink_escape");
    let manager = harness.manager();
    let mut notes = vec![NoteMetadata {
        rel_path: "linked_folder/note".to_string(),
        labels: Vec::new(),
        last_updated: None,
    }];

    // 1. Create a note directory and file outside the notebook
    let outside_dir = std::env::temp_dir().join(format!("cognate_outside_{}", now_nanos()));
    let outside_note_dir = outside_dir.join("note");
    std::fs::create_dir_all(&outside_note_dir).unwrap();
    std::fs::write(outside_note_dir.join("note.md"), "outside note").unwrap();

    // 2. Create a symlink pointing to the outside directory inside the notebook
    let symlink_path = harness.path().join("linked_folder");
    std::os::unix::fs::symlink(&outside_dir, &symlink_path).unwrap();

    // 3. Trying to delete linked_folder/note should fail because it canonicalizes outside the notebook
    let res = manager.delete_note("linked_folder/note", &mut notes).await;
    assert!(res.is_err(), "Expected symlink note deletion to fail");
    let error = res.unwrap_err();
    assert!(
        matches!(error, EngineError::Validation { .. }),
        "Expected validation error for symlink escape, got: {:?}",
        error
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&outside_dir);
}

#[tokio::test]
async fn test_metadata_subsecond_precision_removal() {
    let temp = TempTestDir::new("subsecond");
    let manager = NotebookManager::new(temp.path());

    let notes = vec![NoteMetadata {
        rel_path: "note-1".to_string(),
        labels: vec![],
        last_updated: Some("2026-07-13T12:00:00.123456Z".to_string()),
    }];

    manager.save_metadata(&notes).await.unwrap();

    let loaded = manager.load_metadata().await.unwrap();
    assert_eq!(
        loaded.notes[0].last_updated.as_deref(),
        Some("2026-07-13T12:00:00Z")
    );
}

#[tokio::test]
async fn test_metadata_subsecond_precision_no_timezone() {
    let temp = TempTestDir::new("subsecond_no_tz");
    let manager = NotebookManager::new(temp.path());

    let notes = vec![NoteMetadata {
        rel_path: "note-1".to_string(),
        labels: vec![],
        last_updated: Some("2026-07-13T12:00:00.123456".to_string()),
    }];

    manager.save_metadata(&notes).await.unwrap();

    let loaded = manager.load_metadata().await.unwrap();
    assert_eq!(
        loaded.notes[0].last_updated.as_deref(),
        Some("2026-07-13T12:00:00")
    );
}
