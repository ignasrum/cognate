mod common;

use std::time::Duration;

use cognate_engine::search::SearchIndexManager;
use cognate_engine::storage::{NoteMetadata, NotebookManager};
use common::{NotebookTestHarness, TempTestDir};

#[tokio::test]
async fn search_notes_finds_matches_in_path_label_and_content() {
    let harness = NotebookTestHarness::new("search_notes");
    let manager = harness.manager();
    let mut search_index = SearchIndexManager::new(harness.path());

    let mut notes: Vec<NoteMetadata> = Vec::new();
    manager
        .create_note("work/todo", &mut notes)
        .await
        .expect("Failed to create work/todo");
    manager
        .create_note("ideas/brainstorm", &mut notes)
        .await
        .expect("Failed to create ideas/brainstorm");

    if let Some(first_note) = notes.iter_mut().find(|note| note.rel_path == "work/todo") {
        first_note.labels.push("urgent".to_string());
    }

    std::fs::write(
        harness.path().join("ideas/brainstorm/note.md"),
        "Need to build an indexing strategy for search results.",
    )
    .expect("Failed to write brainstorm note content");

    let path_results = search_index
        .search("work", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(path_results.len(), 1);
    assert_eq!(path_results[0].rel_path, "work/todo");
    assert_eq!(path_results[0].snippet, "Path match");

    let label_results = search_index
        .search("urgent", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(label_results.len(), 1);
    assert_eq!(label_results[0].rel_path, "work/todo");
    assert!(
        label_results[0].snippet.contains("Label match"),
        "Expected snippet to indicate label match, got: {}",
        label_results[0].snippet
    );

    let content_results = search_index
        .search("indexing", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(content_results.len(), 1);
    assert_eq!(content_results[0].rel_path, "ideas/brainstorm");
    assert!(
        content_results[0]
            .snippet
            .to_lowercase()
            .contains("indexing"),
        "Expected snippet to include matching content"
    );
}

#[tokio::test]
async fn test_search_cache_operations() {
    let temp = TempTestDir::new("search_cache_ops");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    manager.create_note("note1", &mut notes).await.unwrap();
    manager.create_note("note2", &mut notes).await.unwrap();

    manager
        .save_note_content("note1", "rust language")
        .await
        .unwrap();
    manager
        .save_note_content("note2", "python programming")
        .await
        .unwrap();

    // Warm up cache
    let results = search_index
        .search("rust", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].rel_path, "note1");

    // Test upsert updates cache
    search_index.cache_upsert("note1", "rust programming language", None);

    let results = search_index
        .search("rust", &notes, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    // Test cache removal
    search_index.cache_remove("note1");
}

#[tokio::test]
async fn test_search_index_stale_refresh() {
    let temp = TempTestDir::new("search_stale_refresh");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();

    manager
        .save_note_content("note", "original text")
        .await
        .unwrap();

    // Query with zero refresh interval to force immediate reload
    let results = search_index
        .search("original", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    // Rewrite file externally
    let file_path = temp.path().join("note/note.md");
    tokio::fs::write(&file_path, "modified text").await.unwrap();

    // Immediate reload should pick up the new content
    let results = search_index
        .search("modified", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[tokio::test]
async fn test_search_cache_weird_queries() {
    let temp = TempTestDir::new("weird_queries");
    let mut search_index = SearchIndexManager::new(temp.path());

    // Search with empty or whitespace queries
    let results = search_index
        .search("", &[], Duration::from_secs(60))
        .await
        .unwrap();
    assert!(results.is_empty());

    let results = search_index
        .search("   ", &[], Duration::from_secs(60))
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_coverage_porter_stemmer_rules_and_snippet_truncation() {
    let temp = TempTestDir::new("stemmer_rules");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();

    let long_sentence = "This is a very long sentence designed specifically to trigger snippet truncation logic in the search manager. ".repeat(5);
    let content = format!("losses flies agreed creating. {}", long_sentence);
    manager.save_note_content("note", &content).await.unwrap();

    let search_terms = vec!["losses", "flies", "agreed", "creating"];
    for term in search_terms {
        let results = search_index
            .search(term, &notes, Duration::from_secs(0))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    let results = search_index
        .search("truncation", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].snippet.ends_with("..."),
        "Expected snippet to be truncated, got: {}",
        results[0].snippet
    );
}

#[tokio::test]
async fn test_atomic_bytes_write_rename_failure() {
    let temp = TempTestDir::new("bytes_rollback");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());
    let mut notes = Vec::new();
    manager.create_note("note", &mut notes).await.unwrap();

    // Verify index file was created and remove it
    let index_file = temp.path().join(".cognate_index.bin");
    assert!(index_file.exists());
    std::fs::remove_file(&index_file).unwrap();

    // Inject rename fault AFTER note creation is complete
    let failure_marker = temp.path().join(".cognate_fail_atomic_rename");
    std::fs::write(&failure_marker, "fail").unwrap();

    let file_path = temp.path().join("note/note.md");
    tokio::fs::write(&file_path, "modified text").await.unwrap();

    let _ = search_index
        .search("modified", &notes, Duration::from_secs(0))
        .await;
    assert!(!temp.path().join(".cognate_index.bin").exists());
}

#[tokio::test]
async fn test_search_cache_folder_rename_and_clear() {
    let temp = TempTestDir::new("cache_folder_rename");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());

    let mut notes = Vec::new();
    manager
        .create_note("folder/note-1", &mut notes)
        .await
        .unwrap();
    manager
        .create_note("folder/note-2", &mut notes)
        .await
        .unwrap();

    let _ = search_index
        .search("text", &notes, Duration::from_secs(60))
        .await;

    search_index.cache_rename("folder", "renamed");

    search_index.clear_cache();
}

#[tokio::test]
async fn test_search_negative_query_terms() {
    let temp = TempTestDir::new("search_neg_query");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());
    let mut notes = Vec::new();

    manager.create_note("note1", &mut notes).await.unwrap();
    manager
        .save_note_content("note1", "apple banana cherry")
        .await
        .unwrap();

    // Query with negative terms: should match note1 for fruit but filter out if negative term present
    let results = search_index
        .search("apple -cherry", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert!(
        results.is_empty(),
        "Expected no results since cherry is negative term, got: {:?}",
        results
    );

    let results = search_index
        .search("apple -date", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].rel_path, "note1");
}

#[tokio::test]
async fn test_search_missing_note_file_on_disk() {
    let temp = TempTestDir::new("search_missing_disk");
    let manager = NotebookManager::new(temp.path());
    let mut search_index = SearchIndexManager::new(temp.path());
    let mut notes = Vec::new();

    manager.create_note("note1", &mut notes).await.unwrap();
    manager
        .save_note_content("note1", "apple banana")
        .await
        .unwrap();

    // Delete the file from disk physically, but keep it in metadata
    let file_path = temp.path().join("note1/note.md");
    std::fs::remove_file(&file_path).unwrap();

    // Clear search cache so that it is forced to read from disk
    search_index.clear_cache();

    // Clear notes last_updated to force a refresh and index sync detection
    notes[0].last_updated = None;

    // Running search should skip reading the missing file and continue without crashing
    let results = search_index
        .search("apple", &notes, Duration::from_secs(0))
        .await
        .unwrap();
    assert!(results.is_empty());
}
