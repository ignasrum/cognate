use std::collections::HashMap;

use super::super::HTML_BR_SENTINEL;

pub(crate) fn build_markdown_preview_content(
    markdown: &str,
    images: &HashMap<String, String>,
) -> String {
    let _ = images;
    normalize_html_line_break_tags(&linkify_bare_urls(markdown))
}

fn linkify_bare_urls(markdown: &str) -> String {
    let mut result = String::with_capacity(markdown.len());
    let mut in_fenced_code_block = false;

    for line in markdown.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = content.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fenced_code_block = !in_fenced_code_block;
            result.push_str(line);
        } else if in_fenced_code_block {
            result.push_str(line);
        } else {
            result.push_str(&linkify_bare_urls_in_line(content));
            if line.ends_with('\n') {
                result.push('\n');
            }
        }
    }

    result
}

fn linkify_bare_urls_in_line(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut cursor = 0;

    for (index, _) in line.match_indices(char::is_whitespace) {
        let token_start = if cursor == 0 { 0 } else { cursor };
        let token_end = index;
        append_linkified_token(&mut result, &line[token_start..token_end]);
        let whitespace_len = line[index..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();
        result.push_str(&line[index..index + whitespace_len]);
        cursor = index + whitespace_len;
    }

    append_linkified_token(&mut result, &line[cursor..]);
    result
}

fn append_linkified_token(result: &mut String, token: &str) {
    let is_url = token.starts_with("http://") || token.starts_with("https://");
    if !is_url {
        result.push_str(token);
        return;
    }

    let punctuation_len = token
        .chars()
        .rev()
        .take_while(|character| matches!(character, '.' | ',' | ';' | ':' | '!' | '?'))
        .map(char::len_utf8)
        .sum::<usize>();
    let url_end = token.len().saturating_sub(punctuation_len);
    result.push('<');
    result.push_str(&token[..url_end]);
    result.push('>');
    result.push_str(&token[url_end..]);
}

pub(crate) fn normalize_html_line_break_tags(markdown: &str) -> String {
    let mut normalized = String::with_capacity(markdown.len());
    let mut cursor = 0usize;

    for (event, range) in
        pulldown_cmark::Parser::new_ext(markdown, markdown_parser_options()).into_offset_iter()
    {
        if let pulldown_cmark::Event::Html(html) | pulldown_cmark::Event::InlineHtml(html) = event
            && let Some(line_breaks) = html_line_breaks_replacement(html.as_ref())
        {
            normalized.push_str(&markdown[cursor..range.start]);
            normalized.push_str(&line_breaks);
            let mut next_cursor = range.end;

            if markdown[next_cursor..].starts_with("\r\n") {
                next_cursor += 2;
            } else if markdown[next_cursor..].starts_with('\n') {
                next_cursor += 1;
            }

            cursor = next_cursor;
        }
    }

    if cursor == 0 {
        return markdown.to_string();
    }

    if cursor < markdown.len() {
        normalized.push_str(&markdown[cursor..]);
    }

    normalized
}

pub(crate) fn html_line_breaks_replacement(html: &str) -> Option<String> {
    let mut cursor = 0usize;
    let mut count = 0usize;

    while cursor < html.len() {
        let remaining = &html[cursor..];
        let leading_ws = remaining.len() - remaining.trim_start().len();
        cursor += leading_ws;

        if cursor >= html.len() {
            break;
        }

        if !html[cursor..].starts_with('<') {
            return None;
        }

        let tag_end = html[cursor..].find('>')?;

        let tag = &html[cursor..cursor + tag_end + 1];
        if !is_html_line_break_tag(tag) {
            return None;
        }

        count += 1;
        cursor += tag_end + 1;
    }

    if count == 0 {
        None
    } else {
        Some(HTML_BR_SENTINEL.repeat(count))
    }
}

fn is_html_line_break_tag(html: &str) -> bool {
    let trimmed = html.trim();
    if !(trimmed.starts_with('<') && trimmed.ends_with('>')) {
        return false;
    }

    let mut inner = trimmed[1..trimmed.len() - 1].trim();
    if inner.starts_with('/') {
        return false;
    }

    if let Some(without_self_close) = inner.strip_suffix('/') {
        inner = without_self_close.trim();
    }

    let mut parts = inner.split_whitespace();
    let Some(tag_name) = parts.next() else {
        return false;
    };

    tag_name.eq_ignore_ascii_case("br")
}

pub(crate) fn markdown_parser_options() -> pulldown_cmark::Options {
    pulldown_cmark::Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | pulldown_cmark::Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_TASKLISTS
}
