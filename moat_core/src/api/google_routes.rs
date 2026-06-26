use crate::api::auth::UserSession;
use crate::api::server::ApiState;
use crate::business::db;
use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize)]
pub struct ConnectResponse {
    pub url: String,
}

#[derive(Deserialize)]
pub struct OAuthConfigSaveRequest {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
}

#[derive(Serialize)]
pub struct OAuthConfigResponse {
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_secret_configured: bool,
}

pub async fn google_connect_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    let oauth_state = Uuid::new_v4().to_string();
    let nonce = Uuid::new_v4().to_string();
    let expires_at = Utc::now().timestamp() + 600;

    {
        let conn = state.runtime.db.lock().await;
        let _ = db::purge_expired_oauth_states(&conn);
        if let Err(e) = db::save_oauth_state(
            &conn,
            &oauth_state,
            &nonce,
            &session.business_id,
            expires_at,
        ) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    }

    let conn = state.runtime.db.lock().await;
    let config = db::read_google_config(&conn, &session.business_id).unwrap_or_default();

    let client_id = config.client_id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default());

    let redirect_uri = config.redirect_uri
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let base_url = std::env::var("MOAT_PUBLIC_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string());
            format!("{}/oauth/callback", base_url)
        });

    let scope = urlencoding::encode("https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/spreadsheets https://www.googleapis.com/auth/drive.readonly https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile");

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
        client_id, redirect_uri, scope, oauth_state
    );

    Json(ConnectResponse { url }).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
struct TokenResp {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

pub async fn oauth_callback_handler(
    State(state): State<ApiState>,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    let (_nonce, business_id) = {
        let conn = state.runtime.db.lock().await;
        match db::consume_oauth_state(&conn, &query.state) {
            Ok(Some(s)) => s,
            _ => return Redirect::to("/?error=invalid_state").into_response(),
        }
    };

    let (client_id, client_secret, redirect_uri) = {
        let conn = state.runtime.db.lock().await;
        let config = db::read_google_config(&conn, &business_id).unwrap_or_default();

        let cid = config.client_id
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default());

        let csec = config.client_secret
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default());

        let ruri = config.redirect_uri
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let base_url = std::env::var("MOAT_PUBLIC_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:8080".to_string());
                format!("{}/oauth/callback", base_url)
            });

        (cid, csec, ruri)
    };

    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", query.code.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await;

    let token_res = match res {
        Ok(r) if r.status().is_success() => r,
        _ => return Redirect::to("/?error=token_exchange_failed").into_response(),
    };

    let tokens: TokenResp = match token_res.json().await {
        Ok(t) => t,
        _ => return Redirect::to("/?error=invalid_token_response").into_response(),
    };

    let expires_at = Utc::now().timestamp() + tokens.expires_in;

    let profile_res = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&tokens.access_token)
        .send()
        .await;

    let (account_email, google_user_id) = match profile_res {
        Ok(r) if r.status().is_success() => {
            if let Ok(info) = r.json::<serde_json::Value>().await {
                let email = info
                    .get("email")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let id = info
                    .get("id")
                    .or_else(|| info.get("sub"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                (email, id)
            } else {
                ("Unknown".to_string(), "Unknown".to_string())
            }
        }
        _ => ("Unknown".to_string(), "Unknown".to_string()),
    };

    {
        let conn = state.runtime.db.lock().await;

        let old_refresh = db::load_calendar_credentials(&conn, &business_id, "google")
            .ok()
            .flatten()
            .map(|(_, r, _)| r)
            .unwrap_or_default();
        let final_refresh = tokens.refresh_token.unwrap_or(old_refresh);

        let _ = db::save_calendar_credentials(
            &conn,
            &business_id,
            "google",
            &tokens.access_token,
            &final_refresh,
            expires_at,
        );
        let _ = db::save_calendar_credentials(
            &conn,
            &business_id,
            "google_sheets",
            &tokens.access_token,
            &final_refresh,
            expires_at,
        );
        let _ = db::update_calendar_credentials_identity(
            &conn,
            &business_id,
            "google",
            &account_email,
            &google_user_id,
        );
        let _ = db::update_calendar_credentials_identity(
            &conn,
            &business_id,
            "google_sheets",
            &account_email,
            &google_user_id,
        );
    }

    Redirect::to("/").into_response()
}

pub async fn google_status_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    let conn = state.runtime.db.lock().await;

    let creds = db::load_calendar_credentials(&conn, &session.business_id, "google")
        .ok()
        .flatten();
    let identity = db::get_calendar_credentials_identity(&conn, &session.business_id, "google")
        .ok()
        .flatten();
    let config = db::read_google_config(&conn, &session.business_id).unwrap_or_default();

    if let Some((_, refresh_token, expires_at)) = creds {
        let now = chrono::Utc::now().timestamp();
        let health = if expires_at <= now && refresh_token.is_empty() {
            "Degraded"
        } else {
            "Good"
        };

        Json(json!({
            "status": "Connected",
            "health": health,
            "account": identity.map(|i| i.0).unwrap_or_else(|| "Unknown".to_string()),
            "last_sync": chrono::Utc::now().to_rfc3339(),
            "config": config,
            "calendar_selected": true,
            "spreadsheet_selected": config.selected_spreadsheet_id.is_some(),
            "worksheet_selected": true
        }))
        .into_response()
    } else {
        Json(json!({
            "status": "Disconnected",
            "health": "None",
            "account": "",
            "last_sync": "",
            "calendar_selected": false,
            "spreadsheet_selected": false,
            "worksheet_selected": false
        }))
        .into_response()
    }
}

pub async fn google_disconnect_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    let conn = state.runtime.db.lock().await;
    let _ = db::delete_calendar_credentials(&conn, &session.business_id, "google");
    let _ = db::delete_calendar_credentials(&conn, &session.business_id, "google_sheets");

    let _ = crate::api::routes::dispatch_event(
        &state,
        &session.business_id,
        "ConnectionDisconnected",
        "google",
        &serde_json::json!({}),
    ).await;

    Json(json!({"success": true})).into_response()
}

#[derive(Deserialize)]
pub struct ConfigSaveRequest {
    pub calendar_id: Option<String>,
    pub spreadsheet_id: Option<String>,
    pub worksheet_id: Option<String>,
}

pub async fn google_config_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(req): Json<ConfigSaveRequest>,
) -> impl IntoResponse {
    let conn = state.runtime.db.lock().await;
    let mut config = db::read_google_config(&conn, &session.business_id).unwrap_or_default();
    
    // Merge settings
    config.selected_spreadsheet_id = req.spreadsheet_id;

    let key = std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string());
    let config_str = serde_json::to_string(&config).unwrap_or_default();
    let encrypted_config = crate::security::encryption::encrypt_credential(&config_str, &key);

    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    let mut stmt = match conn.prepare(
        "INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) 
         ON CONFLICT(business_id, provider_id) DO UPDATE SET 
             configuration=excluded.configuration,
             updated_at=excluded.updated_at"
    ) {
        Ok(stmt) => stmt,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if stmt.execute((&id, &session.business_id, "google", encrypted_config, now, now)).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Json(json!({"success": true})).into_response()
}

pub async fn get_google_oauth_config_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    let conn = state.runtime.db.lock().await;
    let config = db::read_google_config(&conn, &session.business_id).unwrap_or_default();

    let response = OAuthConfigResponse {
        client_id: config.client_id,
        redirect_uri: config.redirect_uri,
        client_secret_configured: config.client_secret.is_some() && !config.client_secret.unwrap().is_empty(),
    };

    Json(response).into_response()
}

pub async fn save_google_oauth_config_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(req): Json<OAuthConfigSaveRequest>,
) -> impl IntoResponse {
    let conn = state.runtime.db.lock().await;
    let mut config = db::read_google_config(&conn, &session.business_id).unwrap_or_default();

    config.client_id = Some(req.client_id);
    config.redirect_uri = Some(req.redirect_uri);

    if let Some(secret) = req.client_secret {
        let trimmed = secret.trim();
        if !trimmed.is_empty() && trimmed != "********" {
            config.client_secret = Some(trimmed.to_string());
        }
    }

    // Purge old tokens on settings edit
    let _ = db::delete_calendar_credentials(&conn, &session.business_id, "google");
    let _ = db::delete_calendar_credentials(&conn, &session.business_id, "google_sheets");

    let key = std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string());
    let config_str = serde_json::to_string(&config).unwrap_or_default();
    let encrypted_config = crate::security::encryption::encrypt_credential(&config_str, &key);

    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    let mut stmt = match conn.prepare(
        "INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) 
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) 
         ON CONFLICT(business_id, provider_id) DO UPDATE SET 
             configuration=excluded.configuration,
             updated_at=excluded.updated_at"
    ) {
        Ok(stmt) => stmt,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if stmt.execute((&id, &session.business_id, "google", encrypted_config, now, now)).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Json(json!({"success": true})).into_response()
}

pub async fn google_resources_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> impl IntoResponse {
    let access_token = {
        let conn = state.runtime.db.lock().await;
        let creds = db::load_calendar_credentials(&conn, &session.business_id, "google")
            .ok()
            .flatten();
        if let Some((access, _refresh, _)) = creds {
            access
        } else {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "No credentials"})),
            )
                .into_response();
        }
    };

    let client = reqwest::Client::new();

    let cal_res = client
        .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
        .bearer_auth(&access_token)
        .send()
        .await;

    let mut cals = json!({});
    if let Ok(r) = cal_res {
        if let Ok(c) = r.json::<serde_json::Value>().await {
            cals = c;
        }
    }

    let drive_res = client.get("https://www.googleapis.com/drive/v3/files?q=mimeType='application/vnd.google-apps.spreadsheet'")
        .bearer_auth(&access_token)
        .send()
        .await;

    let mut sheets = json!({});
    if let Ok(r) = drive_res {
        if let Ok(s) = r.json::<serde_json::Value>().await {
            let files = s.get("files").cloned().unwrap_or(json!([]));
            // map them to ID and NAME
            if let Some(items) = files.as_array() {
                let mapped: Vec<_> = items
                    .iter()
                    .map(|i| json!({"id": i["id"], "summary": i["name"]}))
                    .collect();
                sheets = json!({"items": mapped});
            }
        }
    }

    Json(json!({
        "calendars": cals,
        "spreadsheets": sheets,
        "worksheets": { "items": [] }
    }))
    .into_response()
}
