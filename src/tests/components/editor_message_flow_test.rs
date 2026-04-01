#[cfg(test)]
mod tests {
    use crate::components::editor::note_coordinator;
    use crate::components::editor::{Editor, Message as EditorMessage};
    use crate::components::note_explorer;
    use crate::configuration::Configuration;
    use crate::notebook::{
        self, MetadataLoadResult, NoteMetadata, NoteSearchResult, NotebookError,
    };
    use iced::widget::text_editor::{Action, Edit};
    use iced::window;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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
                "cognate_editor_flow_{}_{}_{}",
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

    fn seed_note(
        notebook_dir: &TestNotebookDir,
        rel_path: &str,
        content: &str,
    ) -> Vec<NoteMetadata> {
        let note_dir = Path::new(notebook_dir.as_str()).join(rel_path);
        fs::create_dir_all(&note_dir).expect("Failed to create note directory");
        fs::write(note_dir.join("note.md"), content).expect("Failed to seed note content");

        let notes = vec![NoteMetadata {
            rel_path: rel_path.to_string(),
            labels: vec!["seed".to_string()],
            last_updated: Some("2024-01-01T00:00:00Z".to_string()),
        }];

        notebook::save_metadata(notebook_dir.as_str(), &notes).expect("Failed to seed metadata");
        notes
    }

    fn create_editor_with_notebook(notebook_path: &str) -> Editor {
        let cfg = Configuration {
            theme: "Dark".to_string(),
            notebook_path: notebook_path.to_string(),
            scale: 1.0,
            config_path: "config.json".to_string(),
            version: "test".to_string(),
        };
        let (editor, _initial_task) = Editor::create(cfg);
        editor
    }

    fn load_and_select_note(
        editor: &mut Editor,
        notes: Vec<NoteMetadata>,
        rel_path: &str,
        content: &str,
    ) {
        let _ = Editor::update(
            editor,
            EditorMessage::NoteExplorerMsg(note_explorer::Message::NotesLoaded(Ok(
                MetadataLoadResult {
                    notes,
                    warning: None,
                },
            ))),
        );
        let _ = Editor::update(editor, EditorMessage::NoteSelected(rel_path.to_string()));
        let _ = Editor::update(
            editor,
            EditorMessage::LoadedNoteContent(
                rel_path.to_string(),
                content.to_string(),
                HashMap::new(),
            ),
        );
    }

    #[test]
    fn startup_load_error_keeps_editor_unselected_and_empty() {
        let notebook_dir = TestNotebookDir::new("startup_error");
        let mut editor = create_editor_with_notebook(notebook_dir.as_str());

        let _ = Editor::update(
            &mut editor,
            EditorMessage::NoteExplorerMsg(note_explorer::Message::NotesLoaded(Err(
                NotebookError::recovery("load metadata", "Corrupted notebook metadata"),
            ))),
        );

        assert_eq!(editor.debug_selected_note_path(), None);
        assert_eq!(editor.debug_markdown_text(), "");
    }

    #[test]
    fn stale_search_results_are_ignored_when_newer_query_exists() {
        let mut editor = Editor::default();

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchQueryChanged("alpha".to_string()),
        );
        let (alpha_generation, _, _) = editor.debug_search_state();

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchQueryChanged("beta".to_string()),
        );
        let (beta_generation, query, initial_results) = editor.debug_search_state();

        assert!(beta_generation > alpha_generation);
        assert_eq!(query, "beta");
        assert!(initial_results.is_empty());

        let stale_result = NoteSearchResult {
            rel_path: "alpha/note".to_string(),
            snippet: "Path match".to_string(),
        };
        let fresh_result = NoteSearchResult {
            rel_path: "beta/note".to_string(),
            snippet: "Path match".to_string(),
        };

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchCompleted(alpha_generation, vec![stale_result]),
        );
        let (_, _, stale_applied_results) = editor.debug_search_state();
        assert!(
            stale_applied_results.is_empty(),
            "stale search completions should not overwrite newer queries"
        );

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchCompleted(beta_generation, vec![fresh_result.clone()]),
        );
        let (_, _, final_results) = editor.debug_search_state();
        assert_eq!(final_results, vec![fresh_result]);
    }

    #[test]
    fn clear_search_invalidates_in_flight_results() {
        let mut editor = Editor::default();

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchQueryChanged("alpha".to_string()),
        );
        let (generation, _, _) = editor.debug_search_state();

        let _ = Editor::update(&mut editor, EditorMessage::ClearSearch);
        let (cleared_generation, query, results) = editor.debug_search_state();
        assert!(cleared_generation > generation);
        assert!(query.is_empty());
        assert!(results.is_empty());

        let _ = Editor::update(
            &mut editor,
            EditorMessage::SearchCompleted(
                generation,
                vec![NoteSearchResult {
                    rel_path: "alpha/note".to_string(),
                    snippet: "Path match".to_string(),
                }],
            ),
        );

        let (_, final_query, final_results) = editor.debug_search_state();
        assert!(final_query.is_empty());
        assert!(
            final_results.is_empty(),
            "clearing search should keep stale in-flight results from reappearing"
        );
    }

    #[test]
    fn debounce_message_flow_reschedules_while_previous_save_is_in_flight() {
        let notebook_dir = TestNotebookDir::new("debounce_flow");
        let notes = seed_note(&notebook_dir, "flow/note", "hello");
        let mut editor = create_editor_with_notebook(notebook_dir.as_str());
        load_and_select_note(&mut editor, notes, "flow/note", "hello");

        let _ = Editor::update(
            &mut editor,
            EditorMessage::EditorAction(Action::Edit(Edit::Insert('!'))),
        );

        let (generation, in_flight, reschedule) = editor.debug_metadata_state();
        assert!(generation >= 1);
        assert!(!in_flight);
        assert!(!reschedule);

        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveElapsed(generation),
        );
        let (_, in_flight_after_first_elapsed, reschedule_after_first_elapsed) =
            editor.debug_metadata_state();
        assert!(in_flight_after_first_elapsed);
        assert!(!reschedule_after_first_elapsed);

        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveElapsed(generation),
        );
        let (_, in_flight_after_second_elapsed, reschedule_after_second_elapsed) =
            editor.debug_metadata_state();
        assert!(in_flight_after_second_elapsed);
        assert!(reschedule_after_second_elapsed);

        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveCompleted(generation, Ok(())),
        );
        let (_, in_flight_after_first_completed, reschedule_after_first_completed) =
            editor.debug_metadata_state();
        assert!(in_flight_after_first_completed);
        assert!(!reschedule_after_first_completed);

        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveCompleted(generation, Ok(())),
        );
        let (_, in_flight_after_second_completed, reschedule_after_second_completed) =
            editor.debug_metadata_state();
        assert!(!in_flight_after_second_completed);
        assert!(!reschedule_after_second_completed);
    }

    #[test]
    fn gui_smoke_open_edit_save_and_close_flushes_note_content() {
        let notebook_dir = TestNotebookDir::new("gui_smoke");
        let notes = seed_note(&notebook_dir, "flow/note", "hello");
        let mut editor = create_editor_with_notebook(notebook_dir.as_str());
        load_and_select_note(&mut editor, notes, "flow/note", "hello");

        assert_eq!(
            editor.debug_selected_note_path().as_deref(),
            Some("flow/note")
        );

        let _ = Editor::update(
            &mut editor,
            EditorMessage::EditorAction(Action::Edit(Edit::Insert('!'))),
        );
        let edited_markdown = editor.debug_markdown_text();
        assert!(
            edited_markdown == "!hello" || edited_markdown == "hello!",
            "Expected smoke edit to insert one character, got: {}",
            edited_markdown
        );
        assert!(
            editor.debug_last_updated_for("flow/note").is_some(),
            "Expected edit flow to update last_updated before flush"
        );

        let (generation, _, _) = editor.debug_metadata_state();
        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveElapsed(generation),
        );
        let _ = Editor::update(
            &mut editor,
            EditorMessage::DebouncedMetadataSaveCompleted(generation, Ok(())),
        );

        let window_id = window::Id::unique();
        let _ = Editor::update(&mut editor, EditorMessage::WindowCloseRequested(window_id));
        assert!(
            editor.debug_shutdown_in_progress(),
            "Close request should start shutdown flush flow"
        );

        let (notebook_path, content_note_path, markdown_text, notes) =
            editor.debug_shutdown_payload();
        let flush_result = note_coordinator::flush_for_shutdown(
            &notebook_path,
            content_note_path,
            &markdown_text,
            &notes,
        );
        let _ = Editor::update(
            &mut editor,
            EditorMessage::ShutdownFlushCompleted(window_id, flush_result),
        );
        assert!(
            !editor.debug_shutdown_in_progress(),
            "Shutdown completion should clear in-progress state"
        );

        let saved_content = fs::read_to_string(
            Path::new(notebook_dir.as_str())
                .join("flow/note")
                .join("note.md"),
        )
        .expect("Expected note content to exist after shutdown flush");
        assert_eq!(saved_content, edited_markdown);
    }

    #[test]
    fn shutdown_flush_error_completion_resets_in_progress_state() {
        let mut editor = Editor::default();
        let window_id = window::Id::unique();

        let _ = Editor::update(&mut editor, EditorMessage::WindowCloseRequested(window_id));
        assert!(editor.debug_shutdown_in_progress());

        let _ = Editor::update(
            &mut editor,
            EditorMessage::ShutdownFlushCompleted(
                window_id,
                Err(NotebookError::storage(
                    "shutdown flush",
                    "simulated persistence failure",
                )),
            ),
        );
        assert!(!editor.debug_shutdown_in_progress());
    }
}
