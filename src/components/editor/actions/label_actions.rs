use iced::task::Task; // Use Task instead of Command

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::note_explorer::NoteExplorer;
use crate::components::visualizer;
use crate::components::visualizer::Visualizer;
use crate::notebook;

// Handle label input changed
pub fn handle_label_input_changed(state: &mut EditorState, text: String) {
    if !state.show_about_info() {
        state.set_new_label_text(text);
    }
}

// Handle add label
pub fn handle_add_label(
    state: &mut EditorState,
    note_explorer: &mut NoteExplorer,
    visualizer: &mut Visualizer,
) -> Task<Message> {
    if !state.show_about_info() {
        if let Some(selected_path) = state.selected_note_path().cloned() {
            let label = state.new_label_text().trim().to_string();
            let mut selected_labels = state.selected_note_labels().to_vec();
            
            if !label.is_empty() && !selected_labels.contains(&label) {
                selected_labels.push(label.clone());
                state.set_selected_note_labels(selected_labels);

                if let Some(note) = note_explorer
                    .notes
                    .iter_mut()
                    .find(|n| n.rel_path == selected_path)
                {
                    note.labels.push(label);
                }

                let _ = visualizer.update(visualizer::Message::UpdateNotes(
                    note_explorer.notes.clone(),
                ));

                state.clear_new_label_text();

                let notebook_path = state.notebook_path().to_string();
                let notes_to_save = note_explorer.notes.clone();
                return Task::perform(
                    async move {
                        notebook::save_metadata(&notebook_path, &notes_to_save[..])
                            .map_err(|e| e.to_string())
                    },
                    Message::MetadataSaved,
                );
            }
        }
    }
    Task::none()
}

// Handle remove label
pub fn handle_remove_label(
    state: &mut EditorState,
    note_explorer: &mut NoteExplorer,
    visualizer: &mut Visualizer,
    label_to_remove: String,
) -> Task<Message> {
    if let Some(selected_path) = state.selected_note_path().cloned() {
        if !state.show_about_info() {
            let mut selected_labels = state.selected_note_labels().to_vec();
            selected_labels.retain(|label| label != &label_to_remove);
            state.set_selected_note_labels(selected_labels);

            if let Some(note) = note_explorer
                .notes
                .iter_mut()
                .find(|n| n.rel_path == selected_path)
            {
                note.labels.retain(|label| label != &label_to_remove);
            }

            let _ = visualizer.update(visualizer::Message::UpdateNotes(
                note_explorer.notes.clone(),
            ));

            let notebook_path = state.notebook_path().to_string();
            let notes_to_save = note_explorer.notes.clone();
            return Task::perform(
                async move {
                    notebook::save_metadata(&notebook_path, &notes_to_save[..])
                        .map_err(|e| e.to_string())
                },
                Message::MetadataSaved,
            );
        }
    }
    Task::none()
}
