use iced::widget::{Column, Container, Text, button, rich_text};
use iced::widget::{markdown, text::Span};
use iced::{Border, Color, Element, Length};

use crate::components::editor::Message;
use crate::notebook::NoteSearchResult;

pub(super) fn render_search_results(
    search_query: &str,
    results: &[NoteSearchResult],
    next_cursor: Option<&str>,
    total: usize,
    loading: bool,
    error: Option<&str>,
) -> Element<'static, Message> {
    let mut results_column = Column::new().spacing(4).push(
        Text::new(format!(
            "Search results for '{}': {}",
            search_query,
            if total == 0 { results.len() } else { total }
        ))
        .size(14),
    );

    if let Some(error) = error {
        results_column = results_column.push(Text::new(error.to_string()).size(13));
        results_column =
            results_column.push(button(Text::new("Retry")).on_press(Message::RunSearch));
    } else if results.is_empty() && !loading {
        results_column = results_column.push(Text::new("No matches found.").size(13));
    } else {
        let max_results_to_render = 8;
        for result in results.iter().take(max_results_to_render) {
            results_column = results_column.push(
                button(Text::new(result.rel_path.clone()).size(14))
                    .on_press(Message::NoteSelected(result.rel_path.clone()))
                    .padding(3),
            );
            results_column = results_column.push(highlighted_snippet(result));
            results_column =
                results_column.push(Text::new(format!("Match: {:?}", result.match_type)).size(11));
        }

        if results.len() > max_results_to_render {
            results_column = results_column.push(Text::new("More results available...").size(12));
        }
    }
    if loading {
        results_column = results_column.push(Text::new("Searching...").size(12));
    } else if next_cursor.is_some() {
        results_column = results_column
            .push(button(Text::new("Load more results")).on_press(Message::LoadMoreSearchResults));
    }

    Container::new(results_column)
        .padding(6)
        .width(Length::Fill)
        .into()
}

fn highlighted_snippet(result: &NoteSearchResult) -> Element<'static, Message> {
    let chars = result.snippet.chars().collect::<Vec<_>>();
    let spans: Vec<Span<'static, markdown::Uri>> = chars
        .into_iter()
        .enumerate()
        .map(|(index, character)| {
            let mut span = Span::<markdown::Uri>::new(character.to_string());
            if result
                .highlights
                .iter()
                .any(|range| index >= range.start && index < range.end)
            {
                span = span
                    .background(Color::from_rgba(0.18, 0.70, 0.95, 0.28))
                    .border(Border::default().rounded(2.0));
            }
            span
        })
        .collect();
    rich_text(spans).size(12).into()
}
