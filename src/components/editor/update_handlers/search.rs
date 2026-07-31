use super::*;
use std::time::Duration;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::SearchQueryChanged(query) => {
            state.state.set_search_query(query);
            let generation = state.next_search_generation();
            if state.state.search_query().trim().is_empty() {
                state.state.set_search_results(Vec::new());
                return Task::none();
            }
            Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    generation
                },
                Message::SearchDebounced,
            )
        }
        Message::SearchDebounced(generation) => {
            if generation != state.search_generation {
                return Task::none();
            }
            let query = state.state.search_query().trim().to_string();
            if query.is_empty() {
                return Task::none();
            }
            state.state.begin_search();
            spawn_search_page_task(state, query, generation, false)
        }
        Message::RunSearch => {
            let generation = state.next_search_generation();
            let query = state.state.search_query().trim().to_string();
            if query.is_empty() || state.state.notebook_path().is_empty() {
                state.state.set_search_results(Vec::new());
                return Task::none();
            }
            state.state.begin_search();
            spawn_search_page_task(state, query, generation, false)
        }
        Message::SearchCompleted(generation, results) => {
            if generation == state.search_generation
                && !state.state.search_query().trim().is_empty()
            {
                state.state.set_search_results(results);
            }
            Task::none()
        }
        Message::SearchPageCompleted(generation, append, result) => {
            if generation != state.search_generation {
                return Task::none();
            }
            match result {
                Ok(page) => state.state.complete_search(page, append),
                Err(error) => state.state.fail_search(error.ui_message()),
            }
            Task::none()
        }
        Message::LoadMoreSearchResults => {
            if state.state.search_loading() {
                return Task::none();
            }
            let Some(cursor) = state.state.search_next_cursor().map(str::to_string) else {
                return Task::none();
            };
            let query = state.state.search_query().trim().to_string();
            let generation = state.search_generation;
            state.state.begin_search();
            spawn_search_page_task_with_cursor(state, query, generation, cursor, true)
        }
        Message::ClearSearch => {
            let _ = state.next_search_generation();
            state.state.clear_search();
            Task::none()
        }
        _ => unreachable!("search handler received invalid message"),
    }
}

fn spawn_search_page_task(
    state: &Editor,
    query: String,
    generation: u64,
    append: bool,
) -> Task<Message> {
    spawn_search_page_task_with_cursor(state, query, generation, String::new(), append)
}

fn spawn_search_page_task_with_cursor(
    state: &Editor,
    query: String,
    generation: u64,
    cursor: String,
    append: bool,
) -> Task<Message> {
    let notebook_path = state.state.notebook_path().to_string();
    Task::perform(
        async move {
            notebook::search_notes_page(
                notebook_path,
                query,
                25,
                (!cursor.is_empty()).then_some(cursor),
            )
            .await
        },
        move |result| Message::SearchPageCompleted(generation, append, result),
    )
}
