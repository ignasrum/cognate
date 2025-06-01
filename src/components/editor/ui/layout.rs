use iced::widget::{Column, Container, Row, Text, text_editor, button};
use iced::{Element, Length};
use std::collections::HashSet;
use std::path::Path;

use crate::components::editor::Message;
use crate::components::note_explorer;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::ui::dialogs;
use crate::components::editor::ui::input_fields;
use crate::components::visualizer;

pub fn generate_layout<'a>(
    state: &'a EditorState,
    content: &'a iced::widget::text_editor::Content,
    note_explorer_component: &'a note_explorer::NoteExplorer,
    visualizer_component: &'a visualizer::Visualizer,
) -> Element<'a, Message> {
    let mut top_bar = Row::new().spacing(10).padding(5).width(Length::Fill);

    let is_dialog_open = state.is_any_dialog_open();

    // Add the about/back button when appropriate
    if !is_dialog_open && !state.show_visualizer() {
        let about_button_text = if state.show_about_info() { "Back" } else { "About" };
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
            let visualizer_button_text = if state.show_visualizer() {
                "Hide Visualizer"
            } else {
                "Show Visualizer"
            };
            top_bar = top_bar.push(
                button(visualizer_button_text)
                    .padding(5)
                    .on_press(Message::ToggleVisualizer),
            );
        } else if state.show_visualizer() && !is_dialog_open {
            top_bar = top_bar.push(
                button("Hide Visualizer")
                    .padding(5)
                    .on_press(Message::ToggleVisualizer),
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

            let is_renaming_folder = state.move_note_current_path()
                .map_or(false, |p| all_folders_in_notes.contains(p));

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
    } else {
        if !state.show_about_info() {
            top_bar = top_bar.push(Text::new(
                "Please configure the 'notebook_path' in your config.json file to open a notebook.",
            ));
        }
    }

    // Main content area
    let main_content: Element<'_, Message> = if state.show_about_info() {
        dialogs::about_dialog(state.app_version())
    } else if state.show_visualizer() {
        Container::new(visualizer_component.view().map(Message::VisualizerMessage))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if state.show_new_note_input() {
        dialogs::new_note_dialog(state.new_note_path_input())
    } else if state.show_move_note_input() {
        let is_folder = state.move_note_current_path()
            .map(|p| state.is_folder_path(p, &note_explorer_component.notes))
            .unwrap_or(false);
            
        dialogs::move_note_dialog(
            state.move_note_current_path().unwrap_or(&String::new()),
            state.move_note_new_path_input(),
            is_folder
        )
    } else if state.notebook_path().is_empty() {
        Container::new(
            Text::new("Please configure the 'notebook_path' in your config.json file to open a notebook.")
                .size(20)
                .style(|_: &_| iced::widget::text::Style {
                    color: Some(iced::Color::from_rgb(0.7, 0.2, 0.2)),
                    ..Default::default()
                })
        )
         .center_x(Length::Fill)
         .center_y(Length::Fill)
         .width(Length::Fill)
         .height(Length::Fill)
         .into()
    } else {
        // Main editor view with note explorer and text editor
        let note_explorer_view: Element<'_, Message> = Container::new(
            note_explorer_component
                .view(state.selected_note_path())
                .map(|note_explorer_message| match note_explorer_message {
                    note_explorer::Message::NoteSelected(path) => Message::NoteSelected(path),
                    note_explorer::Message::ToggleFolder(path) => {
                        Message::NoteExplorerMessage(note_explorer::Message::ToggleFolder(path))
                    }
                    note_explorer::Message::InitiateFolderRename(path) => {
                        Message::InitiateFolderRename(path)
                    }
                    other_msg => Message::NoteExplorerMessage(other_msg),
                }),
        )
        .width(Length::FillPortion(2))
        .into();

        // Remove the height constraint from the editor widget
        let mut editor_widget = text_editor(content);

        if state.selected_note_path().is_some() {
            editor_widget = editor_widget.on_action(Message::EditorAction);
        }

        // Create a column with the editor and a bottom spacer
        let editor_with_bottom_spacer = Column::new()
            .push(editor_widget)
            .push(Container::new(Text::new("")) // Empty container as spacer
                  .height(Length::Fixed(5.0))
                  .width(Length::Fill))
            .width(Length::Fill);

        // Create a row with the editor column and a right-side spacer
        let editor_with_padding = Row::new()
            .push(editor_with_bottom_spacer)
            .push(Container::new(Text::new("").width(Length::Fixed(20.0)))) // Right padding
            .width(Length::Fill);

        // Keep the scrollable container with height constraints
        let editor_scrollable = iced::widget::scrollable(editor_with_padding)
            .width(Length::Fill)
            .height(Length::Fill);
            
        // Create the editor container with the scrollable editor
        let editor_container = Container::new(editor_scrollable)
            .width(Length::FillPortion(8))
            .height(Length::Fill);

        let content_row = Row::new()
            .push(note_explorer_view)
            .push(editor_container)
            .spacing(10)
            .padding(10)
            .width(Length::Fill)
            .height(Length::FillPortion(10));

        // Labels section at the bottom
        let labels_row = input_fields::create_labels_section(
            state.selected_note_path(),
            state.selected_note_labels(),
            state.new_label_text(),
        );

        let bottom_bar: Element<'_, Message> = Container::new(labels_row)
            .width(Length::Fill)
            .height(Length::FillPortion(1))
            .into();

        Column::new().push(content_row).push(bottom_bar).into()
    };

    Container::new(Column::new().push(top_bar).push(main_content))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
