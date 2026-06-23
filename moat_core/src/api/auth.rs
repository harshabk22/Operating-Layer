use axum::{
    extract::Request as AxumRequest,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum UserRole {
    Owner,
    Operator,
    Viewer,
}

#[derive(Clone, Debug)]
pub struct UserSession {
    pub username: String,
    pub role: UserRole,
    pub business_id: String,
}

pub struct AuthStore {
    pub sessions: Arc<DashMap<String, UserSession>>,
}

impl Default for AuthStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }
}

/// Helper to check cookie for session_id and resolve the session.
pub fn get_session_from_cookie(
    cookie_header: &str,
    sessions: &DashMap<String, UserSession>,
) -> Option<UserSession> {
    cookie_header
        .split(';')
        .map(|s| s.trim())
        .find(|s| s.starts_with("session_id="))
        .map(|s| s["session_id=".len()..].to_string())
        .and_then(|session_id| sessions.get(&session_id).map(|ref_val| ref_val.clone()))
}

/// Middleware to enforce session authentication.
pub async fn require_auth(
    state_sessions: Arc<DashMap<String, UserSession>>,
    mut request: AxumRequest,
    next: Next,
) -> Result<Response, StatusCode> {
    let cookie_header = request
        .headers()
        .get(header::COOKIE)
        .and_then(|c| c.to_str().ok())
        .unwrap_or("");

    if let Some(session) = get_session_from_cookie(cookie_header, &state_sessions) {
        request.extensions_mut().insert(session);
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Middleware to enforce Owner role only.
pub async fn require_owner_role(request: AxumRequest, next: Next) -> Result<Response, StatusCode> {
    if let Some(session) = request.extensions().get::<UserSession>() {
        if session.role == UserRole::Owner {
            Ok(next.run(request).await)
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

/// Middleware to enforce RBAC rules.
pub async fn rbac_middleware(request: AxumRequest, next: Next) -> Result<Response, StatusCode> {
    // 1. Maintain compatibility with the original rbac query param checks (reject "anonymous").
    if let Some(query) = request.uri().query() {
        let query_pairs = url::form_urlencoded::parse(query.as_bytes());
        for (key, val) in query_pairs {
            if key == "role" && val == "anonymous" {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    // 2. If request is a state mutation (POST, PUT, PATCH, DELETE), verify the role is Owner/Operator.
    // Viewer role returns 403 Forbidden.
    let method = request.method().clone();
    if method == axum::http::Method::POST
        || method == axum::http::Method::PUT
        || method == axum::http::Method::PATCH
        || method == axum::http::Method::DELETE
    {
        if let Some(session) = request.extensions().get::<UserSession>() {
            if session.role == UserRole::Viewer {
                return Err(StatusCode::FORBIDDEN);
            }
        } else {
            // Under normal flow, rbac_middleware will run after require_auth, so session should exist.
            // But if it doesn't, we return UNAUTHORIZED.
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(next.run(request).await)
}

