#[path = "preview/cursor.rs"]
mod cursor;
#[path = "preview/transform.rs"]
mod transform;

#[cfg(test)]
pub(crate) use cursor::{column_byte_offset, preview_line_from_cursor_byte};
pub(crate) use cursor::{
    cursor_preview_character_index, cursor_preview_character_range, extract_embedded_image_ids,
    preview_markdown_after_action, preview_rendered_char_count,
};
pub(crate) use transform::build_markdown_preview_content;
#[cfg(test)]
pub(crate) use transform::{html_line_breaks_replacement, normalize_html_line_break_tags};
