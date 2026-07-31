mod cache;
pub mod index;
mod maintenance;
pub mod manager;
mod matching;
pub mod query;
pub mod tokenizer;

pub use index::{DocId, InvertedIndex};
pub use manager::{
    SearchHighlight, SearchIndexManager, SearchMatchType, SearchRequest, SearchResponse,
    SearchResultEntry,
};
pub use query::{ParsedSearchQuery, SearchHit, SearchQueryError, execute_search, parse_query};
