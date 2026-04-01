use super::*;

pub(super) fn handle(state: &mut Editor, message: Message) -> Task<Message> {
    match message {
        Message::SearchQueryChanged(query) => {
            state.state.set_search_query(query);
            let generation = state.next_search_generation();
            let query = state.state.search_query().trim().to_string();
            if query.trim().is_empty() {
                state.state.set_search_results(Vec::new());
                return Task::none();
            }

            spawn_search_task(state, query, generation)
        }
        Message::RunSearch => {
            let generation = state.next_search_generation();
            let query = state.state.search_query().trim().to_string();
            if query.is_empty() || state.state.notebook_path().is_empty() {
                state.state.set_search_results(Vec::new());
                return Task::none();
            }

            spawn_search_task(state, query, generation)
        }
        Message::SearchCompleted(generation, results) => {
            if generation == state.search_generation
                && !state.state.search_query().trim().is_empty()
            {
                state.state.set_search_results(results);
            }
            Task::none()
        }
        Message::ClearSearch => {
            let _ = state.next_search_generation();
            state.state.clear_search();
            Task::none()
        }
        _ => unreachable!("search handler received invalid message"),
    }
}

fn spawn_search_task(state: &Editor, query: String, generation: u64) -> Task<Message> {
    let notebook_path = state.state.notebook_path().to_string();
    let notes = state
        .note_explorer
        .notes
        .iter()
        .map(notebook::SearchNote::from)
        .collect::<Vec<notebook::SearchNote>>();
    Task::perform(
        async move { notebook::search_notes_with_snapshot(notebook_path, notes, query).await },
        move |results| Message::SearchCompleted(generation, results),
    )
}
