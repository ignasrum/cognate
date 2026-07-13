use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use super::tokenizer::{tokenize_markdown, TermContext};

pub type DocId = u32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Posting {
    pub doc_id: DocId,
    pub term_frequency: f32, // Weighted term frequency
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentMetadata {
    pub id: DocId,
    pub path: String,
    pub length: f32, // Weighted length
    pub labels: Vec<String>,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct InvertedIndex {
    pub postings: HashMap<String, Vec<Posting>>,
    pub documents: HashMap<String, DocumentMetadata>, // Key: document relative path
    pub next_doc_id: DocId,
}

impl InvertedIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn index_document(&mut self, path: &str, content: &str, labels: &[String], last_updated: Option<String>) {
        // Remove existing version of this document first
        self.remove_document(path);

        let doc_id = self.next_doc_id;
        self.next_doc_id += 1;

        let tokens = tokenize_markdown(content, labels);
        let mut term_counts: HashMap<String, f32> = HashMap::new();
        let mut total_weighted_length = 0.0;

        for (token, context) in tokens {
            let weight = match context {
                TermContext::Body => 1.0,
                TermContext::Header => 2.5,
                TermContext::Label => 4.0,
            };
            *term_counts.entry(token).or_insert(0.0) += weight;
            total_weighted_length += weight;
        }

        // Add to postings
        for (term, freq) in term_counts {
            self.postings.entry(term).or_default().push(Posting {
                doc_id,
                term_frequency: freq,
            });
        }

        // Save metadata
        self.documents.insert(
            path.to_string(),
            DocumentMetadata {
                id: doc_id,
                path: path.to_string(),
                length: total_weighted_length,
                labels: labels.to_vec(),
                last_updated,
            },
        );
    }

    pub fn remove_document(&mut self, path: &str) {
        if let Some(doc_meta) = self.documents.remove(path) {
            let target_doc_id = doc_meta.id;
            // Clean up postings
            for postings_list in self.postings.values_mut() {
                postings_list.retain(|p| p.doc_id != target_doc_id);
            }
            // Retain only non-empty posting entries to save memory
            self.postings.retain(|_, v| !v.is_empty());
        }
    }

    pub fn get_avg_doc_length(&self) -> f32 {
        if self.documents.is_empty() {
            return 0.0;
        }
        let total_length: f32 = self.documents.values().map(|doc| doc.length).sum();
        total_length / self.documents.len() as f32
    }
}
