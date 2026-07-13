use pulldown_cmark::{Event, Options, Parser, Tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TermContext {
    Body,
    Header,
    Label,
}

pub fn tokenize_markdown(content: &str, labels: &[String]) -> Vec<(String, TermContext)> {
    let mut tokens = Vec::new();

    // 1. Process labels first
    for label in labels {
        for word in tokenize_string(label) {
            tokens.push((word, TermContext::Label));
        }
    }

    // 2. Parse Markdown
    let mut options = Options::empty();
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, options);
    let mut in_heading = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
            }
            Event::End(pulldown_cmark::TagEnd::Heading(..)) => {
                in_heading = false;
            }
            Event::Text(text) | Event::Code(text) => {
                let context = if in_heading {
                    TermContext::Header
                } else {
                    TermContext::Body
                };
                for word in tokenize_string(&text) {
                    tokens.push((word, context));
                }
            }
            _ => {}
        }
    }

    tokens
}

pub fn tokenize_string(input: &str) -> Vec<String> {
    input
        .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| stem_word(&s))
        .collect()
}

// Simple rule-based English suffix stemmer
fn stem_word(word: &str) -> String {
    if word.len() <= 3 {
        return word.to_string();
    }

    let mut stemmed = word.to_string();

    // Plurals / endings
    if stemmed.ends_with("sses") {
        stemmed.truncate(stemmed.len() - 2); // sses -> ss
    } else if stemmed.ends_with("ies") {
        stemmed.truncate(stemmed.len() - 3);
        stemmed.push('i'); // ies -> i
    } else if stemmed.ends_with("ss") {
        // Do nothing
    } else if stemmed.ends_with('s')
        && !stemmed.ends_with("us")
        && !stemmed.ends_with("is")
        && !stemmed.ends_with("as")
    {
        stemmed.pop(); // plurals s ->
    }

    // Gerunds and past tense
    if stemmed.ends_with("eed") {
        if stemmed.len() > 4 {
            stemmed.pop(); // eed -> ee (agreed -> agree)
        }
    } else if stemmed.ends_with("ing") {
        stemmed.truncate(stemmed.len() - 3);
        if stemmed.ends_with("at") || stemmed.ends_with("bl") || stemmed.ends_with("iz") {
            stemmed.push('e'); // ing -> e (creating -> create)
        }
    } else if stemmed.ends_with("ed") {
        stemmed.truncate(stemmed.len() - 2);
        if stemmed.ends_with("at") || stemmed.ends_with("bl") || stemmed.ends_with("iz") {
            stemmed.push('e');
        }
    }

    stemmed
}
