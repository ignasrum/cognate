#[cfg(test)]
mod tests {
    use crate::components::editor::actions::label_actions;
    use crate::components::editor::state::editor_state::EditorState;
    use crate::components::note_explorer::NoteExplorer;
    use crate::components::visualizer::Visualizer;
    use crate::notebook::NoteMetadata;

    fn setup() -> (EditorState, NoteExplorer, Visualizer) {
        let mut state = EditorState::new();
        state.set_notebook_path("dummy".to_string());
        state.set_selected_note_path(Some("note/a".to_string()));
        state.set_selected_note_labels(vec![]);

        let mut explorer = NoteExplorer::new("dummy".to_string());
        explorer.notes = vec![NoteMetadata {
            rel_path: "note/a".to_string(),
            labels: vec![],
        }];

        let visualizer = Visualizer::new();
        (state, explorer, visualizer)
    }

    #[test]
    fn add_label_updates_state_and_note_metadata() {
        let (mut state, mut explorer, mut visualizer) = setup();
        state.set_new_label_text("tag1".to_string());

        let _ = label_actions::handle_add_label(&mut state, &mut explorer, &mut visualizer);

        assert_eq!(state.selected_note_labels(), &["tag1".to_string()]);
        assert_eq!(state.new_label_text(), "");
        assert_eq!(explorer.notes[0].labels, vec!["tag1".to_string()]);

        state.set_new_label_text("tag1".to_string());
        let _ = label_actions::handle_add_label(&mut state, &mut explorer, &mut visualizer);
        assert_eq!(explorer.notes[0].labels, vec!["tag1".to_string()]);
    }

    #[test]
    fn remove_label_updates_state_and_note_metadata() {
        let (mut state, mut explorer, mut visualizer) = setup();
        state.set_selected_note_labels(vec!["tag1".to_string(), "tag2".to_string()]);
        explorer.notes[0].labels = vec!["tag1".to_string(), "tag2".to_string()];

        let _ = label_actions::handle_remove_label(
            &mut state,
            &mut explorer,
            &mut visualizer,
            "tag1".to_string(),
        );

        assert_eq!(state.selected_note_labels(), &["tag2".to_string()]);
        assert_eq!(explorer.notes[0].labels, vec!["tag2".to_string()]);
    }

    #[test]
    fn label_input_ignored_while_about_dialog_open() {
        let (mut state, _explorer, _visualizer) = setup();
        state.set_show_about_info(true);

        label_actions::handle_label_input_changed(&mut state, "blocked".to_string());
        assert_eq!(state.new_label_text(), "");
    }
}
