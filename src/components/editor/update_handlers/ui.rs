use super::*;
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
