use super::*;
use crate::components::visualizer as visualizer_component;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::ToggleVisualizer => {
            state.state.toggle_visualizer();

            if state.state.show_visualizer() && !state.state.notebook_path().is_empty() {
                state.visualizer.sync_notes(&state.note_explorer.notes);
                let _ = state
                    .visualizer
                    .update(visualizer_component::Message::FocusOnNote(
                        state.state.selected_note_path().cloned(),
                    ));
                Task::none()
            } else {
                note_actions::get_select_note_command(
                    state.state.selected_note_path(),
                    &state.note_explorer.notes,
                )
            }
        }
        Message::VisualizerMsg(visualizer_message) => note_actions::handle_visualizer_message(
            &mut state.visualizer,
            &mut state.note_explorer,
            &mut state.state,
            &mut state.undo_manager,
            visualizer_message,
        ),
        _ => unreachable!("visualizer handler received invalid message"),
    }
}
