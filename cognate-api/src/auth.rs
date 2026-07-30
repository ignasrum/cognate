use crate::{error::ApiError, state::AppState};
use axum::{
    extract::{Request, State},
    http::{HeaderValue, header::AUTHORIZATION},
    middleware::Next,
    response::Response,
};
use subtle::ConstantTimeEq;

pub const CLIENT_KEY_PREFIX: &str = "cgnt_live_";

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AuthenticatedClient {
    pub id: String,
    pub client_name: String,
}

pub fn digest_secret(secret: &str) -> [u8; 32] {
    *blake3::hash(secret.as_bytes()).as_bytes()
}

pub fn generate_client_secret() -> Result<String, ApiError> {
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|error| ApiError::Config(error.to_string()))?;
    Ok(format!("{CLIENT_KEY_PREFIX}{}", encode_hex(&random)))
}

pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let values = request.headers().get_all(AUTHORIZATION);
    if values.iter().count() != 1 {
        return Err(ApiError::Unauthorized);
    }
    let value = values.iter().next().ok_or(ApiError::Unauthorized)?;
    let presented = bearer_value(value).ok_or(ApiError::Unauthorized)?;
    if presented.len() > 256 || !presented.starts_with(CLIENT_KEY_PREFIX) {
        return Err(ApiError::Unauthorized);
    }

    let digest = digest_secret(presented);
    let digest_text = encode_hex(&digest);
    let record = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, client_name, key_hash FROM clients WHERE key_hash = ? AND revoked_at IS NULL",
    )
    .bind(&digest_text)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let Some((id, client_name, stored_digest)) = record else {
        return Err(ApiError::Unauthorized);
    };
    if stored_digest
        .as_bytes()
        .ct_eq(digest_text.as_bytes())
        .unwrap_u8()
        != 1
    {
        return Err(ApiError::Unauthorized);
    }

    let _ = sqlx::query("UPDATE clients SET last_used_at = datetime('now') WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await;
    request
        .extensions_mut()
        .insert(AuthenticatedClient { id, client_name });
    Ok(next.run(request).await)
}

fn bearer_value(value: &HeaderValue) -> Option<&str> {
    let text = value.to_str().ok()?;
    let (scheme, key) = text.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("Bearer") && !key.is_empty() && !key.contains(' ') {
        Some(key)
    } else {
        None
    }
}

pub fn new_client_id() -> Result<String, ApiError> {
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| ApiError::Config(error.to_string()))?;
    Ok(encode_hex(&random))
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
