use iced::widget::{Button, Column, Container, Row, Text, text_editor};
use iced::{Element, Length};

use crate::components::editor::Message;
use crate::components::editor::ui::dialogs;
use crate::components::editor::ui::input_fields;
use crate::components::note_explorer;

use super::preview;
use super::search_results;

pub(super) fn build_connection_error_page<'a>(error: &'a str) -> Element<'a, Message> {
    build_status_page(
        "Could not connect to server",
        error,
        Some(Message::RetryConnection),
    )
}

pub(super) fn build_note_load_error_page<'a>(error: &'a str) -> Element<'a, Message> {
    build_status_page("Could not load note", error, None)
}

fn build_status_page<'a>(
    title: &'a str,
    error: &'a str,
    retry: Option<Message>,
) -> Element<'a, Message> {
    let mut content = Column::new()
        .spacing(16)
        .align_x(iced::Alignment::Center)
        .push(Text::new(title).size(32))
        .push(Text::new(error).size(16));
    if let Some(retry) = retry {
        content = content.push(
            Button::new(Text::new("Retry connection"))
                .padding(10)
                .on_press(retry),
        );
    }

    Container::new(content)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn build_main_content<'a>(context: super::LayoutContext<'a>) -> Element<'a, Message> {
    let state = context.state;
    let note_explorer_component = context.note_explorer_component;
    let visualizer_component = context.visualizer_component;

    if state.show_about_info() {
        return dialogs::about_dialog(state.app_version());
    }

    if state.show_visualizer() {
        return Container::new(visualizer_component.view().map(Message::VisualizerMsg))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    if state.show_new_note_input() {
        return dialogs::new_note_dialog(state.new_note_path_input());
    }

    if state.show_move_note_input() {
        let is_folder = state
            .move_note_current_path()
            .map(|p| state.is_folder_path(p, &note_explorer_component.notes))
            .unwrap_or(false);

        return dialogs::move_note_dialog(
            state.move_note_current_path().unwrap_or(&String::new()),
            state.move_note_new_path_input(),
            is_folder,
        );
    }

    if state.show_embedded_image_delete_confirmation() {
        return dialogs::confirm_embedded_image_delete_dialog(
            state.pending_embedded_image_delete_count(),
        );
    }

    if state.is_conflict_dialog_open()
        && let Some(conflict) = state.conflict()
    {
        return dialogs::conflict_dialog(conflict);
    }

    if state.notebook_path().is_empty() {
        return Container::new(
            Text::new(
                "Please configure the 'notebook_path' in your config.json file to open a notebook.",
            )
            .size(20)
            .style(|_: &_| iced::widget::text::Style {
                color: Some(iced::Color::from_rgb(0.7, 0.2, 0.2)),
            }),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }
    build_editor_workspace(context)
}
fn build_editor_workspace<'a>(context: super::LayoutContext<'a>) -> Element<'a, Message> {
    let state = context.state;
    let note_explorer_component = context.note_explorer_component;
    let content = context.content;
    let markdown_content = context.markdown_content;
    let markdown_image_handles = context.markdown_image_handles;
    let image_context_menu = context.image_context_menu;
    let image_context_position = context.image_context_position;
    let preview_indicator_char_range = context.preview_indicator_char_range;
    let mut explorer_column = Column::new().spacing(8).width(Length::Fill);

    if !state.search_query().trim().is_empty() {
        explorer_column = explorer_column.push(search_results::render_search_results(
            state.search_query(),
            state.search_results(),
            state.search_next_cursor(),
            state.search_total(),
            state.search_loading(),
            state.search_error(),
        ));
    }

    explorer_column = explorer_column.push(
        note_explorer_component
            .view(state.selected_note_path())
            .map(|note_explorer_message| match note_explorer_message {
                note_explorer::Message::NoteSelected(path) => Message::NoteSelected(path),
                note_explorer::Message::ToggleFolder(path) => {
                    Message::NoteExplorerMsg(note_explorer::Message::ToggleFolder(path))
                }
                note_explorer::Message::InitiateFolderRename(path) => {
                    Message::InitiateFolderRename(path)
                }
                other_msg => Message::NoteExplorerMsg(other_msg),
            }),
    );

    let note_explorer_view: Element<'_, Message> = Container::new(explorer_column)
        .width(Length::FillPortion(2))
        .into();

    let mut editor_widget = text_editor(content);

    if state.selected_note_path().is_some() {
        editor_widget = editor_widget
            .on_action(Message::EditorAction)
            .key_binding(|key_press| {
                let is_paste_shortcut = key_press.modifiers.command()
                    && !key_press.modifiers.alt()
                    && matches!(
                        key_press.key.as_ref(),
                        iced::keyboard::Key::Character("v" | "V")
                    );

                if is_paste_shortcut {
                    Some(iced::widget::text_editor::Binding::Custom(
                        Message::PasteFromClipboard,
                    ))
                } else {
                    iced::widget::text_editor::Binding::from_key_press(key_press)
                }
            });
    }

    let selected_note_last_updated = state
        .selected_note_path()
        .and_then(|selected_path| {
            note_explorer_component
                .notes
                .iter()
                .find(|note| &note.rel_path == selected_path)
        })
        .and_then(|note| note.last_updated.as_deref());

    let selected_note_info = state.selected_note_path().map(|_| {
        let updated_text = selected_note_last_updated.map_or_else(
            || "Last updated: unknown".to_string(),
            |value| format!("Last updated: {}", value),
        );

        Row::new().push(
            Container::new(Text::new(updated_text).size(14))
                .width(Length::Fill)
                .align_x(iced::Alignment::End),
        )
    });

    let mut editor_column = Column::new().spacing(5).width(Length::Fill);

    if let Some(note_info_row) = selected_note_info {
        editor_column = editor_column.push(note_info_row);
    }

    editor_column = editor_column.push(editor_widget).width(Length::Fill);

    let editor_with_padding = Row::new()
        .push(editor_column)
        .push(Container::new(Text::new("").width(Length::Fixed(20.0))))
        .width(Length::Fill);

    let editor_scrollable = iced::widget::scrollable(editor_with_padding)
        .width(Length::Fill)
        .height(Length::Fill);

    let editor_container = Container::new(editor_scrollable)
        .width(Length::FillPortion(4))
        .height(Length::Fill);
    let markdown_preview_container = preview::build_markdown_preview_panel(
        state,
        markdown_content,
        markdown_image_handles,
        image_context_menu,
        image_context_position,
        preview_indicator_char_range,
    );

    let content_row = Row::new()
        .push(note_explorer_view)
        .push(editor_container)
        .push(markdown_preview_container)
        .spacing(10)
        .padding(10)
        .width(Length::Fill)
        .height(Length::FillPortion(10));

    let labels_row = input_fields::create_labels_section(
        state.selected_note_path(),
        state.selected_note_labels(),
        state.new_label_text(),
    );

    let bottom_bar: Element<'_, Message> = Container::new(labels_row)
        .width(Length::Fill)
        .height(Length::Shrink)
        .into();

    Column::new().push(content_row).push(bottom_bar).into()
}
