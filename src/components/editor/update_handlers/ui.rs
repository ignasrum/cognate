use super::*;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::InitiateFolderRename(folder_path) => {
            state.state.show_rename_folder_dialog(folder_path);
            Task::none()
        }
        Message::AboutButtonClicked => {
            state.state.toggle_about_info();
            Task::none()
        }
        Message::IncreaseScale => {
            let new_scale = round_scale_step((state.state.ui_scale() + 0.1).min(4.0));
            state.state.set_ui_scale(new_scale);
            state.persist_scale_task()
        }
        Message::DecreaseScale => {
            let new_scale = round_scale_step((state.state.ui_scale() - 0.1).max(0.5));
            state.state.set_ui_scale(new_scale);
            state.persist_scale_task()
        }
        Message::MarkdownLinkClicked(_uri) => {
            #[cfg(debug_assertions)]
            eprintln!("Markdown link clicked: {}", _uri);
            Task::none()
        }
        _ => unreachable!("ui handler received invalid message"),
    }
}
