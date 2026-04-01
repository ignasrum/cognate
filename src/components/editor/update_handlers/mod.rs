use super::*;

mod label;
mod note_lifecycle;
mod persistence;
mod search;
mod ui;
mod visualizer;

impl Editor {
    pub(super) fn handle_label_messages(state: &mut Self, message: Message) -> Task<Message> {
        label::handle(state, message)
    }

    pub(super) fn handle_search_messages(state: &mut Self, message: Message) -> Task<Message> {
        search::handle(state, message)
    }

    pub(super) fn handle_debounced_metadata_messages(
        state: &mut Self,
        message: Message,
    ) -> Task<Message> {
        persistence::handle_debounced_metadata(state, message)
    }

    pub(super) fn handle_shutdown_messages(state: &mut Self, message: Message) -> Task<Message> {
        persistence::handle_shutdown(state, message)
    }

    pub(super) fn handle_save_feedback_messages(message: Message) -> Task<Message> {
        persistence::handle_save_feedback(message)
    }

    pub(super) fn handle_visualizer_messages(state: &mut Self, message: Message) -> Task<Message> {
        visualizer::handle(state, message)
    }

    pub(super) fn handle_note_lifecycle_messages(
        state: &mut Self,
        message: Message,
    ) -> Task<Message> {
        note_lifecycle::handle(state, message)
    }

    pub(super) fn handle_ui_messages(state: &mut Self, message: Message) -> Task<Message> {
        ui::handle(state, message)
    }
}
