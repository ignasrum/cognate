use super::index::{DocId, InvertedIndex};
use super::tokenizer::tokenize_string;
use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSearchQuery {
    pub text: String,
    pub terms: Vec<String>,
    pub phrases: Vec<String>,
    pub label_filters: Vec<String>,
    pub path_filters: Vec<String>,
    pub excluded_terms: Vec<String>,
    pub excluded_labels: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub updated_range: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SearchQueryError {
    #[error("search query is too long")]
    TooLong,
    #[error("search query contains an unterminated quote")]
    UnterminatedQuote,
    #[error("search query has too many terms")]
    TooManyTerms,
    #[error("invalid updated range: {0}")]
    InvalidDateRange(String),
}

pub fn parse_query(query: &str) -> Result<ParsedSearchQuery, SearchQueryError> {
    if query.chars().count() > 256 {
        return Err(SearchQueryError::TooLong);
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in query.trim().chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            character => current.push(character),
        }
    }
    if quoted {
        return Err(SearchQueryError::UnterminatedQuote);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.len() > 32 {
        return Err(SearchQueryError::TooManyTerms);
    }

    let mut parsed = ParsedSearchQuery {
        text: String::new(),
        terms: Vec::new(),
        phrases: Vec::new(),
        label_filters: Vec::new(),
        path_filters: Vec::new(),
        excluded_terms: Vec::new(),
        excluded_labels: Vec::new(),
        excluded_paths: Vec::new(),
        updated_range: None,
    };
    for token in tokens {
        let (negative, token) = token
            .strip_prefix('-')
            .map_or((false, token.as_str()), |value| (true, value));
        if token.is_empty() {
            continue;
        }
        if let Some(value) = token.strip_prefix("label:") {
            if !value.is_empty() {
                if negative {
                    parsed.excluded_labels.push(value.to_lowercase());
                } else {
                    parsed.label_filters.push(value.to_lowercase());
                }
            }
            continue;
        }
        if let Some(value) = token.strip_prefix("path:") {
            if !value.is_empty() {
                if negative {
                    parsed.excluded_paths.push(value.to_lowercase());
                } else {
                    parsed.path_filters.push(value.to_lowercase());
                }
            }
            continue;
        }
        if !negative && let Some(value) = token.strip_prefix("updated:") {
            if let Some(days) = value.strip_prefix("last-").and_then(|period| match period {
                "week" => Some(7),
                "month" => Some(30),
                _ => None,
            }) {
                let today = time::OffsetDateTime::now_utc().date();
                let from = today - time::Duration::days(days);
                let format_date = |date: time::Date| {
                    format!(
                        "{:04}-{:02}-{:02}",
                        date.year(),
                        date.month() as u8,
                        date.day()
                    )
                };
                parsed.updated_range = Some((format_date(from), format_date(today)));
                continue;
            }
            let Some((from, to)) = value.split_once("..") else {
                return Err(SearchQueryError::InvalidDateRange(value.to_string()));
            };
            if from.is_empty() || to.is_empty() || from > to {
                return Err(SearchQueryError::InvalidDateRange(value.to_string()));
            }
            parsed.updated_range = Some((from.to_string(), to.to_string()));
            continue;
        }

        let value = token.trim_matches('"').to_lowercase();
        if value.is_empty() || matches!(value.as_str(), "and" | "or" | "not") {
            continue;
        }
        if token.starts_with('"') && token.ends_with('"') {
            parsed.phrases.push(value.clone());
        } else if negative {
            parsed.excluded_terms.push(value.clone());
        } else {
            parsed.terms.push(value.clone());
        }
        if !parsed.text.is_empty() {
            parsed.text.push(' ');
        }
        if negative {
            parsed.text.push('-');
        }
        parsed.text.push_str(&value);
    }
    Ok(parsed)
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
}

pub fn execute_search(index: &InvertedIndex, query_str: &str, limit: usize) -> Vec<SearchHit> {
    let raw_tokens = query_str.split_whitespace().collect::<Vec<&str>>();
    if raw_tokens.is_empty() {
        return Vec::new();
    }

    let mut positive_terms = Vec::new();
    let mut negative_terms = Vec::new();

    let mut is_not_modifier = false;
    for token in raw_tokens {
        let cleaned = token.to_lowercase();
        if cleaned == "not" {
            is_not_modifier = true;
            continue;
        }
        if cleaned == "and" || cleaned == "or" {
            // Treat as structural keywords, skip direct indexing
            continue;
        }

        if cleaned.starts_with('-') && cleaned.len() > 1 {
            let stemmed = tokenize_string(&cleaned[1..]);
            negative_terms.extend(stemmed);
        } else if is_not_modifier {
            let stemmed = tokenize_string(&cleaned);
            negative_terms.extend(stemmed);
            is_not_modifier = false;
        } else {
            let stemmed = tokenize_string(&cleaned);
            positive_terms.extend(stemmed);
        }
    }

    if positive_terms.is_empty() {
        return Vec::new();
    }

    // Gather all documents matching positive terms
    let n_total_docs = index.documents.len() as f32;
    let avg_doc_len = index.get_avg_doc_length();

    // BM25 parameters
    let k1: f32 = 1.2;
    let b: f32 = 0.75;

    let mut doc_scores: HashMap<DocId, f32> = HashMap::new();

    // Map doc_id back to path for easier reference
    let id_to_meta: HashMap<DocId, &super::index::DocumentMetadata> = index
        .documents
        .values()
        .map(|meta| (meta.id, meta))
        .collect();

    for term in &positive_terms {
        let Some(postings) = index.postings.get(term) else {
            continue;
        };

        let n_matching_docs = postings.len() as f32;
        // Calculate IDF
        let idf = ((n_total_docs - n_matching_docs + 0.5) / (n_matching_docs + 0.5) + 1.0)
            .max(0.0001)
            .ln();

        for posting in postings {
            let doc_id = posting.doc_id;
            let doc_meta = match id_to_meta.get(&doc_id) {
                Some(meta) => meta,
                None => continue,
            };

            let tf = posting.term_frequency;
            let doc_len = doc_meta.length;

            let numerator = tf * (k1 + 1.0);
            let denominator = tf + k1 * (1.0 - b + b * (doc_len / avg_doc_len.max(1.0)));
            let term_score = idf * (numerator / denominator);

            *doc_scores.entry(doc_id).or_insert(0.0) += term_score;
        }
    }

    // Apply negative filters
    for term in &negative_terms {
        if let Some(postings) = index.postings.get(term) {
            for posting in postings {
                doc_scores.remove(&posting.doc_id);
            }
        }
    }

    // Map to SearchHits
    let mut hits: Vec<SearchHit> = doc_scores
        .into_iter()
        .filter_map(|(doc_id, score)| {
            id_to_meta.get(&doc_id).map(|meta| SearchHit {
                path: meta.path.clone(),
                score,
            })
        })
        .collect();

    // Sort by score descending
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);

    hits
}
