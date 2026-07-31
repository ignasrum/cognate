use cognate_engine::search::{SearchQueryError, parse_query};

#[test]
fn parses_text_filters_and_negation() {
    let query = parse_query("rust label:work path:projects/ -label:archive -draft").unwrap();

    assert_eq!(query.terms, vec!["rust"]);
    assert_eq!(query.label_filters, vec!["work"]);
    assert_eq!(query.path_filters, vec!["projects/"]);
    assert_eq!(query.excluded_labels, vec!["archive"]);
    assert_eq!(query.excluded_terms, vec!["draft"]);
}

#[test]
fn parses_phrases_and_updated_ranges() {
    let query = parse_query("\"rust api\" updated:2026-01-01..2026-12-31").unwrap();

    assert_eq!(query.phrases, vec!["rust api"]);
    assert_eq!(
        query.updated_range,
        Some(("2026-01-01".to_string(), "2026-12-31".to_string()))
    );
}

#[test]
fn rejects_unterminated_quotes_and_invalid_ranges() {
    assert_eq!(
        parse_query("\"unfinished"),
        Err(SearchQueryError::UnterminatedQuote)
    );
    assert!(matches!(
        parse_query("updated:2026-12-31..2026-01-01"),
        Err(SearchQueryError::InvalidDateRange(_))
    ));
}

#[test]
fn rejects_queries_with_too_many_terms() {
    let query = (0..33).map(|_| "term").collect::<Vec<_>>().join(" ");
    assert_eq!(parse_query(&query), Err(SearchQueryError::TooManyTerms));
}
