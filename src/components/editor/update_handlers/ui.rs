use super::*;
use crate::notebook;
use std::process::Command;

fn open_markdown_link(uri: &str) -> Result<(), String> {
    let scheme = uri
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());

    if !matches!(scheme.as_deref(), Some("http" | "https" | "mailto")) {
        return Err(format!("unsupported link scheme: {uri}"));
    }

    let mut command = match std::env::consts::OS {
        "macos" => {
            let mut command = Command::new("open");
            command.arg(uri);
            command
        }
        "windows" => {
            let mut command = Command::new("cmd");
            command.args(["/C", "start", "", uri]);
            command
        }
        _ => {
            let mut command = Command::new("xdg-open");
            command.arg(uri);
            command
        }
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open link {uri}: {error}"))
}

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::InitiateFolderRename(folder_path) => {
            state.state.show_rename_folder_dialog(folder_path);
            Task::none()
        }
        Message::RetryConnection => {
            state
                .state
                .set_connection_error("Checking connection to server...".to_string());
            Task::perform(
                crate::notebook::check_connection(),
                Message::ConnectionChecked,
            )
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
        Message::MarkdownLinkClicked(uri) => {
            Task::perform(async move { open_markdown_link(&uri) }, |result| {
                if let Err(error) = result {
                    eprintln!("{error}");
                }
                Message::Dummy
            })
        }
        Message::ConflictKeepServer => {
            let Some(conflict) = state.state.conflict().cloned() else {
                return Task::none();
            };
            notebook::set_note_revision(&conflict.note_path, &conflict.server_revision);
            state.content = iced::widget::text_editor::Content::with_text(&conflict.server_content);
            state.markdown_text = conflict.server_content;
            state.undo_manager.initialize_history(&conflict.note_path);
            state.state.hide_conflict_dialog();
            state.sync_markdown_preview()
        }
        Message::ConflictRetryLocal => {
            let Some(conflict) = state.state.conflict().cloned() else {
                return Task::none();
            };
            notebook::set_note_revision(&conflict.note_path, &conflict.server_revision);
            state.state.hide_conflict_dialog();
            let notebook_path = state.state.notebook_path().to_string();
            Task::perform(
                async move {
                    notebook::save_note_content(
                        notebook_path,
                        conflict.note_path,
                        conflict.local_content,
                    )
                    .await
                },
                Message::NoteContentSaved,
            )
        }
        Message::ConflictSaveCopy => {
            let Some(conflict) = state.state.conflict().cloned() else {
                return Task::none();
            };
            let notebook_path = state.state.notebook_path().to_string();
            let mut notes = state.note_explorer.notes.clone();
            let mut copy_path = format!("{}.conflict", conflict.note_path);
            let mut suffix = 2;
            while notes.iter().any(|note| note.rel_path == copy_path) {
                copy_path = format!("{}.conflict-{suffix}", conflict.note_path);
                suffix += 1;
            }
            Task::perform(
                async move {
                    notebook::create_new_note(&notebook_path, &copy_path, &mut notes).await?;
                    notebook::save_note_content(
                        notebook_path,
                        copy_path.clone(),
                        conflict.local_content,
                    )
                    .await?;
                    Ok(copy_path)
                },
                Message::ConflictCopySaved,
            )
        }
        Message::ConflictDismiss => {
            state.state.hide_conflict_dialog();
            Task::none()
        }
        Message::Dummy => Task::none(),
        _ => unreachable!("ui handler received invalid message"),
    }
}

#[cfg(test)]
mod tests {
    use super::open_markdown_link;

    #[test]
    fn rejects_unsupported_link_schemes() {
        let result = open_markdown_link("javascript:alert(1)");

        assert!(result.is_err());
    }
}
