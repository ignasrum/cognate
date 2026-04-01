use iced::widget::{Column, Container};
use iced::{Element, Length};
use std::collections::HashMap;

use crate::components::editor::Message;
use crate::components::editor::state::editor_state::EditorState;
use crate::components::note_explorer;
use crate::components::visualizer;

mod preview;
mod search_results;
mod top_bar;
mod workspace;

pub const MARKDOWN_PREVIEW_SCROLLABLE_ID: &str = "cognate_markdown_preview_scrollable";

pub fn generate_layout<'a>(
    state: &'a EditorState,
    content: &'a iced::widget::text_editor::Content,
    markdown_content: &'a iced::widget::markdown::Content,
    markdown_image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    note_explorer_component: &'a note_explorer::NoteExplorer,
    visualizer_component: &'a visualizer::Visualizer,
    preview_indicator_char_range: Option<(usize, usize)>,
) -> Element<'a, Message> {
    let top_bar = top_bar::build_top_bar(state, note_explorer_component);
    let main_content = workspace::build_main_content(
        state,
        content,
        markdown_content,
        markdown_image_handles,
        note_explorer_component,
        visualizer_component,
        preview_indicator_char_range,
    );

    Container::new(Column::new().push(top_bar).push(main_content))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
