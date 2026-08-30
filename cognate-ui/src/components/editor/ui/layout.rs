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

pub(crate) struct LayoutContext<'a> {
    pub(crate) state: &'a EditorState,
    pub(crate) content: &'a iced::widget::text_editor::Content,
    pub(crate) markdown_content: &'a iced::widget::markdown::Content,
    pub(crate) markdown_image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    pub(crate) image_context_menu: Option<&'a str>,
    pub(crate) image_context_position: Option<iced::Point>,
    pub(crate) note_explorer_component: &'a note_explorer::NoteExplorer,
    pub(crate) visualizer_component: &'a visualizer::Visualizer,
    pub(crate) preview_indicator_char_range: Option<(usize, usize)>,
}

pub(crate) fn generate_layout_with_image_context<'a>(
    context: LayoutContext<'a>,
) -> Element<'a, Message> {
    if let Some(error) = context.state.connection_error() {
        return workspace::build_connection_error_page(error);
    }
    if let Some(error) = context.state.note_load_error() {
        return workspace::build_note_load_error_page(error);
    }

    let top_bar = top_bar::build_top_bar(context.state, context.note_explorer_component);
    let main_content = workspace::build_main_content(context);
    Container::new(Column::new().push(top_bar).push(main_content))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
