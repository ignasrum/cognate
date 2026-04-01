use iced::task::Task;

use super::*;

impl Editor {
    pub(super) fn handle_selection_messages(state: &mut Self, message: Message) -> Task<Message> {
        let previous_markdown = state.markdown_text.clone();
        let task = match message {
            Message::NoteExplorerMsg(note_explorer_message) => {
                note_actions::handle_note_explorer_message(
                    &mut state.note_explorer,
                    &mut state.visualizer,
                    &mut state.state,
                    &mut state.content,
                    &mut state.markdown_text,
                    note_explorer_message,
                )
            }
            Message::NoteSelected(note_path) => note_actions::handle_note_selected(
                &mut state.note_explorer,
                &mut state.undo_manager,
                &mut state.state,
                note_path,
            ),
            _ => unreachable!("selection handler received invalid message"),
        };

        if state.markdown_text != previous_markdown {
            if state.state.selected_note_path().is_none() {
                state.content_note_path = None;
            }
            state.prune_embedded_images_for_current_markdown();
            state.sync_markdown_preview();
            return Task::batch(vec![task, state.scroll_preview_to_cursor_task()]);
        }

        task
    }
}
