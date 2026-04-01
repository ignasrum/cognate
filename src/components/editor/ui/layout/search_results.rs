use iced::widget::{Column, Container, Text, button};
use iced::{Element, Length};

use crate::components::editor::Message;
use crate::notebook::NoteSearchResult;

pub(super) fn render_search_results(
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
