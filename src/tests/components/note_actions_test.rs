#[cfg(test)]
mod tests {
    use crate::components::editor::actions::note_actions;
    use crate::components::editor::state::editor_state::EditorState;
    use crate::components::editor::text_management::undo_manager::UndoManager;
    use crate::components::editor::Message as EditorMessage;
    use crate::components::note_explorer;
    use crate::components::note_explorer::NoteExplorer;
    use crate::components::visualizer;
    use crate::components::visualizer::Visualizer;
    use crate::notebook::NoteMetadata;
    use iced::widget::text_editor::Content;

    fn note(path: &str, labels: &[&str]) -> NoteMetadata {
        NoteMetadata {
            rel_path: path.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn setup_state_with_notebook() -> EditorState {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy_notebook".to_string());
        state
    }

    #[test]
    fn get_select_note_command_handles_selected_and_first_note() {
        let notes = vec![note("a", &[]), note("b", &[])];
        let selected = "b".to_string();

        let _ = note_actions::get_select_note_command(Some(&selected), &notes);
        let _ = note_actions::get_select_note_command(None, &notes);
        let _ = note_actions::get_select_note_command(None, &[]);
    }

    #[test]
    fn create_note_handles_empty_and_valid_inputs() {
        let mut state = setup_state_with_notebook();
        let current_notes = vec![note("a", &[])];

        let _ = note_actions::handle_create_note(&mut state, current_notes.clone());

        state.show_new_note_dialog();
        state.update_new_note_path("".to_string());
        let _ = note_actions::handle_create_note(&mut state, current_notes.clone());

        state.show_new_note_dialog();
        state.update_new_note_path("folder/new_note".to_string());
        let _ = note_actions::handle_create_note(&mut state, current_notes);
        assert!(!state.show_new_note_input());
    }

    #[test]
    fn note_created_and_deleted_handlers_cover_success_and_error_paths() {
        let mut explorer = NoteExplorer::new("dummy".to_string());
        let _ = note_actions::handle_note_created(Ok(note("created/path", &[])), &mut explorer);
        let _ = note_actions::handle_note_created(Err("create failed".to_string()), &mut explorer);

        let mut state = setup_state_with_notebook();
        let mut content = Content::with_text("hello");
        let mut markdown = "hello".to_string();
        let mut undo = UndoManager::new();

        state.set_selected_note_path(Some("a".to_string()));
        undo.initialize_history("a");
        undo.add_to_history("a", "snapshot".to_string());

        let _ = note_actions::handle_note_deleted(
            Ok(()),
            &mut state,
            &mut content,
            &mut markdown,
            &mut undo,
            &mut explorer,
        );
        assert_eq!(state.selected_note_path(), None);
        assert!(markdown.is_empty());

        let _ = note_actions::handle_note_deleted(
            Err("delete failed".to_string()),
            &mut state,
            &mut content,
            &mut markdown,
            &mut undo,
            &mut explorer,
        );
    }

    #[test]
    fn delete_flow_handlers_cover_confirmation_branches() {
        let mut state = setup_state_with_notebook();
        let current_notes = vec![note("a", &[])];

        let _ = note_actions::handle_delete_note(&mut state);

        state.set_show_about_info(true);
        state.set_selected_note_path(Some("a".to_string()));
        let _ = note_actions::handle_delete_note(&mut state);

        state.set_show_about_info(false);
        let _ = note_actions::handle_delete_note(&mut state);

        let _ = note_actions::handle_confirm_delete_note(false, &mut state, current_notes.clone());

        state.set_selected_note_path(Some("a".to_string()));
        let _ = note_actions::handle_confirm_delete_note(true, &mut state, current_notes.clone());
        assert_eq!(state.selected_note_path(), None);

        let _ = note_actions::handle_confirm_delete_note(true, &mut state, current_notes);
    }

    #[test]
    fn move_flow_handlers_cover_key_paths() {
        let mut state = setup_state_with_notebook();
        let notes = vec![note("a", &[]), note("b", &[])];
        let mut explorer = NoteExplorer::new("dummy".to_string());
        let mut undo = UndoManager::new();

        let _ = note_actions::handle_confirm_move_note(&mut state, notes.clone());

        state.show_move_note_dialog("a".to_string());
        state.update_move_note_path("".to_string());
        let _ = note_actions::handle_confirm_move_note(&mut state, notes.clone());

        state.show_move_note_dialog("a".to_string());
        state.update_move_note_path("a".to_string());
        let _ = note_actions::handle_confirm_move_note(&mut state, notes.clone());

        state.show_move_note_dialog("a".to_string());
        state.update_move_note_path("c".to_string());
        let _ = note_actions::handle_confirm_move_note(&mut state, notes.clone());

        undo.initialize_history("a");
        undo.add_to_history("a", "v1".to_string());
        state.show_move_note_dialog("a".to_string());
        let _ = note_actions::handle_note_moved(Ok("c".to_string()), &mut state, &mut undo, &mut explorer);
        assert!(undo.get_previous_content("c").is_some());

        let _ = note_actions::handle_note_moved(
            Err("move failed".to_string()),
            &mut state,
            &mut undo,
            &mut explorer,
        );
    }

    #[test]
    fn explorer_and_visualizer_message_handlers_update_editor_state() {
        let mut state = setup_state_with_notebook();
        let mut explorer = NoteExplorer::new("dummy".to_string());
        let mut visualizer = Visualizer::new();
        let mut undo = UndoManager::new();
        let mut content = Content::with_text("");
        let mut markdown = String::new();

        let loaded_notes = vec![note("folder/note", &["tag"]), note("x", &[])];
        let _ = note_actions::handle_note_explorer_message(
            &mut explorer,
            &mut visualizer,
            &mut state,
            &mut content,
            &mut markdown,
            note_explorer::Message::NotesLoaded(loaded_notes),
        );

        let _ = note_actions::handle_note_selected(
            &mut explorer,
            &mut undo,
            &mut state,
            &mut content,
            &mut markdown,
            "folder/note".to_string(),
        );
        assert_eq!(state.selected_note_path(), Some(&"folder/note".to_string()));

        let _ = note_actions::handle_visualizer_message(
            &mut visualizer,
            &mut explorer,
            &mut state,
            &mut content,
            &mut markdown,
            &mut undo,
            visualizer::Message::ToggleLabel("tag".to_string()),
        );
        let _ = note_actions::handle_visualizer_message(
            &mut visualizer,
            &mut explorer,
            &mut state,
            &mut content,
            &mut markdown,
            &mut undo,
            visualizer::Message::NoteSelectedInVisualizer("x".to_string()),
        );
    }

    #[test]
    fn editor_message_variants_for_note_actions_paths_are_constructible() {
        let _ = EditorMessage::NoteCreated(Ok(note("n", &[])));
        let _ = EditorMessage::NoteDeleted(Ok(()));
        let _ = EditorMessage::NoteMoved(Ok("x".to_string()));
    }
}
