#[cfg(test)]
mod tests {
    use crate::components::editor::state::editor_state::EditorState;
    use crate::components::editor::text_management::content_handler;
    use crate::components::editor::text_management::undo_manager::{self, UndoManager};
    use iced::widget::text_editor::{Action, Content, Cursor, Edit, Position};

    fn cursor(line: usize, column: usize) -> Cursor {
        Cursor {
            position: Position { line, column },
            selection: None,
        }
    }

    #[test]
    fn undo_manager_history_and_path_changes_work() {
        let mut undo = UndoManager::new();
        undo.initialize_history("note/a");
        undo.add_to_history("note/a", "v1".to_string(), cursor(0, 1));
        undo.add_to_history("note/a", "v2".to_string(), cursor(0, 2));

        assert_eq!(undo.get_previous_content("note/a"), Some("v2".to_string()));
        assert_eq!(undo.get_previous_content("note/a"), Some("v1".to_string()));
        assert_eq!(undo.get_previous_content("note/a"), None);

        undo.handle_path_change("note/a", "note/b");
        assert_eq!(undo.get_previous_content("note/b"), None);

        undo.add_to_history("note/b", "v3".to_string(), cursor(0, 1));
        undo.remove_history("note/b");
        assert_eq!(undo.get_previous_content("note/b"), None);
    }

    #[test]
    fn undo_manager_initial_content_tracks_external_changes() {
        let mut undo = UndoManager::new();
        undo.initialize_history("note/x");
        undo.handle_initial_content("note/x", "first");
        undo.handle_initial_content("note/x", "changed");

        assert_eq!(
            undo.get_previous_content("note/x"),
            Some("changed".to_string())
        );
        assert_eq!(
            undo.get_previous_content("note/x"),
            Some("first".to_string())
        );
    }

    #[test]
    fn content_handlers_modify_text_and_history() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/a".to_string()));

        let mut content = Content::with_text("");
        let mut markdown = String::new();
        let mut undo = UndoManager::new();
        undo.initialize_history("note/a");

        let _ = content_handler::handle_tab_key(
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert!(markdown.starts_with("    "));

        let _ = content_handler::handle_select_all(&mut content, &state);

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('x')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert!(undo.get_previous_content("note/a").is_some());

        state.set_loading_note(true);
        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/a".to_string(),
            "loaded".to_string(),
        );
        assert!(!state.is_loading_note());
        assert_eq!(markdown, "loaded");

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('!')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_ne!(markdown, "loaded");
        assert!(undo.get_previous_content("note/a").is_some());
    }

    #[test]
    fn loaded_note_content_ignores_stale_results_and_applies_current_selection() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/b".to_string()));
        state.set_loading_note(true);

        let mut content = Content::with_text("existing");
        let mut markdown = "existing".to_string();
        let mut undo = UndoManager::new();
        undo.initialize_history("note/b");

        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/a".to_string(),
            "stale".to_string(),
        );

        assert_eq!(markdown, "existing");
        assert!(state.is_loading_note());

        let _ = content_handler::handle_loaded_note_content(
            &mut content,
            &mut markdown,
            &mut undo,
            &mut state,
            "note/b".to_string(),
            "fresh".to_string(),
        );

        assert_eq!(markdown, "fresh");
        assert!(!state.is_loading_note());
    }

    #[test]
    fn handle_undo_replaces_editor_text_when_history_exists() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.add_to_history("note/u", "previous".to_string(), cursor(0, 1));

        let mut content = Content::with_text("current");
        content.move_to(cursor(0, 3));
        let mut markdown = "current".to_string();
        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "previous");
        let cursor = content.cursor();
        assert_eq!(cursor.position.line, 0);
        assert_eq!(cursor.position.column, 1);
    }

    #[test]
    fn handle_undo_clamps_cursor_for_shorter_previous_content() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.add_to_history("note/u", "x".to_string(), cursor(0, 999));

        let mut content = Content::with_text("current");
        content.move_to(cursor(0, 5));
        let mut markdown = "current".to_string();
        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "x");
        let cursor = content.cursor();
        assert_eq!(cursor.position.line, 0);
        assert_eq!(cursor.position.column, 1);
    }

    #[test]
    fn undo_after_midline_insert_restores_pre_edit_cursor_position() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");

        let mut content = Content::with_text("abcd");
        content.move_to(cursor(0, 2));
        let mut markdown = "abcd".to_string();

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('X')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abXcd");

        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "abcd");
        let cursor = content.cursor();
        assert_eq!(cursor.position.line, 0);
        assert_eq!(cursor.position.column, 2);
    }

    #[test]
    fn last_undo_does_not_jump_cursor_when_text_is_unchanged() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.handle_initial_content("note/u", "abcd");

        let mut content = Content::with_text("abcd");
        content.move_to(cursor(0, 2));
        let mut markdown = "abcd".to_string();

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('X')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abXcd");

        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abcd");
        assert_eq!(content.cursor().position.column, 2);

        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abcd");
        assert_eq!(content.cursor().position.column, 2);
    }

    #[test]
    fn handle_redo_restores_undone_text_and_cursor() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.handle_initial_content("note/u", "abcd");

        let mut content = Content::with_text("abcd");
        content.move_to(cursor(0, 2));
        let mut markdown = "abcd".to_string();

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('X')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abXcd");
        let after_edit_cursor = content.cursor();

        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abcd");

        let _ = undo_manager::handle_redo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "abXcd");
        assert_eq!(content.cursor().position, after_edit_cursor.position);
    }

    #[test]
    fn handle_redo_is_cleared_after_new_edit() {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/u".to_string()));

        let mut undo = UndoManager::new();
        undo.initialize_history("note/u");
        undo.handle_initial_content("note/u", "abcd");

        let mut content = Content::with_text("abcd");
        content.move_to(cursor(0, 2));
        let mut markdown = "abcd".to_string();

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('X')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abXcd");

        let _ = undo_manager::handle_undo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abcd");

        let _ = content_handler::handle_editor_action(
            &mut content,
            &mut markdown,
            &mut undo,
            Action::Edit(Edit::Insert('Y')),
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );
        assert_eq!(markdown, "abYcd");

        let _ = undo_manager::handle_redo(
            &mut undo,
            &mut content,
            &mut markdown,
            state.selected_note_path(),
            state.notebook_path(),
            &state,
        );

        assert_eq!(markdown, "abYcd");
    }
}
