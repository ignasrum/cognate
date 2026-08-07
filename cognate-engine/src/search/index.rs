use super::tokenizer::{TermContext, tokenize_markdown};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

    pub fn index_document(
        &mut self,
        path: &str,
        content: &str,
        labels: &[String],
        last_updated: Option<String>,
    ) {
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

    /// Remap document paths without changing document IDs or token postings.
    ///
    /// A move changes where a document is addressed, but not its content. Keeping
    /// the existing IDs avoids the remove-and-reindex work required for a folder
    /// move.
    pub fn rename_paths(&mut self, from_rel: &str, to_rel: &str) {
        let from_prefix = format!("{from_rel}/");
        let to_prefix = format!("{to_rel}/");
        let paths = self.documents.keys().cloned().collect::<Vec<_>>();
        let mut remapped = Vec::new();

        for path in paths {
            let target = if path == from_rel {
                Some(to_rel.to_string())
            } else if path.starts_with(&from_prefix) {
                Some(format!("{to_prefix}{}", &path[from_prefix.len()..]))
            } else {
                None
            };
            if let Some(target) = target {
                remapped.push((path, target));
            }
        }

        for (old_path, new_path) in remapped {
            if let Some(mut metadata) = self.documents.remove(&old_path) {
                metadata.path = new_path.clone();
                self.documents.insert(new_path, metadata);
            }
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
