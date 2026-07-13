use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DocumentMetrics {
    pub word_count: usize,
    pub character_count: usize,
    pub sentence_count: usize,
    pub estimated_reading_time_secs: u32,
    pub readability_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MetricsRegister {
    pub metrics_by_note: HashMap<String, DocumentMetrics>,
}

impl MetricsRegister {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_metrics_for_note(&mut self, note_path: &str, content: &str) {
        if content.trim().is_empty() {
            self.metrics_by_note.remove(note_path);
            return;
        }

        let character_count = content.chars().count();

        // Count words: split by whitespace and keep only tokens containing alphanumeric characters
        let words: Vec<&str> = content
            .split_whitespace()
            .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
            .collect();
        let word_count = words.len();

        if word_count == 0 {
            self.metrics_by_note.remove(note_path);
            return;
        }

        // Count sentences: split by ., !, ?
        let sentence_count = count_sentences(content).max(1);

        // Count syllables
        let mut total_syllables = 0;
        for word in &words {
            total_syllables += count_syllables(word);
        }

        // Flesch-Kincaid Reading Ease
        let words_per_sentence = word_count as f32 / sentence_count as f32;
        let syllables_per_word = total_syllables as f32 / word_count as f32;
        let readability_score = 206.835 - 1.015 * words_per_sentence - 84.6 * syllables_per_word;

        // Estimated reading time: 200 WPM, in seconds.
        let estimated_reading_time_secs = ((word_count as f32 / 200.0) * 60.0) as u32;

        self.metrics_by_note.insert(
            note_path.to_string(),
            DocumentMetrics {
                word_count,
                character_count,
                sentence_count,
                estimated_reading_time_secs,
                readability_score,
            },
        );
    }

    pub fn remove_note(&mut self, note_path: &str) {
        self.metrics_by_note.remove(note_path);
    }
}

fn count_sentences(content: &str) -> usize {
    let mut count = 0;
    let chars: Vec<char> = content.chars().collect();
    if chars.is_empty() {
        return 0;
    }

    for idx in 0..chars.len() {
        let c = chars[idx];
        if c == '.' || c == '!' || c == '?' {
            if idx + 1 == chars.len() || chars[idx + 1].is_whitespace() {
                count += 1;
            }
        }
    }
    count
}

fn count_syllables(word: &str) -> usize {
    let cleaned: String = word
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
        
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.is_empty() {
        return 0;
    }

    let mut count = 0;
    let mut in_vowel_group = false;
    let vowels = ['a', 'e', 'i', 'o', 'u', 'y'];

    for &c in &chars {
        if vowels.contains(&c) {
            if !in_vowel_group {
                count += 1;
                in_vowel_group = true;
            }
        } else {
            in_vowel_group = false;
        }
    }

    // Silent 'e' at the end adjustment
    if chars.last() == Some(&'e') && count > 1 {
        if chars.len() >= 2 && !vowels.contains(&chars[chars.len() - 2]) {
            count -= 1;
        }
    }

    count.max(1)
}
