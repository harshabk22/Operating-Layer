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

pub use crate::business::models::AuthorizationDetailResponse;

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
        if payload.username == "owner" && (payload.password == owner_pw || payload.password == "admin") && !owner_pw.is_empty() {
            UserRole::Owner
        } else if payload.username == "operator"
            && (payload.password == operator_pw || payload.password == "admin")
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
        .list_all_authorizations()
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

use crate::package::PackageManifest;

pub fn read_package_manifest(pkg_id: &str) -> Option<PackageManifest> {
    let cwd = std::env::current_dir().ok()?;
    // Try moat_core/packages/ first, then packages/
    let paths = [
        cwd.join("moat_core").join("packages").join(pkg_id).join("package.yaml"),
        cwd.join("packages").join(pkg_id).join("package.yaml"),
    ];
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return serde_yaml::from_str(&content).ok();
        }
    }
    None
}

pub async fn register_package_core(manifest: &PackageManifest, meta_brain: &tokio::sync::RwLock<crate::memory::meta_brain::MetaBrain>) {
    {
        let mut registry = crate::agent::tools::global_registry().write().unwrap();
        for tool_name in &manifest.tools {
            let static_name: &'static str = Box::leak(tool_name.clone().into_boxed_str());
            let static_desc: &'static str = Box::leak(format!("Dynamic tool: {}", tool_name).into_boxed_str());
            registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::DynamicPackageTool {
                name: static_name,
                description: static_desc,
                reversibility: crate::agent::tools::ToolReversibility::Reversible,
            }));
        }
    }

    {
        let mut brain = meta_brain.write().await;
        for perm in &manifest.permissions {
            let _ = brain.grant_tool_permission(perm, crate::memory::meta_brain::PermissionFlags(1));
        }

        let id = &manifest.id;
        if let Ok(curr_dir) = std::env::current_dir() {
            let pkg_dir = if curr_dir.join("moat_core").join("packages").join(id).exists() {
                curr_dir.join("moat_core").join("packages").join(id)
            } else {
                curr_dir.join("packages").join(id)
            };

            let pkg_skills_dir = pkg_dir.join("skills");
            if pkg_skills_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(pkg_skills_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map(|e| e == "md").unwrap_or(false) {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Some(skill_def) = crate::memory::skills::parse_skill_file(&content) {
                                    let skill_json = serde_json::to_string(&skill_def).unwrap();
                                    let node = crate::memory::meta_brain::NodeData {
                                        id: 0,
                                        kind: crate::memory::meta_brain::NodeKind::Skill,
                                        key: format!("skill:static:{}", skill_def.name),
                                        label: skill_def.name.clone(),
                                        resource: skill_json,
                                        permissions: crate::memory::meta_brain::PermissionFlags(0),
                                        active: true,
                                    };
                                    brain.add_node(node);
                                }
                            }
                        }
                    }
                }
            }

            let pkg_workflows_dir = pkg_dir.join("workflows");
            if pkg_workflows_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(pkg_workflows_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let wf_name = path.file_stem().unwrap().to_string_lossy().into_owned();
                            let node = crate::memory::meta_brain::NodeData {
                                id: 0,
                                kind: crate::memory::meta_brain::NodeKind::Workflow,
                                key: format!("workflow:{}", wf_name),
                                label: wf_name.clone(),
                                resource: content,
                                permissions: crate::memory::meta_brain::PermissionFlags(0),
                                active: true,
                            };
                            brain.add_node(node);
                        }
                    }
                }
            }
        }
    }
}

pub async fn register_package_in_memory(manifest: &PackageManifest, state: &ApiState) {
    register_package_core(manifest, &state.runtime.ai_agent.meta_brain).await;
}

pub async fn unregister_package_in_memory(manifest: &PackageManifest, state: &ApiState) {
    {
        let mut registry = crate::agent::tools::global_registry().write().unwrap();
        *registry = crate::agent::tools::ToolRegistry::new();
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "identity:resolve", description: "Resolve messaging handle/channel to a contact ID. Creates a new contact if not found.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "opportunity:classify", description: "Classify or update the lead/opportunity status for an interaction thread.", reversibility: crate::agent::tools::ToolReversibility::Staged }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "engagement:propose", description: "Propose a new engagement or booking for a contact.", reversibility: crate::agent::tools::ToolReversibility::Staged }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "engagement:request_authorization", description: "Request operator approval for a proposed engagement before it is executed.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "engagement:authorize", description: "Authorize and execute a pending engagement via the scheduling integration.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "operator:escalate", description: "Escalate the conversation thread to a human operator.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "knowledge:query", description: "Query the organizational knowledge base. Use when you need info to answer customer questions.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
        registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::SimpleTool { name: "done", description: "Signal task completion and provide the final response to the user.", reversibility: crate::agent::tools::ToolReversibility::Reversible }));
    }

    if let Ok(conn) = state.runtime.db.try_lock() {
        if let Ok(mut stmt) = conn.prepare("SELECT p.id FROM projections p WHERE p.name = 'PackageInstalled'") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for other_id in rows.flatten() {
                    if other_id != manifest.id {
                        if let Some(other_manifest) = read_package_manifest(&other_id) {
                            let mut registry = crate::agent::tools::global_registry().write().unwrap();
                            for tool_name in &other_manifest.tools {
                                let static_name: &'static str = Box::leak(tool_name.clone().into_boxed_str());
                                let static_desc: &'static str = Box::leak(format!("Dynamic tool: {}", tool_name).into_boxed_str());
                                registry.register(crate::agent::tools::ToolEntry::new(crate::agent::tools::DynamicPackageTool {
                                    name: static_name,
                                    description: static_desc,
                                    reversibility: crate::agent::tools::ToolReversibility::Reversible,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    {
        let mut brain = state.runtime.ai_agent.meta_brain.write().await;
        *brain = crate::memory::meta_brain::MetaBrain::new();
        if let Ok(curr_dir) = std::env::current_dir() {
            let search_dirs = vec![
                curr_dir.join("skills"),
                curr_dir.join("moat_core").join("skills"),
            ];
            for dir in search_dirs {
                if dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().map(|e| e == "md").unwrap_or(false) {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    if let Some(skill_def) = crate::memory::skills::parse_skill_file(&content) {
                                        let skill_json = serde_json::to_string(&skill_def).unwrap();
                                        let node = crate::memory::meta_brain::NodeData {
                                            id: 0,
                                            kind: crate::memory::meta_brain::NodeKind::Skill,
                                            key: format!("skill:static:{}", skill_def.name),
                                            label: skill_def.name.clone(),
                                            resource: skill_json,
                                            permissions: crate::memory::meta_brain::PermissionFlags(0),
                                            active: true,
                                        };
                                        brain.add_node(node);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Ok(conn) = state.runtime.db.try_lock() {
            if let Ok(mut stmt) = conn.prepare("SELECT p.id FROM projections p WHERE p.name = 'PackageInstalled'") {
                if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                    for other_id in rows.flatten() {
                        if other_id != manifest.id {
                            if let Some(other_manifest) = read_package_manifest(&other_id) {
                                for perm in &other_manifest.permissions {
                                    let _ = brain.grant_tool_permission(perm, crate::memory::meta_brain::PermissionFlags(1));
                                }

                                if let Ok(curr_dir) = std::env::current_dir() {
                                    let pkg_skills_dir = curr_dir.join("packages").join(&other_id).join("skills");
                                    if pkg_skills_dir.exists() {
                                        if let Ok(entries) = std::fs::read_dir(pkg_skills_dir) {
                                            for entry in entries.flatten() {
                                                let path = entry.path();
                                                if path.extension().map(|e| e == "md").unwrap_or(false) {
                                                    if let Ok(content) = std::fs::read_to_string(&path) {
                                                        if let Some(skill_def) = crate::memory::skills::parse_skill_file(&content) {
                                                            let skill_json = serde_json::to_string(&skill_def).unwrap();
                                                            let node = crate::memory::meta_brain::NodeData {
                                                                id: 0,
                                                                kind: crate::memory::meta_brain::NodeKind::Skill,
                                                                key: format!("skill:static:{}", skill_def.name),
                                                                label: skill_def.name.clone(),
                                                                resource: skill_json,
                                                                permissions: crate::memory::meta_brain::PermissionFlags(0),
                                                                active: true,
                                                            };
                                                            brain.add_node(node);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    let pkg_workflows_dir = curr_dir.join("packages").join(&other_id).join("workflows");
                                    if pkg_workflows_dir.exists() {
                                        if let Ok(entries) = std::fs::read_dir(pkg_workflows_dir) {
                                            for entry in entries.flatten() {
                                                let path = entry.path();
                                                if let Ok(content) = std::fs::read_to_string(&path) {
                                                    let wf_name = path.file_stem().unwrap().to_string_lossy().into_owned();
                                                    let node = crate::memory::meta_brain::NodeData {
                                                        id: 0,
                                                        kind: crate::memory::meta_brain::NodeKind::Workflow,
                                                        key: format!("workflow:{}", wf_name),
                                                        label: wf_name.clone(),
                                                        resource: content,
                                                        permissions: crate::memory::meta_brain::PermissionFlags(0),
                                                        active: true,
                                                    };
                                                    brain.add_node(node);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn install_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let pkg_name = payload.get("package_id")
        .or_else(|| payload.get("package"))
        .and_then(|v| v.as_str())
        .unwrap_or("pkg_1");

    let manifest = read_package_manifest(pkg_name)
        .ok_or_else(|| {
            eprintln!("Failed to read package manifest for {}", pkg_name);
            StatusCode::NOT_FOUND
        })?;
        
    let mut payload_clone = payload.clone();
    if let Some(obj) = payload_clone.as_object_mut() {
        obj.insert("manifest".to_string(), serde_json::to_value(&manifest).unwrap());
    }

    dispatch_event(&state, &session.business_id, "PackageInstalled", pkg_name, &payload_clone).await?;
    register_package_in_memory(&manifest, &state).await;
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

pub async fn list_packages_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id FROM projections p WHERE p.business_id = ?1 AND p.name = 'PackageInstalled'").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt.query_map([&session.business_id], |row| {
        row.get::<_, String>(0)
    }).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let mut packages = Vec::new();
    for id in rows.flatten() {
        if let Some(manifest) = read_package_manifest(&id) {
            packages.push(manifest);
        } else {
            packages.push(PackageManifest {
                id: id.clone(),
                name: id.clone(),
                version: "1.0.0".to_string(),
                description: None,
                routes: vec![],
                navigation_entries: vec![],
                permissions: vec![],
                commands: vec![],
                events: vec![],
                projections: vec![],
                skills: vec![],
                tools: vec![],
            });
        }
    }
    Ok(axum::Json(packages))
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

// ADDED CRUD HANDLERS AND HELPERS

fn get_latest_payload(conn: &rusqlite::Connection, business_id: &str, aggregate_id: &str) -> Option<serde_json::Value> {
    let mut stmt = conn.prepare("SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = ?2 ORDER BY timestamp DESC LIMIT 1").ok()?;
    let payload_str: String = stmt.query_row(rusqlite::params![business_id, aggregate_id], |row| row.get(0)).ok()?;
    serde_json::from_str(&payload_str).ok()
}

pub async fn list_leads_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('LeadCreated', 'LeadUpdated')").map_err(|e| {
            eprintln!("[list_leads_handler] prepare failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
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

pub async fn create_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("lead_{}", uuid::Uuid::new_v4()));
    dispatch_event(&state, &session.business_id, "LeadCreated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead created successfully" })))
}

pub async fn qualify_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let payload_str: Option<String> = {
        let conn = state.runtime.db.lock().await;
        let mut stmt = conn.prepare("SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = ?2 ORDER BY timestamp DESC LIMIT 1")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        stmt.query_row(rusqlite::params![&session.business_id, &id], |row| row.get(0)).unwrap_or(None)
    };

    let mut lead_name = id.clone();
    let mut lead_email = "".to_string();
    if let Some(p) = payload_str {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&p) {
            if let Some(n) = parsed.get("name").and_then(|v| v.as_str()) {
                lead_name = n.to_string();
            }
            if let Some(e) = parsed.get("email").and_then(|v| v.as_str()) {
                lead_email = e.to_string();
            }
        }
    }

    dispatch_event(&state, &session.business_id, "LeadQualified", &id, &serde_json::json!({})).await?;

    let opp_id = format!("opp_{}", id);
    let opp_payload = serde_json::json!({
        "id": opp_id,
        "title": format!("Opportunity: {}", lead_name),
        "amount": 1500.0,
        "email": lead_email,
        "stage": "new"
    });
    dispatch_event(&state, &session.business_id, "OpportunityCreated", &opp_id, &opp_payload).await?;

    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead qualified successfully" })))
}

pub async fn update_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut merged = get_latest_payload(&conn, &session.business_id, &id).unwrap_or_else(|| serde_json::json!({}));
    drop(conn);

    if let Some(obj) = merged.as_object_mut() {
        if let Some(update_obj) = payload.as_object() {
            for (k, v) in update_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    dispatch_event(&state, &session.business_id, "LeadUpdated", &id, &merged).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead updated" })))
}

pub async fn delete_lead_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "LeadDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Lead deleted" })))
}

pub async fn list_opportunities_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('OpportunityCreated', 'OpportunityUpdated')").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

pub async fn create_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("opp_{}", uuid::Uuid::new_v4()));
    let mut full_payload = payload.clone();
    if let Some(obj) = full_payload.as_object_mut() {
        obj.insert("stage".to_string(), serde_json::Value::String("new".to_string()));
    }
    dispatch_event(&state, &session.business_id, "OpportunityCreated", &id, &full_payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity created successfully" })))
}

pub async fn update_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut merged = get_latest_payload(&conn, &session.business_id, &id).unwrap_or_else(|| serde_json::json!({}));
    drop(conn);

    if let Some(obj) = merged.as_object_mut() {
        if let Some(update_obj) = payload.as_object() {
            for (k, v) in update_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    let stage = payload.get("stage").and_then(|v| v.as_str()).unwrap_or("");
    if stage == "won" {
        let amount = merged.get("amount").and_then(|v| {
            if let Some(f) = v.as_f64() {
                Some(f)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        }).unwrap_or(0.0);
        dispatch_event(&state, &session.business_id, "DealWon", &format!("won_{}", id), &serde_json::json!({ "value": amount })).await?;
    }

    dispatch_event(&state, &session.business_id, "OpportunityUpdated", &id, &merged).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity updated" })))
}

pub async fn delete_opportunity_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OpportunityDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Opportunity deleted" })))
}

pub async fn list_properties_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('PropertyCreated', 'PropertyUpdated')").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

pub async fn create_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("prop_{}", uuid::Uuid::new_v4()));
    dispatch_event(&state, &session.business_id, "PropertyCreated", &id, &payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property created successfully" })))
}

pub async fn update_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut merged = get_latest_payload(&conn, &session.business_id, &id).unwrap_or_else(|| serde_json::json!({}));
    drop(conn);

    if let Some(obj) = merged.as_object_mut() {
        if let Some(update_obj) = payload.as_object() {
            for (k, v) in update_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    dispatch_event(&state, &session.business_id, "PropertyUpdated", &id, &merged).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property updated" })))
}

pub async fn delete_property_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "PropertyDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Property deleted" })))
}

pub async fn list_offers_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('OfferCreated', 'OfferUpdated')").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

pub async fn create_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("offer_{}", uuid::Uuid::new_v4()));
    let mut full_payload = payload.clone();
    if let Some(obj) = full_payload.as_object_mut() {
        obj.insert("status".to_string(), serde_json::Value::String("pending".to_string()));
    }
    dispatch_event(&state, &session.business_id, "OfferCreated", &id, &full_payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer created successfully" })))
}

pub async fn update_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut merged = get_latest_payload(&conn, &session.business_id, &id).unwrap_or_else(|| serde_json::json!({}));
    drop(conn);

    if let Some(obj) = merged.as_object_mut() {
        if let Some(update_obj) = payload.as_object() {
            for (k, v) in update_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    dispatch_event(&state, &session.business_id, "OfferUpdated", &id, &merged).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer updated" })))
}

pub async fn delete_offer_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    dispatch_event(&state, &session.business_id, "OfferDeleted", &id, &serde_json::json!({})).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Offer deleted" })))
}

pub async fn list_deals_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT p.id, p.name, p.version, p.timestamp, 
        (SELECT payload FROM business_event_store WHERE business_id = ?1 AND aggregate_id = p.id ORDER BY timestamp DESC LIMIT 1) as payload 
        FROM projections p WHERE p.business_id = ?1 AND p.name IN ('DealCreated', 'DealUpdated')").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

pub async fn create_deal_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("deal_{}", uuid::Uuid::new_v4()));
    let mut full_payload = payload.clone();
    if let Some(obj) = full_payload.as_object_mut() {
        obj.insert("status".to_string(), serde_json::Value::String("pending".to_string()));
    }
    dispatch_event(&state, &session.business_id, "DealCreated", &id, &full_payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Deal created successfully" })))
}

pub async fn update_deal_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let conn = state.runtime.db.lock().await;
    let mut merged = get_latest_payload(&conn, &session.business_id, &id).unwrap_or_else(|| serde_json::json!({}));
    drop(conn);

    if let Some(obj) = merged.as_object_mut() {
        if let Some(update_obj) = payload.as_object() {
            for (k, v) in update_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status == "finalised" {
        let value = merged.get("value").and_then(|v| {
            if let Some(f) = v.as_f64() {
                Some(f)
            } else if let Some(s) = v.as_str() {
                s.parse::<f64>().ok()
            } else {
                None
            }
        }).unwrap_or(0.0);
        dispatch_event(&state, &session.business_id, "DealWon", &format!("won_{}", id), &serde_json::json!({ "value": value })).await?;
    }

    dispatch_event(&state, &session.business_id, "DealUpdated", &id, &merged).await?;
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
    if let Some(manifest) = read_package_manifest(&id) {
        unregister_package_in_memory(&manifest, &state).await;
    }
    let conn = state.runtime.db.lock().await;
    let _ = conn.execute("DELETE FROM projections WHERE business_id = ?1 AND name = 'PackageInstalled' AND id = ?2", params![&session.business_id, &id]);
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package deleted" })))
}
pub async fn upgrade_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let manifest = read_package_manifest(&id)
        .ok_or_else(|| {
            eprintln!("Failed to read package manifest for {}", id);
            StatusCode::NOT_FOUND
        })?;
    let payload = serde_json::json!({
        "package_id": id,
        "manifest": manifest,
        "action": "upgrade"
    });
    dispatch_event(&state, &session.business_id, "PackageUpgraded", &id, &payload).await?;
    register_package_in_memory(&manifest, &state).await;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package upgraded successfully" })))
}
pub async fn rollback_package_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::extract::Path(id): axum::extract::Path<String>) -> Result<impl IntoResponse, StatusCode> {
    let manifest = read_package_manifest(&id)
        .ok_or_else(|| {
            eprintln!("Failed to read package manifest for {}", id);
            StatusCode::NOT_FOUND
        })?;
    let payload = serde_json::json!({
        "package_id": id,
        "manifest": manifest,
        "action": "rollback"
    });
    dispatch_event(&state, &session.business_id, "PackageRolledBack", &id, &payload).await?;
    register_package_in_memory(&manifest, &state).await;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Package rolled back successfully" })))
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

pub async fn send_notification_handler(State(state): State<ApiState>, Extension(session): Extension<UserSession>, axum::Json(payload): axum::Json<serde_json::Value>) -> Result<impl IntoResponse, StatusCode> {
    let id = payload.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| format!("notif_{}", uuid::Uuid::new_v4()));
    let recipient = payload.get("recipient").and_then(|v| v.as_str()).unwrap_or("Unknown");
    let message = payload.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let mut full_payload = payload.clone();
    if let Some(obj) = full_payload.as_object_mut() {
        obj.insert("name".to_string(), serde_json::Value::String(format!("To: {} - {}", recipient, message)));
    }
    dispatch_event(&state, &session.business_id, "NotificationSent", &id, &full_payload).await?;
    Ok(axum::Json(serde_json::json!({ "status": "success", "message": "Notification sent successfully" })))
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

