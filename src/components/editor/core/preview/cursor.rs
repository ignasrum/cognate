use iced::widget::text_editor::{Action, Cursor as EditorCursor, Edit, Position as EditorPosition};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
const HTML_BR_SENTINEL_CHAR: char = '\u{E000}';

pub(crate) fn extract_embedded_image_ids(markdown: &str) -> HashSet<String> {
    let mut referenced = HashSet::new();

    for event in
        pulldown_cmark::Parser::new_ext(markdown, super::transform::markdown_parser_options())
    {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event {
            let image_ref = dest_url.as_ref().trim();
            if !image_ref.is_empty() {
                referenced.insert(image_ref.to_string());
            }
        }
    }

    referenced
}

pub(crate) fn preview_markdown_after_action(
    markdown: &str,
    cursor: EditorCursor,
    action: &Action,
) -> Option<String> {
    match action {
        Action::Edit(edit) => preview_markdown_after_edit(markdown, cursor, edit),
        _ => None,
    }
}

fn preview_markdown_after_edit(
    markdown: &str,
    cursor: EditorCursor,
    edit: &Edit,
) -> Option<String> {
    let (selection_start, selection_end) =
        selection_byte_range(markdown, cursor.position, cursor.selection)?;

    let mut preview = markdown.to_string();
    let has_selection = selection_start != selection_end;

    match edit {
        Edit::Insert(ch) => {
            preview.replace_range(selection_start..selection_end, &ch.to_string());
        }
        Edit::Paste(text) => {
            preview.replace_range(selection_start..selection_end, text.as_str());
        }
        Edit::Enter => {
            preview.replace_range(selection_start..selection_end, "\n");
        }
        Edit::Backspace => {
            if has_selection {
                preview.replace_range(selection_start..selection_end, "");
            } else {
                let backspace_start = previous_char_boundary(markdown, selection_start)?;
                preview.replace_range(backspace_start..selection_start, "");
            }
        }
        Edit::Delete => {
            if has_selection {
                preview.replace_range(selection_start..selection_end, "");
            } else {
                let delete_end = next_char_boundary(markdown, selection_end)?;
                preview.replace_range(selection_end..delete_end, "");
            }
        }
        Edit::Indent | Edit::Unindent => return None,
    }

    Some(preview)
}

fn selection_byte_range(
    markdown: &str,
    position: EditorPosition,
    selection: Option<EditorPosition>,
) -> Option<(usize, usize)> {
    let position_index = position_to_byte_index(markdown, position)?;

    if let Some(selection_position) = selection {
        let selection_index = position_to_byte_index(markdown, selection_position)?;
        if position_index <= selection_index {
            Some((position_index, selection_index))
        } else {
            Some((selection_index, position_index))
        }
    } else {
        Some((position_index, position_index))
    }
}

fn position_to_byte_index(markdown: &str, position: EditorPosition) -> Option<usize> {
    let mut line_start = 0usize;
    let mut current_line = 0usize;

    while current_line < position.line {
        let next_newline = markdown[line_start..].find('\n')?;
        line_start += next_newline + 1;
        current_line += 1;
    }

    let line_end = markdown[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(markdown.len());
    let line_text = &markdown[line_start..line_end];
    let column_offset = column_byte_offset(line_text, position.column)?;

    Some(line_start + column_offset)
}

fn position_to_byte_index_clamped_for_preview(markdown: &str, position: EditorPosition) -> usize {
    let mut line_start = 0usize;
    let mut current_line = 0usize;

    while current_line < position.line {
        let Some(next_newline) = markdown[line_start..].find('\n') else {
            return markdown.len();
        };

        line_start += next_newline + 1;
        current_line += 1;
    }

    let line_end = markdown[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(markdown.len());

    let mut clamped_index = line_start.saturating_add(position.column).min(line_end);

    while clamped_index > line_start && !markdown.is_char_boundary(clamped_index) {
        clamped_index -= 1;
    }

    clamped_index
}

pub(crate) fn cursor_preview_character_index(
    markdown: &str,
    cursor: EditorCursor,
    images: &HashMap<String, String>,
) -> Option<usize> {
    let cursor_byte_index = position_to_byte_index_clamped_for_preview(markdown, cursor.position);
    Some(adjusted_preview_character_index_at_byte(
        markdown,
        cursor_byte_index,
        images,
    ))
}

fn selection_byte_range_from_selected_text(
    markdown: &str,
    selected_text: Option<&str>,
    cursor_byte_index: usize,
) -> Option<(usize, usize)> {
    let selected_text = selected_text?;
    if selected_text.is_empty() || selected_text.len() > markdown.len() {
        return None;
    }

    if selected_text == markdown {
        return Some((0, markdown.len()));
    }

    let mut best_match: Option<(usize, usize, usize, bool)> = None;
    let mut search_start = 0usize;

    while let Some(relative_match) = markdown[search_start..].find(selected_text) {
        let match_start = search_start + relative_match;
        let match_end = match_start + selected_text.len();
        let contains_cursor = cursor_byte_index >= match_start && cursor_byte_index <= match_end;
        let distance = if contains_cursor {
            0
        } else if cursor_byte_index < match_start {
            match_start - cursor_byte_index
        } else {
            cursor_byte_index.saturating_sub(match_end)
        };

        let replace = match best_match {
            None => true,
            Some((_, _, best_distance, best_contains_cursor)) => {
                distance < best_distance
                    || (distance == best_distance && contains_cursor && !best_contains_cursor)
            }
        };

        if replace {
            best_match = Some((match_start, match_end, distance, contains_cursor));
        }

        search_start = match_start + 1;
        if search_start > markdown.len() {
            break;
        }
    }

    best_match.map(|(start, end, _, _)| (start, end))
}

pub(crate) fn cursor_preview_character_range(
    markdown: &str,
    cursor: EditorCursor,
    selected_text: Option<&str>,
    images: &HashMap<String, String>,
) -> Option<(usize, usize)> {
    let cursor_byte_index = position_to_byte_index_clamped_for_preview(markdown, cursor.position);
    let cursor_char_index =
        adjusted_preview_character_index_at_byte(markdown, cursor_byte_index, images);

    let cursor_range = cursor.selection.and_then(|selection_position| {
        let selection_byte_index =
            position_to_byte_index_clamped_for_preview(markdown, selection_position);

        if selection_byte_index == cursor_byte_index {
            None
        } else if cursor_byte_index <= selection_byte_index {
            Some((cursor_byte_index, selection_byte_index))
        } else {
            Some((selection_byte_index, cursor_byte_index))
        }
    });

    let (selection_start, selection_end) = cursor_range
        .or_else(|| {
            selection_byte_range_from_selected_text(markdown, selected_text, cursor_byte_index)
        })
        .unwrap_or((cursor_byte_index, cursor_byte_index));

    if selection_start == selection_end {
        return Some((cursor_char_index, 1));
    };

    let preview_start = adjusted_preview_character_index_at_byte(markdown, selection_start, images);
    let preview_end = adjusted_preview_character_index_at_byte(markdown, selection_end, images);
    let preview_length = preview_end.saturating_sub(preview_start);

    if preview_length == 0 {
        Some((cursor_char_index, 1))
    } else {
        Some((preview_start, preview_length))
    }
}

fn preview_character_index_at_byte(
    markdown: &str,
    byte_index: usize,
    images: &HashMap<String, String>,
) -> usize {
    let clamped_index = byte_index.min(markdown.len());
    let preview_full = super::transform::build_markdown_preview_content(markdown, images);
    let preview_prefix =
        super::transform::build_markdown_preview_content(&markdown[..clamped_index], images);

    let boundary = if preview_full.starts_with(&preview_prefix) {
        preview_prefix.len()
    } else {
        common_prefix_byte_len(&preview_full, &preview_prefix)
    };

    preview_rendered_char_count_until_byte(&preview_full, boundary)
}

fn preview_rendered_char_count_until_byte(preview_markdown: &str, byte_boundary: usize) -> usize {
    let clamped_boundary = byte_boundary.min(preview_markdown.len());

    pulldown_cmark::Parser::new_ext(
        preview_markdown,
        super::transform::markdown_parser_options(),
    )
    .into_offset_iter()
    .fold(0usize, |count, (event, range)| {
        if range.start >= clamped_boundary {
            return count;
        }

        match event {
            pulldown_cmark::Event::Text(text) => {
                if range.end <= clamped_boundary {
                    count + text.chars().count()
                } else {
                    count
                        + preview_markdown[range.start..clamped_boundary]
                            .chars()
                            .count()
                }
            }
            pulldown_cmark::Event::Code(code) => {
                if range.end <= clamped_boundary {
                    count + code.chars().count()
                } else {
                    count
                        + preview_markdown[range.start..clamped_boundary]
                            .chars()
                            .count()
                }
            }
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => {
                if range.end <= clamped_boundary {
                    count + 1
                } else {
                    count
                }
            }
            _ => count,
        }
    })
}

fn common_prefix_byte_len(left: &str, right: &str) -> usize {
    let mut total = 0usize;

    for (left_char, right_char) in left.chars().zip(right.chars()) {
        if left_char != right_char {
            break;
        }

        total += left_char.len_utf8();
    }

    total
}

fn adjusted_preview_character_index_at_byte(
    markdown: &str,
    byte_index: usize,
    images: &HashMap<String, String>,
) -> usize {
    let clamped_index = byte_index.min(markdown.len());
    let mut rendered_char_count = preview_character_index_at_byte(markdown, clamped_index, images);

    let previous_char = markdown[..clamped_index].chars().next_back();
    let next_char = markdown[clamped_index..].chars().next();

    if matches!(previous_char, Some(' ' | '\t' | '\n' | '\r'))
        && matches!(next_char, Some(ch) if ch != '\n' && ch != '\r')
        && let Some(next_boundary) = next_char_boundary(markdown, clamped_index)
    {
        let rendered_with_next = preview_character_index_at_byte(markdown, next_boundary, images);

        if rendered_with_next > rendered_char_count {
            rendered_char_count = rendered_with_next.saturating_sub(1);
        }
    }

    rendered_char_count
}

pub(crate) fn preview_rendered_char_count(preview_markdown: &str) -> usize {
    pulldown_cmark::Parser::new_ext(
        preview_markdown,
        super::transform::markdown_parser_options(),
    )
    .fold(0usize, |count, event| match event {
        pulldown_cmark::Event::Text(text) => count + text.chars().count(),
        pulldown_cmark::Event::Code(code) => count + code.chars().count(),
        pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => count + 1,
        _ => count,
    })
}

#[cfg(test)]
pub(crate) fn preview_line_from_cursor_byte(markdown: &str, cursor_byte_index: usize) -> usize {
    let clamped_index = cursor_byte_index.min(markdown.len());
    let prefix = &markdown[..clamped_index];
    let normalized_prefix = super::transform::normalize_html_line_break_tags(prefix);

    normalized_prefix
        .chars()
        .filter(|ch| *ch == '\n' || *ch == HTML_BR_SENTINEL_CHAR)
        .count()
}

pub(crate) fn column_byte_offset(text: &str, column: usize) -> Option<usize> {
    if column <= text.len() && text.is_char_boundary(column) {
        Some(column)
    } else {
        None
    }
}

fn previous_char_boundary(text: &str, index: usize) -> Option<usize> {
    if index == 0 || index > text.len() {
        return None;
    }

    text[..index]
        .char_indices()
        .next_back()
        .map(|(offset, _)| offset)
}

fn next_char_boundary(text: &str, index: usize) -> Option<usize> {
    if index >= text.len() {
        return None;
    }

    let ch = text[index..].chars().next()?;
    Some(index + ch.len_utf8())
}
