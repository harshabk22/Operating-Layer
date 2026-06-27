use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use rusqlite::params;
use crate::AppState;

pub struct AdminUser {
    pub username: String,
    pub role: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        
        // Extract Authorization header
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Missing Authorization header" })),
                )
                    .into_response()
            })?;

        if !auth_header.starts_with("Bearer ") {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Authorization header must start with Bearer" })),
            )
                .into_response());
        }

        let token = &auth_header["Bearer ".len()..];
        
        let claims = crate::crypto::verify_jwt(token, &app_state.jwt_secret).map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": format!("Invalid token: {}", e) })),
            )
                .into_response()
        })?;

        Ok(AdminUser {
            username: claims.sub,
            role: claims.role,
        })
    }
}

pub struct RuntimeClient {
    pub customer_id: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for RuntimeClient
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let api_key = parts
            .headers
            .get("X-API-Key")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Missing X-API-Key header" })),
                )
                    .into_response()
            })?;

        let key_hash = crate::crypto::hash_sha256(api_key);
        
        let conn = app_state.db.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT customer_id FROM customer_runtimes WHERE api_key_hash = ?").map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Database error"}))).into_response()
        })?;
        
        let customer_id = stmt.query_row([&key_hash], |row| row.get::<_, String>(0)).map_err(|_| {
            (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid X-API-Key" }))).into_response()
        })?;

        // Update last_heartbeat_at
        let now = chrono::Utc::now().to_rfc3339();
        let _ = conn.execute(
            "UPDATE customer_runtimes SET last_heartbeat_at = ? WHERE api_key_hash = ?",
            params![now, key_hash],
        );

        Ok(RuntimeClient { customer_id })
    }
}

// Axum FromRef helper trait
pub trait FromRef<T> {
    fn from_ref(input: &T) -> Self;
}

impl FromRef<AppState> for AppState {
    fn from_ref(input: &AppState) -> Self {
        input.clone()
    }
}
