use super::HTML_BR_SENTINEL;
use super::clipboard::{
    is_probably_image_file, parse_clipboard_image_file_paths, parse_file_uri_to_path,
    paste_text_from_action, percent_decode, read_clipboard_image_file_as_base64_from_text,
    read_image_file_as_base64,
};
use super::preview::{
    build_markdown_preview_content, column_byte_offset, cursor_preview_character_index,
    cursor_preview_character_range, extract_embedded_image_ids, html_line_breaks_replacement,
    normalize_html_line_break_tags, preview_line_from_cursor_byte,
};
use base64::Engine;
use iced::widget::text_editor::{Action, Cursor as EditorCursor, Edit, Position as EditorPosition};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn extract_embedded_image_ids_parses_all_tags() {
    let markdown = "A ![first](images/img_one.png) and ![second](images/img_two.jpg).";
    let ids = extract_embedded_image_ids(markdown);

    let expected: HashSet<String> = [
        "images/img_one.png".to_string(),
        "images/img_two.jpg".to_string(),
    ]
    .into_iter()
    .collect();
    assert_eq!(ids, expected);
}

#[test]
fn build_markdown_preview_content_keeps_standard_markdown_images() {
    let markdown = "before ![image](images/img_one.png) after";
    let images = HashMap::new();

    let rendered = build_markdown_preview_content(markdown, &images);
    assert_eq!(rendered, markdown);
}

#[test]
fn normalize_html_line_break_tags_converts_br_variants() {
    let markdown = "one<br>two<br/>three<BR />four";
    let normalized = normalize_html_line_break_tags(markdown);

    let expected = format!("one{0}two{0}three{0}four", HTML_BR_SENTINEL);
    assert_eq!(normalized, expected);
}

#[test]
fn normalize_html_line_break_tags_keeps_non_break_html() {
    let markdown = "before <span>inline</span> after";
    let normalized = normalize_html_line_break_tags(markdown);

    assert_eq!(normalized, markdown);
}

#[test]
fn normalize_html_line_break_tags_does_not_touch_code_blocks() {
    let markdown = "```html\n<br>\n```";
    let normalized = normalize_html_line_break_tags(markdown);

    assert_eq!(normalized, markdown);
}

#[test]
fn normalize_html_line_break_tags_converts_multiple_breaks_in_one_html_fragment() {
    let markdown = "one<br><br>two";
    let normalized = normalize_html_line_break_tags(markdown);

    let expected = format!("one{0}{0}two", HTML_BR_SENTINEL);
    assert_eq!(normalized, expected);
}

#[test]
fn html_line_breaks_replacement_rejects_mixed_html() {
    assert_eq!(html_line_breaks_replacement("<br><span>"), None);
}

#[test]
fn normalize_html_line_break_tags_keeps_multiple_break_events_with_spaces_between_tags() {
    let markdown = "line 1<br> <br> <br> <br>line 2";
    let normalized = normalize_html_line_break_tags(markdown);

    assert_eq!(normalized.matches(HTML_BR_SENTINEL).count(), 4);
}

#[test]
fn normalize_html_line_break_tags_consumes_newline_immediately_after_break_tag() {
    let markdown = "line 1<br>\nline 2";
    let normalized = normalize_html_line_break_tags(markdown);

    let expected = format!("line 1{}line 2", HTML_BR_SENTINEL);
    assert_eq!(normalized, expected);
}

#[test]
fn preview_line_from_cursor_byte_tracks_newlines() {
    let markdown = "first line\nsecond line\nthird line";
    let third_line_start = markdown
        .find("third")
        .expect("test fixture should contain third line");

    assert_eq!(preview_line_from_cursor_byte(markdown, third_line_start), 2);
}

#[test]
fn preview_line_from_cursor_byte_counts_html_break_tags() {
    let markdown = "one<br>two<br/>three";
    let third_segment_start = markdown
        .find("three")
        .expect("test fixture should contain third segment");

    assert_eq!(
        preview_line_from_cursor_byte(markdown, third_segment_start),
        2
    );
}

#[test]
fn cursor_preview_character_index_handles_space_before_following_text() {
    let markdown = "hello sample";
    let cursor = EditorCursor {
        position: EditorPosition { line: 0, column: 6 },
        selection: None,
    };

    let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
    assert_eq!(
        index,
        Some(6),
        "cursor after a space before text should still point to the next visible character"
    );
}

#[test]
fn cursor_preview_character_index_handles_newline_before_following_text() {
    let markdown = "hello\nsample";
    let cursor = EditorCursor {
        position: EditorPosition { line: 1, column: 0 },
        selection: None,
    };

    let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
    assert_eq!(
        index,
        Some(6),
        "cursor after a newline before text should still point to the next visible character"
    );
}

#[test]
fn cursor_preview_character_range_defaults_to_single_character_without_selection() {
    let markdown = "hello";
    let cursor = EditorCursor {
        position: EditorPosition { line: 0, column: 2 },
        selection: None,
    };

    let range = cursor_preview_character_range(markdown, cursor, None, &HashMap::new());
    assert_eq!(range, Some((2, 1)));
}

#[test]
fn cursor_preview_character_range_matches_selected_text_length() {
    let markdown = "hello world";
    let cursor = EditorCursor {
        position: EditorPosition {
            line: 0,
            column: 11,
        },
        selection: Some(EditorPosition { line: 0, column: 6 }),
    };

    let range = cursor_preview_character_range(markdown, cursor, None, &HashMap::new());
    assert_eq!(range, Some((6, 5)));
}

#[test]
fn cursor_preview_character_range_uses_selected_text_for_word_selection() {
    let markdown = "hello world";
    let cursor = EditorCursor {
        position: EditorPosition { line: 0, column: 7 },
        selection: Some(EditorPosition { line: 0, column: 7 }),
    };

    let range = cursor_preview_character_range(markdown, cursor, Some("world"), &HashMap::new());
    assert_eq!(range, Some((6, 5)));
}

#[test]
fn cursor_preview_character_range_clamps_virtual_end_position_for_select_all() {
    let markdown = "hello";
    let cursor = EditorCursor {
        position: EditorPosition { line: 1, column: 0 },
        selection: Some(EditorPosition { line: 0, column: 0 }),
    };

    let range = cursor_preview_character_range(markdown, cursor, Some("hello"), &HashMap::new());
    assert_eq!(range, Some((0, 5)));
}

#[test]
fn cursor_preview_character_index_stays_stable_inside_ordered_list_marker() {
    let markdown = "1. first";

    for column in 0..=3 {
        let cursor = EditorCursor {
            position: EditorPosition { line: 0, column },
            selection: None,
        };

        let index = cursor_preview_character_index(markdown, cursor, &HashMap::new());
        assert_eq!(
            index,
            Some(0),
            "ordered-list marker should not shift rendered index at column {}",
            column
        );
    }

    let after_first_character = EditorCursor {
        position: EditorPosition { line: 0, column: 4 },
        selection: None,
    };
    assert_eq!(
        cursor_preview_character_index(markdown, after_first_character, &HashMap::new()),
        Some(1)
    );
}

#[test]
fn column_byte_offset_handles_multibyte_unicode() {
    let text = "båd";

    assert_eq!(column_byte_offset(text, 0), Some(0));
    assert_eq!(column_byte_offset(text, 1), Some(1));
    assert_eq!(column_byte_offset(text, 2), None);
    assert_eq!(column_byte_offset(text, 3), Some(3));
    assert_eq!(column_byte_offset(text, 4), Some(4));
}

#[test]
fn cursor_preview_character_index_tracks_multibyte_unicode_plain_text() {
    let markdown = "båd";

    let before_unicode = EditorCursor {
        position: EditorPosition { line: 0, column: 1 },
        selection: None,
    };
    let after_unicode = EditorCursor {
        position: EditorPosition { line: 0, column: 3 },
        selection: None,
    };

    assert_eq!(
        cursor_preview_character_index(markdown, before_unicode, &HashMap::new()),
        Some(1)
    );
    assert_eq!(
        cursor_preview_character_index(markdown, after_unicode, &HashMap::new()),
        Some(2)
    );
}

#[test]
fn cursor_preview_character_index_tracks_multibyte_unicode_inside_ordered_list() {
    let markdown = "1. båd";

    let before_unicode = EditorCursor {
        position: EditorPosition { line: 0, column: 4 },
        selection: None,
    };
    let after_unicode = EditorCursor {
        position: EditorPosition { line: 0, column: 6 },
        selection: None,
    };

    assert_eq!(
        cursor_preview_character_index(markdown, before_unicode, &HashMap::new()),
        Some(1)
    );
    assert_eq!(
        cursor_preview_character_index(markdown, after_unicode, &HashMap::new()),
        Some(2)
    );
}

#[test]
fn percent_decode_decodes_percent_encoded_text() {
    assert_eq!(
        percent_decode("/tmp/a%20b.png"),
        Some("/tmp/a b.png".to_string())
    );
}

#[test]
fn parse_file_uri_to_path_parses_localhost_uri() {
    let parsed = parse_file_uri_to_path("file://localhost/tmp/image.png");
    assert_eq!(parsed, Some(PathBuf::from("/tmp/image.png")));
}

#[test]
fn is_probably_image_file_matches_common_extensions() {
    assert!(is_probably_image_file(Path::new("/tmp/img.PNG")));
    assert!(!is_probably_image_file(Path::new("/tmp/file.txt")));
}

#[test]
fn parse_clipboard_image_file_paths_supports_gnome_copied_files_format() {
    let path = write_temp_test_file("png", b"\x89PNG\r\n\x1a\n");
    let escaped = path.to_string_lossy().replace(' ', "%20");
    let clipboard_text = format!("copy\nfile://{escaped}");

    let parsed = parse_clipboard_image_file_paths(&clipboard_text);
    assert_eq!(parsed, vec![path.clone()]);

    let _ = std::fs::remove_file(path);
}

#[test]
fn paste_text_from_action_extracts_pasted_text() {
    let action = Action::Edit(Edit::Paste(Arc::new("hello".to_string())));
    assert_eq!(paste_text_from_action(&action), Some("hello".to_string()));

    let non_paste_action = Action::Edit(Edit::Insert('a'));
    assert_eq!(paste_text_from_action(&non_paste_action), None);
}

#[test]
fn read_clipboard_image_file_as_base64_from_text_reads_image_path() {
    let path = write_temp_test_file("png", b"\x89PNG\r\n\x1a\npayload");
    let expected = read_image_file_as_base64(&path).expect("image should be readable");
    let clipboard_text = path.to_string_lossy().to_string();

    let actual = read_clipboard_image_file_as_base64_from_text(&clipboard_text)
        .expect("clipboard path should resolve to image bytes");
    assert_eq!(actual, expected);

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(actual)
        .expect("base64 should decode");
    assert_eq!(decoded, b"\x89PNG\r\n\x1a\npayload");

    let _ = std::fs::remove_file(path);
}

fn write_temp_test_file(ext: &str, bytes: &[u8]) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cognate_test_{nanos}.{ext}"));
    std::fs::write(&path, bytes).expect("failed to write temp file");
    path
}
