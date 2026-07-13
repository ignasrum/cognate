use super::index::{DocId, InvertedIndex};
use super::tokenizer::tokenize_string;
use std::collections::HashMap;

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
