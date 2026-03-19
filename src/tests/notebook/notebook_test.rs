#[cfg(test)]
mod tests {
    use crate::notebook::{self, NoteMetadata};
    use base64::Engine;
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));
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
        assert!(
            result
                .expect_err("expected error")
                .contains("Invalid relative path"),
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));
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
        assert!(
            result
                .expect_err("expected error")
                .contains("Invalid current relative path"),
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
    fn save_note_content_creates_parent_directories_and_persists_text() {
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));
        assert_eq!(loaded.len(), 1);
        assert_ne!(
            loaded[0].last_updated.as_deref(),
            Some("2000-01-01T00:00:00Z"),
            "save_note_content should refresh last_updated metadata"
        );
        assert!(
            !loaded[0]
                .last_updated
                .as_deref()
                .unwrap_or("")
                .contains('.'),
            "last_updated should not include subsecond precision"
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));
        assert_eq!(
            loaded[0].last_updated.as_deref(),
            Some("2000-01-01T00:00:00Z"),
            "last_updated should not change when content is unchanged"
        );
    }

    #[test]
    fn load_notes_metadata_returns_empty_for_invalid_json() {
        let notebook_dir = TestNotebookDir::new("invalid_metadata");
        fs::write(
            Path::new(notebook_dir.as_str()).join("metadata.json"),
            "{ not_valid_json ",
        )
        .expect("Failed to write invalid metadata");

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));

        assert!(loaded.is_empty());
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

        let loaded = block_on(notebook::load_notes_metadata(
            notebook_dir.as_str().to_string(),
        ));

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
    fn export_note_embedded_images_writes_files_with_detected_extensions() {
        let notebook_dir = TestNotebookDir::new("export_embedded_images");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "with/images",
            &mut notes,
        ))
        .expect("Failed to create note for image export");

        let png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let jpg_bytes = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00];

        let mut images = std::collections::HashMap::new();
        images.insert(
            "img_one".to_string(),
            base64::engine::general_purpose::STANDARD.encode(png_bytes),
        );
        images.insert(
            "img/two".to_string(),
            base64::engine::general_purpose::STANDARD.encode(jpg_bytes),
        );

        let summary = block_on(notebook::export_note_embedded_images(
            notebook_dir.as_str().to_string(),
            "with/images".to_string(),
            images,
        ))
        .expect("export_note_embedded_images should succeed");

        assert_eq!(summary.exported_count, 2);
        assert_eq!(summary.skipped_count, 0);

        let export_dir = Path::new(&summary.export_dir);
        assert!(export_dir.exists(), "Expected export directory to exist");
        assert!(
            export_dir.join("img_one.png").exists(),
            "Expected PNG image export file"
        );
        assert!(
            export_dir.join("img_two.jpg").exists(),
            "Expected JPEG image export file with sanitized id"
        );
    }

    #[test]
    fn export_note_embedded_images_returns_error_for_empty_store() {
        let notebook_dir = TestNotebookDir::new("export_embedded_images_empty");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "empty/images",
            &mut notes,
        ))
        .expect("Failed to create note for empty-export test");

        let result = block_on(notebook::export_note_embedded_images(
            notebook_dir.as_str().to_string(),
            "empty/images".to_string(),
            std::collections::HashMap::new(),
        ));

        assert!(result.is_err());
        assert!(
            result
                .expect_err("expected empty-store export to fail")
                .contains("does not contain embedded images"),
            "Expected a helpful error for empty embedded-image store"
        );
    }

    #[test]
    fn export_note_markdown_with_attachments_rewrites_embedded_image_tags() {
        let notebook_dir = TestNotebookDir::new("export_markdown_with_attachments");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "docs/note",
            &mut notes,
        ))
        .expect("Failed to create note for markdown export");

        let markdown = "before ![image:img_one] and ![image:img/two] and ![image:missing] after";
        let png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
        let jpg_bytes = vec![0xFF, 0xD8, 0xFF, 0xDB, 0x00];

        let mut images = std::collections::HashMap::new();
        images.insert(
            "img_one".to_string(),
            base64::engine::general_purpose::STANDARD.encode(png_bytes),
        );
        images.insert(
            "img/two".to_string(),
            base64::engine::general_purpose::STANDARD.encode(jpg_bytes),
        );

        let summary = block_on(notebook::export_note_markdown_with_attachments(
            notebook_dir.as_str().to_string(),
            "docs/note".to_string(),
            markdown.to_string(),
            images,
        ))
        .expect("export_note_markdown_with_attachments should succeed");

        assert_eq!(summary.exported_count, 2);
        assert_eq!(summary.skipped_count, 0);
        assert_eq!(summary.rewritten_reference_count, 2);

        let markdown_path = Path::new(&summary.markdown_path);
        assert!(
            markdown_path.exists(),
            "Expected exported markdown file to exist"
        );

        let exported_markdown =
            fs::read_to_string(markdown_path).expect("Failed to read exported markdown");
        assert!(
            exported_markdown.contains("![image:img_one](images/img_one.png)"),
            "Expected exported markdown to rewrite first embedded image tag"
        );
        assert!(
            exported_markdown.contains("![image:img/two](images/img_two.jpg)"),
            "Expected exported markdown to rewrite second embedded image tag with sanitized file name"
        );
        assert!(
            exported_markdown.contains("![image:missing]"),
            "Expected unresolved embedded image tag to remain unchanged"
        );

        let attachments_dir = Path::new(&summary.attachments_dir);
        assert!(
            attachments_dir.exists(),
            "Expected attachments directory to be created"
        );
        assert!(
            attachments_dir.join("img_one.png").exists(),
            "Expected first image attachment file to exist"
        );
        assert!(
            attachments_dir.join("img_two.jpg").exists(),
            "Expected second image attachment file to exist"
        );
    }

    #[test]
    fn export_note_markdown_with_attachments_exports_markdown_even_without_images() {
        let notebook_dir = TestNotebookDir::new("export_markdown_without_images");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "docs/plain",
            &mut notes,
        ))
        .expect("Failed to create note for markdown-only export");

        let markdown = "# Plain Note\n\nNo embedded images here.";

        let summary = block_on(notebook::export_note_markdown_with_attachments(
            notebook_dir.as_str().to_string(),
            "docs/plain".to_string(),
            markdown.to_string(),
            std::collections::HashMap::new(),
        ))
        .expect("markdown-only export should succeed");

        assert_eq!(summary.exported_count, 0);
        assert_eq!(summary.skipped_count, 0);
        assert_eq!(summary.rewritten_reference_count, 0);

        let exported_markdown = fs::read_to_string(Path::new(&summary.markdown_path))
            .expect("Failed to read exported markdown file");
        assert_eq!(exported_markdown, markdown);
    }

    #[test]
    fn export_note_markdown_with_attachments_keeps_existing_markdown_image_links() {
        let notebook_dir = TestNotebookDir::new("export_markdown_preserves_existing_links");
        let mut notes: Vec<NoteMetadata> = Vec::new();

        block_on(notebook::create_new_note(
            notebook_dir.as_str(),
            "docs/existing-link",
            &mut notes,
        ))
        .expect("Failed to create note for existing-link export");

        let markdown = "![image:already](./existing.png)\n![image:embedded_id]";

        let mut images = std::collections::HashMap::new();
        images.insert(
            "embedded_id".to_string(),
            base64::engine::general_purpose::STANDARD
                .encode(vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        );

        let summary = block_on(notebook::export_note_markdown_with_attachments(
            notebook_dir.as_str().to_string(),
            "docs/existing-link".to_string(),
            markdown.to_string(),
            images,
        ))
        .expect("export should succeed");

        assert_eq!(summary.rewritten_reference_count, 1);

        let exported_markdown = fs::read_to_string(Path::new(&summary.markdown_path))
            .expect("Failed to read exported markdown");
        assert!(
            exported_markdown.contains("![image:already](./existing.png)"),
            "Expected existing markdown image links to remain unchanged"
        );
        assert!(
            exported_markdown.contains("![image:embedded_id](images/embedded_id.png)"),
            "Expected embedded image placeholders to be rewritten"
        );
    }
}
