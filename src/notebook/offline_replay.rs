use super::api_client::*;
use super::{ApiClient, SelectedBackend, selected};
use crate::notebook::NotebookError;
use crate::notebook::offline_queue::{self, QueueStatus};

pub(super) fn revision_prefix(revision: Option<&str>) -> String {
    revision.unwrap_or("none").chars().take(12).collect()
}

pub(crate) async fn replay_offline_queue() -> Result<(), NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Ok(());
    };
    let _write_guard = client
        .write_coordinator
        .begin("__offline_queue__")
        .acquire()
        .await;
    replay_queued_writes(&client).await
}

pub(super) async fn replay_queued_writes(client: &ApiClient) -> Result<(), NotebookError> {
    let entries = offline_queue::read(&client.queue_path)
        .map_err(|error| api_error("offline queue", error))?;
    if entries.is_empty() {
        return Ok(());
    }

    let mut remaining = Vec::new();
    let now = offline_queue::now_seconds();
    for mut entry in entries {
        if entry.status == QueueStatus::Conflict {
            eprintln!(
                "[cognate] offline_write_conflict_paused note={}",
                entry.rel_path
            );
            remaining.push(entry);
            continue;
        }
        if entry.next_retry_at.is_some_and(|retry_at| retry_at > now) {
            remaining.push(entry);
            continue;
        }
        let url = note_endpoint(client, &entry.rel_path)?;
        let mut request = authorized(client.client.put(url).body(entry.content.clone()), client);
        if let Some(revision) = entry.expected_revision.as_deref() {
            request = request.header("if-match", format!("\"{revision}\""));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                eprintln!(
                    "[cognate] offline_write_transport_error note={}",
                    entry.rel_path
                );
                entry.retry_count = entry.retry_count.saturating_add(1);
                entry.next_retry_at =
                    Some(now.saturating_add(offline_queue::retry_delay_seconds(entry.retry_count)));
                remaining.push(entry);
                offline_queue::write(&client.queue_path, &remaining)
                    .map_err(|queue_error| api_error("offline queue", queue_error))?;
                return Err(api_error("offline queue retry", error));
            }
        };
        if response.status().is_success() {
            if let Some(revision) = response
                .headers()
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                client
                    .revisions
                    .lock()
                    .unwrap()
                    .insert(entry.rel_path.clone(), revision);
            }
            if let Some(metadata_revision) = response
                .headers()
                .get("x-metadata-etag")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_matches('"').to_string())
            {
                *client.metadata_revision.lock().unwrap() = Some(metadata_revision);
            }
            eprintln!("[cognate] offline_write_replayed note={}", entry.rel_path);
        } else {
            if response.status() == reqwest::StatusCode::CONFLICT {
                let status = response.status();
                if let Ok(conflict) = response.json::<ApiConflict>().await {
                    eprintln!(
                        "[cognate] offline_write_conflict note={} server_revision={}",
                        entry.rel_path,
                        revision_prefix(Some(&conflict.current_revision))
                    );
                    entry.status = QueueStatus::Conflict;
                    entry.next_retry_at = None;
                    remaining.push(entry.clone());
                    offline_queue::write(&client.queue_path, &remaining)
                        .map_err(|error| api_error("offline queue", error))?;
                    return Err(NotebookError::conflict_for_note(
                        "offline write",
                        entry.rel_path,
                        entry.content,
                        conflict.current_content,
                        conflict.current_revision,
                    ));
                }
                remaining.push(entry);
                offline_queue::write(&client.queue_path, &remaining)
                    .map_err(|error| api_error("offline queue", error))?;
                return Err(api_error(
                    "offline queue retry",
                    format!("server returned HTTP {status}"),
                ));
            }
            eprintln!(
                "[cognate] offline_write_failed note={} status={}",
                entry.rel_path,
                response.status()
            );
            entry.retry_count = entry.retry_count.saturating_add(1);
            entry.next_retry_at =
                Some(now.saturating_add(offline_queue::retry_delay_seconds(entry.retry_count)));
            remaining.push(entry);
        }
    }
    offline_queue::write(&client.queue_path, &remaining)
        .map_err(|error| api_error("offline queue", error))
}
