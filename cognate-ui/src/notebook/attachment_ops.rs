use super::api_client::*;
use super::{SelectedBackend, selected};
use crate::notebook::NotebookError;

pub(crate) async fn upload_attachment(
    _notebook_path: String,
    note_path: String,
    bytes: Vec<u8>,
) -> Result<String, NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "upload attachment",
            "API backend is not selected",
        ));
    };
    let mut url = endpoint(&client, "/v1/attachments")?;
    url.query_pairs_mut().append_pair("note", &note_path);
    let response = authorized(client.client.post(url).body(bytes), &client)
        .header("content-type", "application/octet-stream")
        .send()
        .await
        .map_err(|error| {
            NotebookError::api("upload attachment", None, None, error.to_string(), true)
        })?;
    if !response.status().is_success() {
        return Err(api_response_error(response, "upload attachment").await);
    }
    Ok(response
        .json::<ApiAttachmentPayload>()
        .await
        .map_err(|error| api_error("upload attachment", error))?
        .rel_path)
}

pub(crate) async fn download_attachment(
    _notebook_path: String,
    rel_path: String,
) -> Result<Vec<u8>, NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "download attachment",
            "API backend is not selected",
        ));
    };
    let response = authorized(
        client.client.get(attachment_endpoint(&client, &rel_path)?),
        &client,
    )
    .send()
    .await
    .map_err(|error| {
        NotebookError::api("download attachment", None, None, error.to_string(), true)
    })?;
    if !response.status().is_success() {
        return Err(api_response_error(response, "download attachment").await);
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| api_error("download attachment", error))
}

pub(crate) async fn delete_attachment(
    _notebook_path: String,
    rel_path: String,
) -> Result<(), NotebookError> {
    let SelectedBackend::Api(client) = selected() else {
        return Err(api_error(
            "delete attachment",
            "API backend is not selected",
        ));
    };
    let get_response = authorized(
        client.client.get(attachment_endpoint(&client, &rel_path)?),
        &client,
    )
    .send()
    .await
    .map_err(|error| {
        NotebookError::api("delete attachment", None, None, error.to_string(), true)
    })?;
    if !get_response.status().is_success() {
        return Err(api_response_error(get_response, "delete attachment").await);
    }
    let revision = get_response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| api_error("delete attachment", "server response omitted ETag"))?;
    send_empty(
        authorized(
            client
                .client
                .delete(attachment_endpoint(&client, &rel_path)?)
                .header("if-match", revision),
            &client,
        ),
        "delete attachment",
    )
    .await
}
