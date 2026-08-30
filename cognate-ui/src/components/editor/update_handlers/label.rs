use super::*;
use crate::components::editor::actions::label_actions;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::NewLabelInputChanged(text) => {
            label_actions::handle_label_input_changed(&mut state.state, text);
            Task::none()
        }
        Message::AddLabel => label_actions::handle_add_label(
            &mut state.state,
            &mut state.note_explorer,
            &mut state.visualizer,
        ),
        Message::RemoveLabel(label) => label_actions::handle_remove_label(
            &mut state.state,
            &mut state.note_explorer,
            &mut state.visualizer,
            label,
        ),
        _ => unreachable!("label handler received invalid message"),
    }
}
