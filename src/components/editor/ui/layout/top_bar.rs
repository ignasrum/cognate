use iced::Length;
use iced::widget::{Container, Row, Space, Text, TextInput as IcedTextInput, button};
use std::collections::HashSet;
use std::path::Path;

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::note_explorer;
use crate::components::visualizer;

pub(super) fn build_top_bar<'a>(
    state: &'a EditorState,
    note_explorer_component: &'a note_explorer::NoteExplorer,
) -> Row<'a, Message> {
    let mut top_bar = Row::new().spacing(10).padding(5).width(Length::Fill);

    let is_dialog_open = state.is_any_dialog_open();

    // Add the about/back button when appropriate
    if !is_dialog_open && !state.show_visualizer() {
        let about_button_text = if state.show_about_info() {
            "Back"
        } else {
            "About"
        };
        top_bar = top_bar.push(
            button(about_button_text)
                .padding(5)
                .on_press(Message::AboutButtonClicked),
        );
    } else if state.show_about_info() {
        top_bar = top_bar.push(
            button("Back")
                .padding(5)
                .on_press(Message::AboutButtonClicked),
        );
    }

    // Add notebook specific buttons
    if !state.notebook_path().is_empty() {
        if !is_dialog_open && !state.show_visualizer() {
            top_bar = top_bar.push(
                button("Show Visualizer")
                    .padding(5)
                    .on_press(Message::ToggleVisualizer),
            );
        } else if state.show_visualizer() && !is_dialog_open {
            let hide_visualizer_action = if let Some(note_path) = state.selected_note_path() {
                Message::VisualizerMsg(visualizer::Message::NoteSelectedInVisualizer(
                    note_path.clone(),
                ))
            } else {
                Message::ToggleVisualizer
            };

            top_bar = top_bar.push(
                button("Hide Visualizer")
                    .padding(5)
                    .on_press(hide_visualizer_action),
            );
        }

        if !state.show_visualizer()
            && !state.show_new_note_input()
            && !state.show_move_note_input()
            && !state.show_about_info()
        {
            top_bar = top_bar.push(button("New Note").padding(5).on_press(Message::NewNote));
            if state.selected_note_path().is_some() {
                top_bar = top_bar.push(
                    button("Delete Note")
                        .padding(5)
                        .on_press(Message::DeleteNote),
                );
                top_bar = top_bar.push(button("Move Note").padding(5).on_press(Message::MoveNote));
            }

            top_bar = top_bar.push(
                IcedTextInput::new("Search notes...", state.search_query())
                    .on_input(Message::SearchQueryChanged)
                    .on_submit(Message::RunSearch)
                    .padding(5)
                    .width(Length::Fixed(240.0)),
            );
            top_bar = top_bar.push(button("Clear").padding(5).on_press(Message::ClearSearch));
        } else if state.show_new_note_input() {
            top_bar = top_bar.push(Text::new("Creating New Note..."));
        } else if state.show_move_note_input() {
            let mut all_folders_in_notes: HashSet<String> = HashSet::new();
            for note in &note_explorer_component.notes {
                if let Some(parent) = Path::new(&note.rel_path).parent() {
                    let folder_path = parent.to_string_lossy().into_owned();
                    if !folder_path.is_empty() && folder_path != "." {
                        all_folders_in_notes.insert(folder_path);
                    }
                }
            }

            let is_renaming_folder = state
                .move_note_current_path()
                .is_some_and(|p| all_folders_in_notes.contains(p));

            let operation_text = if is_renaming_folder {
                "Renaming Folder"
            } else {
                "Moving Note"
            };
            top_bar = top_bar.push(Text::new(format!(
                "{} '{}'...",
                operation_text,
                state.move_note_current_path().unwrap_or(&String::new())
            )));
        }
    } else if !state.show_about_info() {
        top_bar = top_bar.push(Text::new(
            "Please configure the 'notebook_path' in your config.json file to open a notebook.",
        ));
    }

    // Always keep scale controls right-aligned in top bar.
    top_bar = top_bar
        .push(Space::new().width(Length::Fill))
        .push(
            button(
                Container::new(Text::new("-"))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .padding(0)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .on_press(Message::DecreaseScale),
        )
        .push(
            button(
                Container::new(Text::new("+"))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .padding(0)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0))
            .on_press(Message::IncreaseScale),
        );

    top_bar
}
