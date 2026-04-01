use iced::widget::{Column, Container, Row, Text, image, markdown, rich_text};
use iced::{Element, Length};
use std::cell::Cell;
use std::collections::HashMap;

use crate::components::editor::Message;
use crate::components::editor::core::HTML_BR_SENTINEL;
use crate::components::editor::state::editor_state::EditorState;

use super::MARKDOWN_PREVIEW_SCROLLABLE_ID;

struct MarkdownPreviewViewer<'a> {
    image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    indicator_char_range: Option<(usize, usize)>,
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
        if let Some(image_handle) = self.image_handles.get(url.as_str()) {
            return image(image_handle.clone())
                .width(Length::Fill)
                .content_fit(iced::ContentFit::Contain)
                .into();
        }

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
            self.indicator_char_range,
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
            self.indicator_char_range,
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
                self.indicator_char_range,
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
    indicator_char_range: Option<(usize, usize)>,
    consumed_chars: &Cell<usize>,
    font_override: Option<iced::Font>,
) -> Element<'a, Message> {
    let lines = split_markdown_spans_by_newline_with_indicator(
        text,
        style,
        indicator_char_range,
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
    indicator_char_range: Option<(usize, usize)>,
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

            let should_highlight = indicator_char_range.is_some_and(|(start, len)| {
                let end = start.saturating_add(len);
                global_char_index >= start && global_char_index < end
            });

            if should_highlight {
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

pub(super) fn build_markdown_preview_panel<'a>(
    state: &'a EditorState,
    markdown_content: &'a iced::widget::markdown::Content,
    markdown_image_handles: &'a HashMap<String, iced::widget::image::Handle>,
    preview_indicator_char_range: Option<(usize, usize)>,
) -> Element<'a, Message> {
    let markdown_preview_body: Element<'_, Message> = if state.selected_note_path().is_some() {
        let preview_viewer = MarkdownPreviewViewer {
            image_handles: markdown_image_handles,
            indicator_char_range: preview_indicator_char_range,
            consumed_chars: Cell::new(0),
        };
        markdown::view_with(markdown_content.items(), iced::Theme::Dark, &preview_viewer)
    } else {
        Container::new(Text::new("Select a note to see markdown preview."))
            .width(Length::Fill)
            .padding(10)
            .into()
    };

    let markdown_preview_frame = Container::new(markdown_preview_body)
        .width(Length::Fill)
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

    let markdown_preview_with_padding = Row::new()
        .push(markdown_preview_frame)
        .push(Container::new(Text::new("").width(Length::Fixed(20.0))))
        .width(Length::Fill);

    let markdown_preview_scrollable = iced::widget::scrollable(markdown_preview_with_padding)
        .width(Length::Fill)
        .height(Length::Fill)
        .id(MARKDOWN_PREVIEW_SCROLLABLE_ID);

    Container::new(markdown_preview_scrollable)
        .width(Length::FillPortion(4))
        .height(Length::Fill)
        .into()
}
