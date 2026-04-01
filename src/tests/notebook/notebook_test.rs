#[cfg(test)]
mod tests {
    use crate::notebook::{self, NoteMetadata, NotebookErrorKind};
    use std::fs;
    use std::future::Future;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct NoopWaker;

    impl Wake for NoopWaker {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWaker));
        let mut context = Context::from_waker(&waker);
        let mut future = Pin::from(Box::new(future));

        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    struct TestNotebookDir {
        path: PathBuf,
    }

    impl TestNotebookDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("System clock error")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "cognate_test_{}_{}_{}",
                name,
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("Failed to create temporary notebook directory");
            Self { path }
        }

        fn as_str(&self) -> &str {
            self.path
                .to_str()
                .expect("Temporary path must be valid UTF-8")
        }
    }

    impl Drop for TestNotebookDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn assert_note_md_exists(notebook: &TestNotebookDir, rel_path: &str) {
        let note_path = Path::new(notebook.as_str()).join(rel_path).join("note.md");
        assert!(
            note_path.exists(),
            "Expected note file to exist at '{}'",
            note_path.display()
        );
    }

    fn assert_note_md_not_exists(notebook: &TestNotebookDir, rel_path: &str) {
        let note_path = Path::new(notebook.as_str()).join(rel_path).join("note.md");
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

    fn load_notes_or_panic(notebook_dir: &TestNotebookDir) -> Vec<NoteMetadata> {
        block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ))
        .expect("Expected metadata load to succeed")
        .notes
    }

    #[test]
    fn create_new_note_creates_file_and_metadata() {
        let notebook_dir = TestNotebookDir::new("create_note");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        let created = block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "work/todo",
            &mut notes,
        ))
        .expect("create_new_note should succeed");

        assert_eq!(created.rel_path, "work/todo");
        assert!(created.last_updated.is_some());
        assert!(
            !created.last_updated.as_deref().unwrap_or("").contains('.'),
            "last_updated should not include subsecond precision"
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].last_updated.is_some());
        assert_note_md_exists(&notebook_dir, "work/todo");

        let loaded = load_notes_or_panic(&notebook_dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].rel_path, "work/todo");
        assert!(loaded[0].last_updated.is_some());
    }

    #[test]
    fn create_new_note_rejects_invalid_relative_path() {
        let notebook_dir = TestNotebookDir::new("invalid_path");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        let result = block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "../outside",
            &mut notes,
        ));

        assert!(result.is_err());
        assert!(notes.is_empty());
    }

    #[test]
    fn create_new_note_rejects_duplicate_metadata_path() {
        let notebook_dir = TestNotebookDir::new("duplicate_note");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "dup/note",
            &mut notes,
        ))
        .expect("Initial note creation should succeed");

        let duplicate = block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "dup/note",
            &mut notes,
        ));

        assert!(duplicate.is_err());
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn delete_note_removes_file_and_metadata() {
        let notebook_dir = TestNotebookDir::new("delete_note");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "alpha",
            &mut notes,
        ))
        .expect("Failed to create alpha");
        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "beta",
            &mut notes,
        ))
        .expect("Failed to create beta");

        block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "alpha",
            &mut notes,
        ))
        .expect("delete_note should succeed");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].rel_path, "beta");
        assert_note_md_not_exists(&notebook_dir, "alpha");
        assert_note_md_exists(&notebook_dir, "beta");

        let loaded = load_notes_or_panic(&notebook_dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].rel_path, "beta");
    }

    #[test]
    fn delete_note_removes_filesystem_item_even_if_missing_from_metadata() {
        let notebook_dir = TestNotebookDir::new("delete_without_metadata");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        let external_note_dir = Path::new(notebook_dir.as_str()).join("external/note");
        fs::create_dir_all(&external_note_dir).expect("Failed to create external note directory");
        fs::write(external_note_dir.join("note.md"), "externally created")
            .expect("Failed to create external note file");

        block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "external/note",
            &mut notes,
        ))
        .expect("Deletion should succeed even when metadata entry is missing");

        assert_note_md_not_exists(&notebook_dir, "external/note");
        assert!(notes.is_empty());
    }

    #[test]
    fn delete_note_removes_empty_parent_folders_after_note_delete() {
        let notebook_dir = TestNotebookDir::new("delete_removes_empty_parents");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "only_folder/only_note",
            &mut notes,
        ))
        .expect("Failed to create only_folder/only_note");

        block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "only_folder/only_note",
            &mut notes,
        ))
        .expect("delete_note should succeed");

        assert!(notes.is_empty(), "Metadata entry should be removed");
        assert!(
            !Path::new(notebook_dir.as_str())
                .join("only_folder")
                .exists(),
            "Parent folder should be removed when it becomes empty"
        );
    }

    #[test]
    fn delete_note_rejects_invalid_relative_path() {
        let notebook_dir = TestNotebookDir::new("delete_invalid_path");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        let result = block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "../outside",
            &mut notes,
        ));

        assert!(result.is_err());
        let error = result.expect_err("expected error");
        assert_eq!(error.kind(), NotebookErrorKind::Validation);
        assert!(
            error.to_string().contains("Invalid relative path"),
            "Expected invalid-path validation error",
        );
    }

    #[test]
    fn move_note_moves_files_and_updates_metadata() {
        let notebook_dir = TestNotebookDir::new("move_note");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "old/path",
            &mut notes,
        ))
        .expect("Failed to create note for move");

        let moved_to = block_on(notebook::move_note(
            notebook_dir.as_str(),
            "old/path",
            "new/path",
            &mut notes,
        ))
        .expect("move_note should succeed");

        assert_eq!(moved_to, "new/path");
        assert_note_md_not_exists(&notebook_dir, "old/path");
        assert_note_md_exists(&notebook_dir, "new/path");

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].rel_path, "new/path");

        let loaded = load_notes_or_panic(&notebook_dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].rel_path, "new/path");
    }

    #[test]
    fn move_note_fails_when_target_exists() {
        let notebook_dir = TestNotebookDir::new("move_target_exists");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "source/note",
            &mut notes,
        ))
        .expect("Failed to create source note");
        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "target/note",
            &mut notes,
        ))
        .expect("Failed to create target note");

        let result = block_on(notebook::move_note(
            notebook_dir.as_str(),
            "source/note",
            "target/note",
            &mut notes,
        ));

        assert!(result.is_err());
        assert_note_md_exists(&notebook_dir, "source/note");
        assert_note_md_exists(&notebook_dir, "target/note");
    }

    #[test]
    fn move_note_rejects_invalid_current_relative_path() {
        let notebook_dir = TestNotebookDir::new("move_invalid_current_path");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        let result = block_on(notebook::move_note(
            notebook_dir.as_str(),
            "../outside",
            "target/note",
            &mut notes,
        ));

        assert!(result.is_err());
        let error = result.expect_err("expected error");
        assert_eq!(error.kind(), NotebookErrorKind::Validation);
        assert!(
            error.to_string().contains("Invalid current relative path"),
            "Expected invalid current-path validation error",
        );
    }

    #[test]
    fn create_new_note_rolls_back_when_metadata_save_fails() {
        let notebook_dir = TestNotebookDir::new("create_rollback_metadata_failure");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        fs::create_dir(Path::new(notebook_dir.as_str()).join("metadata.json"))
            .expect("Failed to create metadata.json directory trap");

        let result = block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "rollback/create",
            &mut notes,
        ));

        assert!(result.is_err());
        assert!(notes.is_empty(), "In-memory metadata should be rolled back");
        assert_note_md_not_exists(&notebook_dir, "rollback/create");
        assert!(
            !Path::new(notebook_dir.as_str())
                .join("rollback/create")
                .exists(),
            "Created note directory should be rolled back on metadata failure"
        );
    }

    #[test]
    fn delete_note_rolls_back_when_metadata_save_fails() {
        let notebook_dir = TestNotebookDir::new("delete_rollback_metadata_failure");
        let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
            rel_path: "rollback/delete".to_string(),
            labels: Vec::new(),
            last_updated: None,
        }];

        let note_dir = Path::new(notebook_dir.as_str()).join("rollback/delete");
        fs::create_dir_all(&note_dir).expect("Failed to create note directory");
        fs::write(note_dir.join("note.md"), "rollback").expect("Failed to create note file");
        fs::create_dir(Path::new(notebook_dir.as_str()).join("metadata.json"))
            .expect("Failed to create metadata.json directory trap");

        let result = block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "rollback/delete",
            &mut notes,
        ));

        assert!(result.is_err());
        assert_eq!(notes.len(), 1, "Metadata should be restored on rollback");
        assert_eq!(notes[0].rel_path, "rollback/delete");
        assert_note_md_exists(&notebook_dir, "rollback/delete");
    }

    #[test]
    fn move_note_rolls_back_when_metadata_save_fails() {
        let notebook_dir = TestNotebookDir::new("move_rollback_metadata_failure");
        let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
            rel_path: "rollback/source".to_string(),
            labels: Vec::new(),
            last_updated: None,
        }];

        let source_dir = Path::new(notebook_dir.as_str()).join("rollback/source");
        fs::create_dir_all(&source_dir).expect("Failed to create source note directory");
        fs::write(source_dir.join("note.md"), "rollback")
            .expect("Failed to create source note file");
        fs::create_dir(Path::new(notebook_dir.as_str()).join("metadata.json"))
            .expect("Failed to create metadata.json directory trap");

        let result = block_on(notebook::move_note(
            notebook_dir.as_str(),
            "rollback/source",
            "rollback/destination",
            &mut notes,
        ));

        assert!(result.is_err());
        assert_eq!(notes.len(), 1, "Metadata should be restored on rollback");
        assert_eq!(notes[0].rel_path, "rollback/source");
        assert_note_md_exists(&notebook_dir, "rollback/source");
        assert_note_md_not_exists(&notebook_dir, "rollback/destination");
    }

    #[test]
    fn save_note_content_creates_parent_directories_and_persists_text_without_metadata_rewrite() {
        let notebook_dir = TestNotebookDir::new("save_content");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "new/note",
            &mut notes,
        ))
        .expect("create_new_note should succeed");

        notes[0].last_updated = Some("2000-01-01T00:00:00Z".to_string());
        notebook::save_metadata(notebook_dir.as_str(), &notes)
            .expect("save_metadata should succeed");

        block_on(notebook::save_note_content(
            notebook_dir.as_str().to_string(),
            "new/note".to_string(),
            "hello from test".to_string(),
        ))
        .expect("save_note_content should succeed");

        let content = fs::read_to_string(
            Path::new(notebook_dir.as_str())
                .join("new/note")
                .join("note.md"),
        )
        .expect("Failed to read saved note content");

        assert_eq!(content, "hello from test");

        let loaded = load_notes_or_panic(&notebook_dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0].last_updated.as_deref(),
            Some("2000-01-01T00:00:00Z"),
            "save_note_content should not rewrite metadata"
        );
    }

    #[test]
    fn save_note_content_does_not_update_last_updated_when_content_is_unchanged() {
        let notebook_dir = TestNotebookDir::new("save_content_no_change");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "same/note",
            &mut notes,
        ))
        .expect("create_new_note should succeed");

        fs::write(
            Path::new(notebook_dir.as_str())
                .join("same/note")
                .join("note.md"),
            "same content",
        )
        .expect("Failed to seed note content");

        notes[0].last_updated = Some("2000-01-01T00:00:00Z".to_string());
        notebook::save_metadata(notebook_dir.as_str(), &notes)
            .expect("save_metadata should succeed");

        block_on(notebook::save_note_content(
            notebook_dir.as_str().to_string(),
            "same/note".to_string(),
            "same content".to_string(),
        ))
        .expect("save_note_content should succeed");

        let loaded = load_notes_or_panic(&notebook_dir);
        assert_eq!(
            loaded[0].last_updated.as_deref(),
            Some("2000-01-01T00:00:00Z"),
            "last_updated should not change when content is unchanged"
        );
    }

    #[test]
    fn load_notes_metadata_errors_for_invalid_json_without_backup() {
        let notebook_dir = TestNotebookDir::new("invalid_metadata");
        fs::write(
            Path::new(notebook_dir.as_str()).join("metadata.json"),
            "{ not_valid_json ",
        )
        .expect("Failed to write invalid metadata");

        let load_result = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));

        assert!(load_result.is_err());
        let error = load_result.expect_err("Expected invalid metadata load to fail");
        assert_eq!(error.kind(), NotebookErrorKind::Recovery);
        assert!(error.to_string().contains("Failed to parse metadata"));
    }

    #[test]
    fn load_notes_metadata_backfills_missing_last_updated() {
        let notebook_dir = TestNotebookDir::new("backfill_last_updated");
        let note_dir = Path::new(notebook_dir.as_str()).join("legacy/note");

        fs::create_dir_all(&note_dir).expect("Failed to create legacy note directory");
        fs::write(note_dir.join("note.md"), "legacy").expect("Failed to create legacy note file");
        fs::write(
            Path::new(notebook_dir.as_str()).join("metadata.json"),
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

        let loaded = load_notes_or_panic(&notebook_dir);

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

        let persisted = fs::read_to_string(Path::new(notebook_dir.as_str()).join("metadata.json"))
            .expect("Failed to read metadata after backfill");
        assert!(persisted.contains("last_updated"));
    }

    #[test]
    fn load_notes_metadata_cleans_up_stale_staged_delete_entries() {
        let notebook_dir = TestNotebookDir::new("cleanup_stale_staged_delete");
        let stale_stage =
            Path::new(notebook_dir.as_str()).join(".cognate_txn_delete_rollback__note_1");
        fs::create_dir_all(stale_stage.join("nested"))
            .expect("Failed to create stale staged delete directory");
        fs::write(stale_stage.join("nested").join("note.md"), "stale")
            .expect("Failed to populate stale staged delete directory");

        let _ = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));

        assert!(
            !stale_stage.exists(),
            "Expected stale staged delete directory to be cleaned up"
        );
    }

    #[test]
    fn load_notes_metadata_keeps_recent_staged_delete_entries() {
        let notebook_dir = TestNotebookDir::new("keep_recent_staged_delete");
        let recent_stage = Path::new(notebook_dir.as_str()).join(format!(
            ".cognate_txn_delete_rollback__note_{}",
            now_nanos()
        ));
        fs::create_dir_all(recent_stage.join("nested"))
            .expect("Failed to create recent staged delete directory");
        fs::write(recent_stage.join("nested").join("note.md"), "recent")
            .expect("Failed to populate recent staged delete directory");

        let _ = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));

        assert!(
            recent_stage.exists(),
            "Expected recent staged delete directory to remain for in-flight safety"
        );
    }

    #[test]
    fn search_notes_finds_matches_in_path_label_and_content() {
        let notebook_dir = TestNotebookDir::new("search_notes");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "work/todo",
            &mut notes,
        ))
        .expect("Failed to create work/todo");
        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "ideas/brainstorm",
            &mut notes,
        ))
        .expect("Failed to create ideas/brainstorm");

        if let Some(first_note) = notes.iter_mut().find(|note| note.rel_path == "work/todo") {
            first_note.labels.push("urgent".to_string());
        }

        fs::write(
            Path::new(notebook_dir.as_str())
                .join("ideas/brainstorm")
                .join("note.md"),
            "Need to build an indexing strategy for search results.",
        )
        .expect("Failed to write brainstorm note content");

        let path_results = block_on(notebook::search_notes(
            notebook_dir.as_str().to_string(),
            notes.clone(),
            "work".to_string(),
        ));
        assert_eq!(path_results.len(), 1);
        assert_eq!(path_results[0].rel_path, "work/todo");
        assert_eq!(path_results[0].snippet, "Path match");

        let label_results = block_on(notebook::search_notes(
            notebook_dir.as_str().to_string(),
            notes.clone(),
            "urgent".to_string(),
        ));
        assert_eq!(label_results.len(), 1);
        assert_eq!(label_results[0].rel_path, "work/todo");
        assert!(
            label_results[0].snippet.contains("Label match"),
            "Expected snippet to indicate label match"
        );

        let content_results = block_on(notebook::search_notes(
            notebook_dir.as_str().to_string(),
            notes.clone(),
            "indexing".to_string(),
        ));
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

    #[test]
    fn move_folder_updates_nested_note_paths() {
        let notebook_dir = TestNotebookDir::new("move_folder");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "folder/note_a",
            &mut notes,
        ))
        .expect("Failed to create folder/note_a");
        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "folder/sub/note_b",
            &mut notes,
        ))
        .expect("Failed to create folder/sub/note_b");

        let moved_to = block_on(notebook::move_note(
            notebook_dir.as_str(),
            "folder",
            "renamed",
            &mut notes,
        ))
        .expect("move_note for folder should succeed");

        assert_eq!(moved_to, "renamed");
        assert_note_md_not_exists(&notebook_dir, "folder/note_a");
        assert_note_md_not_exists(&notebook_dir, "folder/sub/note_b");
        assert_note_md_exists(&notebook_dir, "renamed/note_a");
        assert_note_md_exists(&notebook_dir, "renamed/sub/note_b");

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

    #[test]
    fn load_notes_metadata_recovers_from_backup_when_primary_is_corrupted() {
        let notebook_dir = TestNotebookDir::new("metadata_recovery_from_backup");

        fs::write(
            Path::new(notebook_dir.as_str()).join("metadata.json.bak"),
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
        fs::write(
            Path::new(notebook_dir.as_str()).join("metadata.json"),
            "{ corrupt_primary_json ",
        )
        .expect("Failed to write corrupted metadata.json fixture");

        let load_result = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ))
        .expect("Expected metadata load to recover from backup");

        assert_eq!(load_result.notes.len(), 1);
        assert_eq!(load_result.notes[0].rel_path, "recovered/note");
        assert!(
            load_result.warning.is_some(),
            "Recovery path should surface a warning"
        );

        let restored_primary =
            fs::read_to_string(Path::new(notebook_dir.as_str()).join("metadata.json"))
                .expect("Expected metadata.json to be restored from backup");
        assert!(
            restored_primary.contains("recovered/note"),
            "Primary metadata should be restored from backup contents"
        );
    }

    #[test]
    fn save_metadata_keeps_last_known_good_copy_and_preserves_primary_when_atomic_rename_fails() {
        let notebook_dir = TestNotebookDir::new("metadata_backup_and_atomic_failure");
        let initial_notes = vec![NoteMetadata {
            rel_path: "stable/note".to_string(),
            labels: vec!["v1".to_string()],
            last_updated: Some("2024-01-01T00:00:00Z".to_string()),
        }];
        notebook::save_metadata(notebook_dir.as_str(), &initial_notes)
            .expect("Failed to save initial metadata");

        fs::write(
            Path::new(notebook_dir.as_str()).join(".cognate_fail_atomic_rename"),
            "fail",
        )
        .expect("Failed to create atomic-rename failure marker");

        let updated_notes = vec![NoteMetadata {
            rel_path: "stable/note".to_string(),
            labels: vec!["v2".to_string()],
            last_updated: Some("2024-01-02T00:00:00Z".to_string()),
        }];
        let save_result = notebook::save_metadata(notebook_dir.as_str(), &updated_notes);

        assert!(
            save_result.is_err(),
            "Expected save to fail on simulated rename error"
        );

        let primary_after_failure =
            fs::read_to_string(Path::new(notebook_dir.as_str()).join("metadata.json"))
                .expect("Failed to read metadata.json after simulated rename failure");
        assert!(
            primary_after_failure.contains("\"v1\""),
            "Atomic save should keep the previous metadata.json when rename fails"
        );
        assert!(
            !primary_after_failure.contains("\"v2\""),
            "Failed atomic save must not partially apply new metadata"
        );

        let backup_after_failure =
            fs::read_to_string(Path::new(notebook_dir.as_str()).join("metadata.json.bak"))
                .expect("Failed to read metadata.json.bak after simulated rename failure");
        assert!(
            backup_after_failure.contains("\"v1\""),
            "Backup should preserve the last known-good metadata snapshot"
        );
    }

    #[test]
    fn delete_note_surfaces_failed_rollback_when_rollback_rename_fails() {
        let notebook_dir = TestNotebookDir::new("delete_rollback_failure_surface");
        let mut notes: Vec<NoteMetadata> = vec![NoteMetadata {
            rel_path: "rollback/failure".to_string(),
            labels: Vec::new(),
            last_updated: None,
        }];

        let note_dir = Path::new(notebook_dir.as_str()).join("rollback/failure");
        fs::create_dir_all(&note_dir).expect("Failed to create rollback target note directory");
        fs::write(note_dir.join("note.md"), "rollback failure")
            .expect("Failed to write rollback failure note");
        fs::create_dir(Path::new(notebook_dir.as_str()).join("metadata.json"))
            .expect("Failed to create metadata trap directory");
        fs::write(
            Path::new(notebook_dir.as_str()).join(".cognate_fail_delete_rollback"),
            "fail",
        )
        .expect("Failed to create delete-rollback failure marker");

        let delete_result = block_on(notebook::delete_note(
            notebook_dir.as_str(),
            "rollback/failure",
            &mut notes,
        ));

        assert!(delete_result.is_err());
        let error = delete_result.expect_err("Expected delete to fail");
        assert_eq!(error.kind(), NotebookErrorKind::Recovery);
        assert!(
            error
                .to_string()
                .contains("Rollback failed while restoring filesystem state"),
            "Expected explicit rollback failure message, got: {}",
            error
        );
    }
}
