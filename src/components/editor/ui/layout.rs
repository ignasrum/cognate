use iced::widget::{
    Column, Container, Row, Space, Text, TextInput as IcedTextInput, button, image, markdown,
    rich_text, text_editor,
};
use iced::{Element, Length};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::components::editor::Message;
use crate::components::editor::core::HTML_BR_SENTINEL;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::editor::ui::dialogs;
use crate::components::editor::ui::input_fields;
use crate::components::note_explorer;
use crate::components::visualizer;
use crate::notebook::NoteSearchResult;

pub const MARKDOWN_PREVIEW_SCROLLABLE_ID: &str = "cognate_markdown_preview_scrollable";

struct MarkdownPreviewViewer<'a> {
    image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    indicator_char_index: Option<usize>,
    consumed_chars: Cell<usize>,
}

impl<'a> markdown::Viewer<'a, Message> for MarkdownPreviewViewer<'a> {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::MarkdownLinkClicked(url)
    }

    fn image(
        &self,
        settings: markdown::Settings,
        url: &'a markdown::Uri,
        title: &'a str,
        _alt: &markdown::Text,
    ) -> Element<'a, Message> {
        if let Some(image_id) = url.strip_prefix("cognate-image://")
            && let Some(image_handle) = self.image_handles.get(image_id)
        {
            return image(image_handle.clone())
                .width(Length::Fill)
                .content_fit(iced::ContentFit::Contain)
                .into();
        }

        let fallback_text = if title.is_empty() {
            url.as_str()
        } else {
            title
        };
        Container::new(Text::new(fallback_text))
            .padding(settings.spacing.0 / 2.0)
            .into()
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        render_text_block_with_indicator(
            text,
            settings.style,
            settings.text_size,
            self.indicator_char_index,
            &self.consumed_chars,
            None,
        )
    }

    fn heading(
        &self,
        settings: markdown::Settings,
        level: &'a markdown::HeadingLevel,
        text: &'a markdown::Text,
        index: usize,
    ) -> Element<'a, Message> {
        let heading_size = match level {
            markdown::HeadingLevel::H1 => settings.h1_size,
            markdown::HeadingLevel::H2 => settings.h2_size,
            markdown::HeadingLevel::H3 => settings.h3_size,
            markdown::HeadingLevel::H4 => settings.h4_size,
            markdown::HeadingLevel::H5 => settings.h5_size,
            markdown::HeadingLevel::H6 => settings.h6_size,
        };

        Container::new(render_text_block_with_indicator(
            text,
            settings.style,
            heading_size,
            self.indicator_char_index,
            &self.consumed_chars,
            None,
        ))
        .padding(iced::padding::top(if index > 0 {
            settings.text_size / 2.0
        } else {
            iced::Pixels::ZERO
        }))
        .into()
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        let mut rendered_lines = Column::new().spacing(0);

        for line in lines {
            rendered_lines = rendered_lines.push(render_text_block_with_indicator(
                line,
                settings.style,
                settings.code_size,
                self.indicator_char_index,
                &self.consumed_chars,
                Some(settings.style.code_block_font),
            ));
        }

        Container::new(
            iced::widget::scrollable(Container::new(rendered_lines).padding(settings.code_size))
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::default()
                        .width(settings.code_size / 2)
                        .scroller_width(settings.code_size / 2),
                )),
        )
        .width(Length::Fill)
        .padding(settings.code_size / 4)
        .class(<iced::Theme as markdown::Catalog>::code_block())
        .into()
    }
}

fn render_text_block_with_indicator<'a>(
    text: &markdown::Text,
    style: markdown::Style,
    text_size: iced::Pixels,
    indicator_char_index: Option<usize>,
    consumed_chars: &Cell<usize>,
    font_override: Option<iced::Font>,
) -> Element<'a, Message> {
    let lines = split_markdown_spans_by_newline_with_indicator(
        text,
        style,
        indicator_char_index,
        consumed_chars,
    );

    let mut paragraph_lines = Column::new().spacing(0);

    for line in lines {
        let mut line_widget = if line.is_empty() {
            rich_text(vec![iced::widget::text::Span::<markdown::Uri>::new(" ")])
        } else {
            rich_text(line).on_link_click(Message::MarkdownLinkClicked)
        };

        if let Some(font) = font_override {
            line_widget = line_widget.font(font);
        }

        paragraph_lines = paragraph_lines.push(line_widget.size(text_size));
    }

    paragraph_lines.into()
}

fn split_markdown_spans_by_newline_with_indicator(
    text: &markdown::Text,
    style: markdown::Style,
    indicator_char_index: Option<usize>,
    consumed_chars: &Cell<usize>,
) -> Vec<Vec<iced::widget::text::Span<'static, markdown::Uri>>> {
    let spans = text.spans(style);
    let mut lines: Vec<Vec<iced::widget::text::Span<'static, markdown::Uri>>> = vec![Vec::new()];
    let mut global_char_index = consumed_chars.get();

    for span in spans.iter() {
        let content = span.text.as_ref().replace(HTML_BR_SENTINEL, "\n");

        for ch in content.chars() {
            if ch == '\n' {
                global_char_index += 1;
                lines.push(Vec::new());
                continue;
            }

            let mut char_span = span.clone();
            char_span.text = ch.to_string().into();

            if indicator_char_index == Some(global_char_index) {
                char_span = char_span
                    .background(iced::Color::from_rgba(0.18, 0.70, 0.95, 0.28))
                    .border(iced::Border::default().rounded(2.0));
            }

            lines
                .last_mut()
                .expect("at least one line present")
                .push(char_span);
            global_char_index += 1;
        }
    }

    consumed_chars.set(global_char_index);
    lines
}

pub fn generate_layout<'a>(
    state: &'a EditorState,
    content: &'a iced::widget::text_editor::Content,
    markdown_content: &'a iced::widget::markdown::Content,
    markdown_image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    note_explorer_component: &'a note_explorer::NoteExplorer,
    visualizer_component: &'a visualizer::Visualizer,
    preview_indicator_char_index: Option<usize>,
) -> Element<'a, Message> {
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

    // Main content area
    let main_content: Element<'_, Message> = if state.show_about_info() {
        dialogs::about_dialog(state.app_version())
    } else if state.show_visualizer() {
        Container::new(visualizer_component.view().map(Message::VisualizerMsg))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if state.show_new_note_input() {
        dialogs::new_note_dialog(state.new_note_path_input())
    } else if state.show_move_note_input() {
        let is_folder = state
            .move_note_current_path()
            .map(|p| state.is_folder_path(p, &note_explorer_component.notes))
            .unwrap_or(false);

        dialogs::move_note_dialog(
            state.move_note_current_path().unwrap_or(&String::new()),
            state.move_note_new_path_input(),
            is_folder,
        )
    } else if state.show_embedded_image_delete_confirmation() {
        dialogs::confirm_embedded_image_delete_dialog(state.pending_embedded_image_delete_count())
    } else if state.notebook_path().is_empty() {
        Container::new(
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
        .into()
    } else {
        // Main editor view with note explorer and text editor
        let mut explorer_column = Column::new().spacing(8).width(Length::Fill);

        if !state.search_query().trim().is_empty() {
            explorer_column = explorer_column.push(render_search_results(
                state.search_query(),
                state.search_results(),
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

        // Remove the height constraint from the editor widget
        let mut editor_widget = text_editor(content);

        if state.selected_note_path().is_some() {
            editor_widget =
                editor_widget
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

        // Create a column with note info and the editor
        let mut editor_column = Column::new().spacing(5).width(Length::Fill);

        if let Some(note_info_row) = selected_note_info {
            editor_column = editor_column.push(note_info_row);
        }

        editor_column = editor_column.push(editor_widget).width(Length::Fill);

        // Create a row with the editor column and a right-side spacer
        let editor_with_padding = Row::new()
            .push(editor_column)
            .push(Container::new(Text::new("").width(Length::Fixed(20.0)))) // Right padding
            .width(Length::Fill);

        // Keep the scrollable container with height constraints
        let editor_scrollable = iced::widget::scrollable(editor_with_padding)
            .width(Length::Fill)
            .height(Length::Fill);

        // Create the editor container with the scrollable editor
        let editor_container = Container::new(editor_scrollable)
            .width(Length::FillPortion(4))
            .height(Length::Fill);

        let markdown_preview_body: Element<'_, Message> = if state.selected_note_path().is_some() {
            let preview_viewer = MarkdownPreviewViewer {
                image_handles: markdown_image_handles,
                indicator_char_index: preview_indicator_char_index,
                consumed_chars: Cell::new(0),
            };
            markdown::view_with(markdown_content.items(), iced::Theme::Dark, &preview_viewer)
        } else {
            Container::new(Text::new("Select a note to see markdown preview."))
                .width(Length::Fill)
                .padding(10)
                .into()
        };

        let markdown_preview_scrollable = iced::widget::scrollable(markdown_preview_body)
            .width(Length::Fill)
            .height(Length::Fill)
            .id(MARKDOWN_PREVIEW_SCROLLABLE_ID);

        let markdown_preview_container = Container::new(markdown_preview_scrollable)
            .width(Length::FillPortion(4))
            .height(Length::Fill)
            .padding(8)
            .style(|theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(theme.palette().background)),
                text_color: None,
                border: iced::Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: theme.palette().primary,
                },
                shadow: iced::Shadow::default(),
                snap: false,
            });

        let content_row = Row::new()
            .push(note_explorer_view)
            .push(editor_container)
            .push(markdown_preview_container)
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
            .height(Length::Shrink)
            .into();

        Column::new().push(content_row).push(bottom_bar).into()
    };

    Container::new(Column::new().push(top_bar).push(main_content))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn render_search_results(
    search_query: &str,
    results: &[NoteSearchResult],
) -> Element<'static, Message> {
    let mut results_column = Column::new().spacing(4).push(
        Text::new(format!(
            "Search results for '{}': {}",
            search_query,
            results.len()
        ))
        .size(14),
    );

    if results.is_empty() {
        results_column = results_column.push(Text::new("No matches found.").size(13));
    } else {
        let max_results_to_render = 8;
        for result in results.iter().take(max_results_to_render) {
            results_column = results_column.push(
                button(Text::new(result.rel_path.clone()).size(14))
                    .on_press(Message::NoteSelected(result.rel_path.clone()))
                    .padding(3),
            );
            results_column = results_column.push(Text::new(result.snippet.clone()).size(12));
        }

        if results.len() > max_results_to_render {
            results_column = results_column.push(
                Text::new(format!(
                    "... and {} more matches",
                    results.len() - max_results_to_render
                ))
                .size(12),
            );
        }
    }

    Container::new(results_column)
        .padding(6)
        .width(Length::Fill)
        .into()
}
