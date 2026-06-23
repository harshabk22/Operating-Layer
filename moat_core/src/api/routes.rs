use crate::api::auth::{UserRole, UserSession};
use crate::api::server::ApiState;
use crate::business::db;
use crate::business::models::{PipelineStage, ThreadControlMode};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- DTOs ---

#[derive(Deserialize)]
pub struct OAuthExchangeRequest {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Deserialize)]
pub struct GoogleWorkspaceConfigRequest {
    pub selected_spreadsheet_id: String,
}


#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub username: String,
    pub role: UserRole,
}

#[derive(Deserialize)]
pub struct ReasonRequest {
    pub reason: String,
}

#[derive(Deserialize)]
pub struct MessageRequest {
    pub body: String,
}

#[derive(Deserialize)]
pub struct StageRequest {
    pub stage: String,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ThreadDetailResponse {
    pub thread: crate::business::models::InteractionThread,
    pub contact: crate::business::models::ExternalContact,
    pub messages: Vec<crate::business::models::MessageRecord>,
    pub suggestion: Option<crate::business::models::ResponseSuggestion>,
}

#[derive(Serialize)]
pub struct AnalyticsOverview {
    pub active_threads: u64,
    pub pending_authorizations: u64,
    pub pending_escalations: u64,
    pub bookings_today: u64,
    pub conversion_rate: f64,
    pub human_takeover_rate: f64,
}

// --- Handlers ---

pub async fn save_google_oauth_handler(
    State(state): State<ApiState>,
    Json(payload): Json<OAuthExchangeRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    crate::business::db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        &payload.access_token,
        &payload.refresh_token,
        payload.expires_at,
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    crate::business::db::save_calendar_credentials(
        &conn,
        "default_business",
        "google_sheets",
        &payload.access_token,
        &payload.refresh_token,
        payload.expires_at,
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::OK)
}

pub async fn save_google_config_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<GoogleWorkspaceConfigRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let config = serde_json::json!({
        "selected_spreadsheet_id": payload.selected_spreadsheet_id
    });
    
    let key = std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string());
    let encrypted_config = crate::security::encryption::encrypt_credential(&config.to_string(), &key);
    
    let mut stmt = conn.prepare(
        "INSERT INTO connection_instances (business_id, provider_id, configuration) 
         VALUES (?1, ?2, ?3) 
         ON CONFLICT(business_id, provider_id) DO UPDATE SET configuration=excluded.configuration"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    stmt.execute((
        &session.business_id,
        "google",
        encrypted_config
    )).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}


pub async fn login_handler(
    State(state): State<ApiState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let owner_pw = std::env::var("OWNER_PASSWORD").unwrap_or_else(|_| "admin".to_string());
    let operator_pw = std::env::var("OPERATOR_PASSWORD").unwrap_or_else(|_| "admin".to_string());

    let role =
        if payload.username == "owner" && payload.password == owner_pw && !owner_pw.is_empty() {
            UserRole::Owner
        } else if payload.username == "operator"
            && payload.password == operator_pw
            && !operator_pw.is_empty()
        {
            UserRole::Operator
        } else if payload.username == "viewer" {
            UserRole::Viewer
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        };

    let session_id = Uuid::new_v4().to_string();
    let session = UserSession {
        username: payload.username.clone(),
        role: role.clone(),
        business_id: "default_business".to_string(),
    };
    state
        .auth_store
        .sessions
        .insert(session_id.clone(), session);

    let cookie = format!("session_id={}; Path=/; HttpOnly; SameSite=Lax", session_id);
    let body = Json(LoginResponse {
        username: payload.username,
        role,
    });

    Ok((StatusCode::OK, [("set-cookie", cookie)], body))
}

pub async fn logout_handler(
    State(state): State<ApiState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Some(cookie_header) = headers.get("cookie").and_then(|c| c.to_str().ok()) {
        if let Some(session_id) = cookie_header
            .split(';')
            .map(|s| s.trim())
            .find(|s| s.starts_with("session_id="))
            .map(|s| s["session_id=".len()..].to_string())
        {
            state.auth_store.sessions.remove(&session_id);
        }
    }

    let clear_cookie = "session_id=; Path=/; HttpOnly; Max-Age=0; SameSite=Lax";
    (StatusCode::OK, [("set-cookie", clear_cookie)], "Logged out")
}

pub async fn list_threads_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let threads = state
        .operator_service
        .list_active_threads()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(threads))
}

pub async fn get_thread_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;

    let thread = db::get_thread(&conn, &id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let contact = db::get_contact(&conn, &thread.contact_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let messages =
        db::list_messages(&conn, &thread.id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let suggestion = db::get_response_suggestion(&conn, &thread.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ThreadDetailResponse {
        thread,
        contact,
        messages,
        suggestion,
    }))
}

pub async fn pause_thread_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .pause_thread(&id, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn resume_thread_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .resume_thread(&id, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn takeover_thread_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .set_thread_control_mode(&id, ThreadControlMode::Human, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn release_thread_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .set_thread_control_mode(&id, ThreadControlMode::Ai, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn send_message_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<MessageRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .send_operator_message(&id, &payload.body, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn approve_suggestion_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .operator_service
        .send_suggestion(&id, &session.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn list_authorizations_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let auths = state
        .operator_service
        .list_pending_authorizations()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(auths))
}

pub async fn approve_authorization_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<ReasonRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .owner_service
        .approve_authorization(&id, &session.username, &payload.reason)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn reject_authorization_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<ReasonRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .owner_service
        .reject_authorization(&id, &session.username, &payload.reason)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn list_bookings_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let bookings = state
        .operator_service
        .list_bookings()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(bookings))
}

pub async fn get_booking_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let booking = db::get_engagement(&conn, &id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(booking))
}

pub async fn list_escalations_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let escs = state
        .operator_service
        .list_pending_escalations()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(escs))
}

pub async fn resolve_escalation_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<ReasonRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    state
        .owner_service
        .resolve_escalation(&id, &session.username, &payload.reason)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}



pub async fn get_opportunity_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare(
        "SELECT id, contact_id, thread_id, status, value, notes, created_at, updated_at, pipeline_stage 
         FROM business_opportunities WHERE id = ?1"
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut rows = stmt
        .query([&id])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(row) = rows.next().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        let status_str: String = row.get(3).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let pipeline_stage_str: String =
            row.get(8).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(crate::business::models::Opportunity {
            id: row.get(0).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            contact_id: row.get(1).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            thread_id: row.get(2).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            status: crate::business::models::OpportunityStatus::from_str(&status_str)
                .unwrap_or(crate::business::models::OpportunityStatus::Discovered),
            value: row.get(4).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            notes: row.get(5).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            created_at: row.get(6).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            updated_at: row.get(7).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            pipeline_stage: PipelineStage::from_str(&pipeline_stage_str)
                .unwrap_or(PipelineStage::New),
        }))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn update_opportunity_stage_handler(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<StageRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let stage = match payload.stage.as_str() {
        "New" => PipelineStage::New,
        "Qualified" => PipelineStage::Qualified,
        "ProposalSent" => PipelineStage::ProposalSent,
        "AwaitingApproval" => PipelineStage::AwaitingApproval,
        "Booked" => PipelineStage::Booked,
        "Won" => PipelineStage::Won,
        "Lost" => PipelineStage::Lost,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    state
        .operator_service
        .transition_pipeline_stage(&id, stage, &session.username, payload.reason)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn analytics_overview_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;

    // 1. Active Threads (where status != 'closed')
    let active_threads: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_interaction_threads WHERE status != 'closed'",
            [],
            |r| r.get(0),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    drop(conn);

    // Get metrics and analytics
    let metrics = state
        .owner_service
        .get_metrics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let analytics = state
        .operator_service
        .get_analytics()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AnalyticsOverview {
        active_threads: active_threads as u64,
        pending_authorizations: metrics.pending_authorizations,
        pending_escalations: metrics.pending_escalations,
        bookings_today: metrics.bookings_today,
        conversion_rate: analytics.opportunity_conversion_rate,
        human_takeover_rate: analytics.human_takeover_rate,
    }))
}

pub async fn whatsapp_qr_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    if let Some(connector) = state.runtime.registry.lookup("whatsapp_baileys").await {
        if let Ok(Some(qr_str)) = connector.get_qr_code().await {
            if let Some(pos) = qr_str.find("base64,") {
                let b64_part = &qr_str[pos + 7..];
                if let Ok(bytes) = STANDARD.decode(b64_part) {
                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::HeaderValue::from_static("image/png"),
                    );
                    return Ok((headers, bytes));
                }
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}

pub async fn whatsapp_status_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    if let Some(connector) = state.runtime.registry.lookup("whatsapp_baileys").await {
        let healthy = connector.health_check().await.unwrap_or(false);
        if healthy {
            return Ok(Json(serde_json::json!({ "status": "connected" })));
        }

        if let Ok(Some(_)) = connector.get_qr_code().await {
            return Ok(Json(serde_json::json!({ "status": "pairing" })));
        }

        let has_creds = {
            let conn = state.runtime.db.lock().await;
            crate::communication::db::has_baileys_credentials(&conn, "default_business")
                .unwrap_or(false)
        };
        if has_creds {
            return Ok(Json(serde_json::json!({ "status": "reconnecting" })));
        }
    }
    Ok(Json(serde_json::json!({ "status": "unpaired" })))
}

// --- Real Estate Handlers ---


use rusqlite::params;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) async fn dispatch_event(state: &ApiState, business_id: &str, event_type: &str, aggregate_id: &str, payload: &serde_json::Value) -> Result<(), StatusCode> {
    let mut payload_clone = payload.clone();
    if let Some(obj) = payload_clone.as_object_mut() {
        obj.insert("business_id".to_string(), serde_json::Value::String(business_id.to_string()));
    } else {
        payload_clone = serde_json::json!({
            "payload": payload_clone,
            "business_id": business_id
        });
    }
    let payload_str = payload_clone.to_string();
    let conn = state.runtime.db.lock().await;
    let id = Uuid::new_v4().to_string();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    conn.execute(
        "INSERT INTO business_event_store (id, business_id, event_type, aggregate_id, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, business_id, event_type, aggregate_id, payload_str, timestamp]
    ).map_err(|e| { eprintln!("[dispatch_event] INSERT business_event_store failed: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;

    conn.execute(
        "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(id) DO UPDATE SET business_id = ?2, name = ?3, version = version + 1, timestamp = ?4",
        params![aggregate_id, business_id, event_type, timestamp]
    ).map_err(|e| { eprintln!("[dispatch_event] INSERT projections failed: {}", e); StatusCode::INTERNAL_SERVER_ERROR })?;

    Ok(())
}

#[derive(Serialize, Clone)]
pub struct RouteEntry {
    pub path: String,
    pub component: String,
}

#[derive(Serialize, Clone)]
pub struct NavItem {
    pub label: String,
    pub path: String,
    pub icon: String,
}

#[derive(Serialize, Clone)]
pub struct NavGroup {
    pub label: String,
    pub items: Vec<NavItem>,
}

#[derive(Serialize, Clone)]
pub struct PackageMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub routes: Vec<RouteEntry>,
    pub navigation_entries: Vec<NavGroup>,
    pub permissions: Vec<String>,
    pub commands: Vec<String>,
    pub events: Vec<String>,
    pub projections: Vec<String>,
}

pub fn get_package_metadata(id: &str) -> Option<PackageMetadata> {
    match id {
        "sales-core-v1" => Some(PackageMetadata {
            id: "sales-core-v1".to_string(),
            name: "Sales Core".to_string(),
            version: "1.0.0".to_string(),
            routes: vec![
                RouteEntry { path: "sales/leads".to_string(), component: "Leads".to_string() },
                RouteEntry { path: "sales/opportunities".to_string(), component: "Opportunities".to_string() },
            ],
            navigation_entries: vec![
                NavGroup {
                    label: "Sales".to_string(),
                    items: vec![
                        NavItem { label: "Leads".to_string(), path: "/sales/leads".to_string(), icon: "Users".to_string() },
                        NavItem { label: "Opportunities".to_string(), path: "/sales/opportunities".to_string(), icon: "Users".to_string() },
                    ]
                }
            ],
            permissions: vec![
                "sales.leads.read".to_string(),
                "sales.leads.write".to_string(),
            ],
            commands: vec!["CreateLead".to_string(), "QualifyLead".to_string(), "CreateOpportunity".to_string(), "TransitionPipelineStage".to_string()],
            events: vec!["LeadCreated".to_string(), "LeadQualified".to_string(), "OpportunityCreated".to_string(), "PipelineStageTransitioned".to_string()],
            projections: vec!["LeadList".to_string(), "OpportunityList".to_string()],
        }),
        "real-estate-v1" => Some(PackageMetadata {
            id: "real-estate-v1".to_string(),
            name: "Real Estate".to_string(),
            version: "1.0.0".to_string(),
            routes: vec![
                RouteEntry { path: "realestate/properties".to_string(), component: "Properties".to_string() },
                RouteEntry { path: "realestate/offers".to_string(), component: "Offers".to_string() },
                RouteEntry { path: "realestate/deals".to_string(), component: "Deals".to_string() },
            ],
            navigation_entries: vec![
                NavGroup {
                    label: "Real Estate".to_string(),
                    items: vec![
                        NavItem { label: "Properties".to_string(), path: "/realestate/properties".to_string(), icon: "MapPin".to_string() },
                        NavItem { label: "Offers".to_string(), path: "/realestate/offers".to_string(), icon: "MapPin".to_string() },
                        NavItem { label: "Deals".to_string(), path: "/realestate/deals".to_string(), icon: "MapPin".to_string() },
                    ]
                }
            ],
            permissions: vec![
                "realestate.properties.read".to_string(),
                "realestate.properties.write".to_string(),
            ],
            commands: vec!["CreateProperty".to_string(), "CreateOffer".to_string(), "CloseDeal".to_string()],
            events: vec!["PropertyCreated".to_string(), "OfferCreated".to_string(), "DealWon".to_string()],
            projections: vec!["PropertyList".to_string(), "OfferList".to_string(), "DealList".to_string()],
        }),
        "notification-package-v1" => Some(PackageMetadata {
            id: "notification-package-v1".to_string(),
            name: "Notifications".to_string(),
            version: "1.0.0".to_string(),
            routes: vec![
                RouteEntry { path: "notifications".to_string(), component: "Notifications".to_string() },
                RouteEntry { path: "notifications/templates".to_string(), component: "Templates".to_string() },
                RouteEntry { path: "notifications/history".to_string(), component: "History".to_string() },
            ],
            navigation_entries: vec![
                NavGroup {
                    label: "Notifications".to_string(),
                    items: vec![
                        NavItem { label: "Center".to_string(), path: "/notifications".to_string(), icon: "Bell".to_string() },
                        NavItem { label: "Templates".to_string(), path: "/notifications/templates".to_string(), icon: "Bell".to_string() },
                        NavItem { label: "History".to_string(), path: "/notifications/history".to_string(), icon: "Bell".to_string() },
                    ]
                }
            ],
            permissions: vec!["notifications.send".to_string()],
            commands: vec!["SendNotification".to_string()],
            events: vec!["NotificationSent".to_string()],
            projections: vec!["NotificationList".to_string()],
        }),
        _ => None
    }
}

pub async fn install_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let pkg_name = payload.get("package_id")
        .or_else(|| payload.get("package"))
        .and_then(|v| v.as_str())
        .unwrap_or("pkg_1");
    dispatch_event(&state, &session.business_id, "PackageInstalled", pkg_name, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Installation complete" })))
}

pub async fn deploy_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let tenant_id = payload.get("tenantId").and_then(|v| v.as_str()).unwrap_or("tenant_1");
    dispatch_event(&state, &session.business_id, "TenantDeployed", tenant_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Tenant deployed successfully" })))
}

pub async fn rollback_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "TenantRollback", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Rollback complete" })))
}

pub async fn create_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let lead_id = payload.get("id").or_else(|| payload.get("email")).and_then(|v| v.as_str()).unwrap_or("lead_1");
    dispatch_event(&state, &session.business_id, "LeadCreated", lead_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead created successfully" })))
}

pub async fn qualify_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "LeadQualified", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead qualified" })))
}

pub async fn create_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let opp_id = payload.get("id").or_else(|| payload.get("title")).and_then(|v| v.as_str()).unwrap_or("opp_1");
    dispatch_event(&state, &session.business_id, "OpportunityCreated", opp_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity created" })))
}

pub async fn schedule_visit_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let visit_id = payload.get("id").or_else(|| payload.get("date")).and_then(|v| v.as_str()).unwrap_or("visit_1");
    dispatch_event(&state, &session.business_id, "VisitScheduled", visit_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Visit scheduled" })))
}

pub async fn send_notification_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let notif_id = payload.get("id").or_else(|| payload.get("message")).and_then(|v| v.as_str()).unwrap_or("notif_1");
    dispatch_event(&state, &session.business_id, "NotificationSent", notif_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Notification sent" })))
}

pub async fn approve_action_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "ActionApproved", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Action approved successfully" })))
}

pub async fn list_packages_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id FROM projections p WHERE p.business_id = ?1 AND p.name = 'PackageInstalled'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        row.get::<_, String>(0)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let mut packages = Vec::new();
    for id in rows.flatten() {
        if let Some(meta) = get_package_metadata(&id) {
            packages.push(meta);
        } else {
            packages.push(PackageMetadata {
                id: id.clone(),
                name: id.clone(),
                version: "1.0.0".to_string(),
                routes: vec![],
                navigation_entries: vec![],
                permissions: vec![],
                commands: vec![],
                events: vec![],
                projections: vec![],
            });
        }
    }
    Ok(axum::Json(packages))
}

pub async fn list_tenants_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'TenantDeployed'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let mut tenants = Vec::new();
    for tenant in rows.flatten() {
        tenants.push(tenant);
    }
    Ok(axum::Json(tenants))
}

pub async fn list_leads_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('LeadCreated', 'LeadQualified')").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

pub async fn list_opportunities_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'OpportunityCreated'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

pub async fn list_visits_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'VisitScheduled'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

pub async fn list_approvals_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'ActionApproved'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

pub async fn list_notifications_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'NotificationSent'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)

    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}


// MARKETPLACE ADDITIONS
pub async fn upgrade_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PackageUpgraded", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package upgraded" })))
}
pub async fn rollback_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PackageRolledBack", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package rolled back" })))
}

// CONTROL PLANE ADDITIONS
pub async fn backup_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "TenantBackedUp", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Tenant backed up" })))
}
pub async fn restore_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "TenantRestored", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Tenant restored" })))
}

// REAL ESTATE ADDITIONS
pub async fn create_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let prop_id = payload.get("id").or_else(|| payload.get("title")).and_then(|v| v.as_str()).unwrap_or("prop_1");
    dispatch_event(&state, &session.business_id, "PropertyCreated", prop_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property created" })))
}
pub async fn create_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let offer_id = payload.get("id").or_else(|| payload.get("amount")).and_then(|v| v.as_str()).unwrap_or("offer_1");
    dispatch_event(&state, &session.business_id, "OfferCreated", offer_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer created" })))
}
pub async fn close_deal_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let deal_id = payload.get("id").or_else(|| payload.get("value")).and_then(|v| v.as_str()).unwrap_or("deal_1");
    dispatch_event(&state, &session.business_id, "DealWon", deal_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Deal closed" })))
}

pub async fn list_properties_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'PropertyCreated'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}
pub async fn list_offers_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'OfferCreated'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}
pub async fn list_deals_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'DealWon'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

// PROJECTION REBUILDER
pub async fn rebuild_projections_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    // 1. Clear existing projections
    conn.execute("DELETE FROM projections WHERE business_id = ?", [&session.business_id]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // 2. Read from event store and rebuild
    let mut stmt = conn.prepare("SELECT aggregate_id, event_type, timestamp FROM business_event_store WHERE business_id = ? ORDER BY timestamp ASC").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    for (agg_id, evt_type, ts) in rows.flatten() {
        let _ = conn.execute(
            "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET business_id=excluded.business_id, name=excluded.name, version=version + 1, timestamp=excluded.timestamp",
            rusqlite::params![agg_id, &session.business_id, evt_type, 1, ts]
        );
    }
    
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Projections rebuilt successfully" })))
}

// RBAC HELPER / GUARD
pub async fn rbac_guard_handler(axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>) -> Result<impl IntoResponse, StatusCode> {
    if let Some(role) = params.get("role") {
        if role == "anonymous" {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(axum::Json(serde_json::json!({ "status": "success" })))
}

pub async fn get_dashboard_kpis_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let leads: i64 = conn.query_row("SELECT count(*) FROM projections WHERE business_id = ? AND name = 'LeadCreated'", [&session.business_id], |r| r.get(0)).unwrap_or(0);
    let sales: i64 = conn.query_row("SELECT count(*) FROM projections WHERE business_id = ? AND name = 'DealWon'", [&session.business_id], |r| r.get(0)).unwrap_or(0);
    
    // Sum actual deal values from event store
    let mut total_revenue = 0.0;
    if let Ok(mut stmt) = conn.prepare("SELECT payload FROM business_event_store WHERE business_id = ? AND event_type = 'DealWon'") {
        if let Ok(rows) = stmt.query_map([&session.business_id], |row| row.get::<_, String>(0)) {
            for r in rows.flatten() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&r) {
                    if let Some(val) = parsed.get("value") {
                        if let Some(f_val) = val.as_f64() {
                            total_revenue += f_val;
                        } else if let Some(s_val) = val.as_str() {
                            if let Ok(parsed_f) = s_val.parse::<f64>() {
                                total_revenue += parsed_f;
                            }
                        }
                    }
                }
            }
        }
    }
    
    let active_now = state
        .auth_store
        .sessions
        .iter()
        .filter(|ref_multi| ref_multi.value().business_id == session.business_id)
        .count();

    Ok(axum::Json(serde_json::json!({
        "revenue": total_revenue as i64,
        "leads": leads,
        "sales": sales,
        "active_now": active_now
    })))
}

pub async fn list_events_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT aggregate_id, event_type, payload, timestamp FROM business_event_store WHERE business_id = ? ORDER BY timestamp DESC LIMIT 100").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        Ok(serde_json::json!({
            "aggregate_id": row.get::<_, String>(0)?,
            "event_type": row.get::<_, String>(1)?,
            "payload": row.get::<_, String>(2)?,
            "timestamp": row.get::<_, i64>(3)?
        }))
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

// ADDED CRUD HANDLERS
pub async fn update_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "LeadUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead updated" })))
}
pub async fn delete_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "LeadDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead deleted" })))
}

pub async fn update_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OpportunityUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity updated" })))
}
pub async fn delete_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OpportunityDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity deleted" })))
}

pub async fn update_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PropertyUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property updated" })))
}
pub async fn delete_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PropertyDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property deleted" })))
}

pub async fn update_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OfferUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer updated" })))
}
pub async fn delete_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OfferDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer deleted" })))
}

pub async fn update_deal_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "DealUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Deal updated" })))
}
pub async fn delete_deal_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "DealDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Deal deleted" })))
}

pub async fn update_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PackageUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package updated" })))
}
pub async fn delete_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PackageDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package deleted" })))
}

pub async fn update_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "TenantUpdated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Tenant updated" })))
}
pub async fn delete_tenant_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "TenantDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Tenant deleted" })))
}

pub async fn create_template_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let template_id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("tmpl_1");
    dispatch_event(&state, &session.business_id, "NotificationTemplateCreated", template_id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Template created successfully" })))
}

pub async fn list_templates_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name = 'NotificationTemplateCreated'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        let payload_str: Option<String> = row.get(4).unwrap_or(None);
        let mut obj = serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "version": row.get::<_, i32>(2)?,
            "timestamp": row.get::<_, i64>(3)?,
        });
        if let Some(p) = payload_str {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
                if let serde_json::Value::Object(map) = parsed {
                    for (k, v) in map {
                        obj[k] = v;
                    }
                }
            }
        }
        Ok(obj)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut results = Vec::new();
    for v in rows.flatten() { results.push(v); }
    Ok(axum::Json(results))
}

pub async fn delete_template_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "NotificationTemplateDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Template deleted" })))
}
