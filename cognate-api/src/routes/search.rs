use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};

use crate::{api::SearchQuery, error::ApiError, state::AppState};
use cognate_engine::storage::NotebookManager;

pub(crate) async fn search(
    State(state): State<AppState>,
    Query(mut query): Query<SearchQuery>,
) -> Result<Json<Vec<cognate_engine::search::SearchResultEntry>>, ApiError> {
    query.limit = Some(query.limit.unwrap_or(100).min(100));
    let Json(response) = search_page(State(state), Query(query)).await?;
    Ok(Json(response.results))
}

pub(crate) async fn search_page(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<cognate_engine::search::SearchResponse>, ApiError> {
    if query.q.chars().count() > 256 {
        return Err(ApiError::Search {
            code: "invalid_query",
            detail: "query is too long".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::Search {
            code: "invalid_limit",
            detail: "limit must be between 1 and 100".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }
    if query
        .cursor
        .as_deref()
        .is_some_and(|cursor| cursor.len() > 80)
    {
        return Err(ApiError::Search {
            code: "invalid_cursor",
            detail: "cursor is too long".to_string(),
            status: StatusCode::BAD_REQUEST,
        });
    }
    let manager = NotebookManager::new(&state.notebook_path);
    let loaded = manager.load_metadata().await?;
    let manager = state.search_manager().await;
    let mut search = manager.lock().await;
    search
        .search_request(
            &cognate_engine::search::SearchRequest {
                query: query.q,
                limit,
                cursor: query.cursor,
            },
            &loaded.notes,
            std::time::Duration::from_secs(5),
        )
        .await
        .map(Json)
        .map_err(|error| match error {
            cognate_engine::EngineError::Validation { context, detail } => ApiError::Search {
                code: if context == "search cursor" {
                    "invalid_cursor"
                } else {
                    "invalid_query"
                },
                detail,
                status: StatusCode::BAD_REQUEST,
            },
            other => ApiError::Search {
                code: "search_internal",
                detail: other.to_string(),
                status: StatusCode::INTERNAL_SERVER_ERROR,
            },
        })
}
