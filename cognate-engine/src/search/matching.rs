use super::{SearchHighlight, query::ParsedSearchQuery};

pub(super) fn truncate_search_snippet(input: &str, max_chars: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= max_chars {
        input.to_string()
    } else {
        let mut truncated: String = input.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    }
}

pub(super) fn find_matching_content_snippet(
    content: &str,
    query: &ParsedSearchQuery,
) -> Option<(String, Vec<SearchHighlight>)> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();
        if query.phrases.iter().all(|phrase| lower.contains(phrase))
            && query.terms.iter().all(|term| lower.contains(term))
        {
            let snippet = truncate_search_snippet(trimmed, 120);
            return Some((snippet.clone(), highlight_ranges(&snippet, query)));
        }
    }

    None
}

pub(super) fn highlight_ranges(text: &str, query: &ParsedSearchQuery) -> Vec<SearchHighlight> {
    let lower = text.to_lowercase();
    let mut ranges = Vec::new();
    for term in query.terms.iter().chain(query.phrases.iter()) {
        let mut start = 0;
        while let Some(found) = lower[start..].find(term) {
            let start_index = start + found;
            ranges.push(SearchHighlight {
                start: text[..start_index].chars().count(),
                end: text[..start_index + term.len()].chars().count(),
            });
            start = start_index + term.len();
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<SearchHighlight> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}
