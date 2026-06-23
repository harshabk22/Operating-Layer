use crate::api::server::ApiState;
use crate::api::auth::UserSession;
use axum::{
    extract::{Path, State, Extension},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SetupStatusResponse {
    pub current_step: String,
    pub is_completed: bool,
}

pub async fn get_setup_status_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn
        .prepare("SELECT DISTINCT provider_id FROM connection_instances WHERE business_id = ?")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rows = stmt
        .query_map([&session.business_id], |row| row.get::<_, String>(0))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let configured_providers: Vec<String> = rows.filter_map(Result::ok).collect();

    let has_llm = configured_providers.contains(&"deepseek".to_string())
        || configured_providers.contains(&"openai".to_string());
    let has_messaging = configured_providers.contains(&"whatsapp".to_string());

    let needs_setup = !has_llm || !has_messaging;

    Ok(Json(serde_json::json!({
        "current_step": if needs_setup { "1_connections" } else { "completed" },
        "is_completed": !needs_setup,
        "needs_setup": needs_setup,
        "configured_providers": configured_providers
    })))
}

#[derive(Deserialize)]
pub struct ConfigureProviderRequest {
    pub provider_id: String,
    pub configuration: String,
}

pub async fn configure_provider_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<ConfigureProviderRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if payload.provider_id == "google_workspace" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use the Google Workspace connection flow.".to_string(),
        ));
    }

    let conn = state.runtime.db.lock().await;

    // Save/Insert the connection instance in the database
    let mut stmt = conn
        .prepare("INSERT OR REPLACE INTO connection_instances (business_id, provider_id, configuration) VALUES (?, ?, ?)")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    stmt.execute([&session.business_id, &payload.provider_id, &payload.configuration])
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Record event
    crate::business::event_store::record_event(
        &conn,
        "StateMutated",
        &session.business_id,
        &serde_json::json!({ "type": "LegacyMutationReplaced" })
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    Ok(Json(
        serde_json::json!({"status": "success", "connection_id": id}),
    ))
}

pub async fn test_connection_handler(
    State(_state): State<ApiState>,
    Path(_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // In reality we would look up the connection instance and test its API
    // Here we just return success
    Ok(Json(serde_json::json!({"status": "healthy"})))
}

pub async fn list_supervisor_tasks_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.runtime.db.lock().await;
    let tasks = crate::business::db::get_pending_operational_supervisor_tasks_by_business(&conn, &session.business_id, 50)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(tasks))
}

pub async fn list_providers_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.runtime.db.lock().await;
    let mut stmt = conn.prepare("SELECT business_id, provider_id, configuration FROM connection_instances WHERE business_id = ?").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let rows = stmt
        .query_map([&session.business_id], |row| {
            let provider_id = row.get::<_, String>(1).unwrap_or_default();
            Ok(serde_json::json!({
                "id": format!("{}-{}", session.business_id, provider_id),
                "provider_id": provider_id,
                "display_name": provider_id,
                "status": "connected",
                "health": "healthy",
                "last_success_at": 0,
                "last_checked_at": 0,
                "updated_at": 0,
            }))
        })
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut instances = Vec::new();
    for row in rows.flatten() {
        instances.push(row);
    }

    Ok(Json(serde_json::json!({ "providers": instances })))
}

pub async fn get_supervisor_chat_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let conn = state.runtime.db.lock().await;
    let msgs = crate::business::db::get_supervisor_chat_messages(&conn, &session.business_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(msgs))
}

#[derive(Deserialize)]
pub struct SupervisorChatRequest {
    pub message: String,
}

pub async fn post_supervisor_chat_handler(
    State(state): State<ApiState>,
    Extension(session): Extension<UserSession>,
    Json(payload): Json<SupervisorChatRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let user_msg = crate::business::db::SupervisorChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        business_id: session.business_id.clone(),
        role: "owner".to_string(),
        content: payload.message.clone(),
        created_at: chrono::Utc::now().timestamp(),
    };

    {
        let conn = state.runtime.db.lock().await;
        crate::business::db::insert_supervisor_chat_message(&conn, &user_msg)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Spawn a task to reply using LLM
    let runtime = state.runtime.clone();
    let biz_id = session.business_id.clone();
    let user_message_text = payload.message.clone();
    tokio::spawn(async move {
        let client = match crate::agent::llm::LlmClient::from_env() {
            Ok(c) => c,
            Err(e) => {
                let agent_msg = crate::business::db::SupervisorChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    business_id: biz_id,
                    role: "supervisor".to_string(),
                    content: format!("Error creating LLM client: {}", e),
                    created_at: chrono::Utc::now().timestamp(),
                };
                let conn = runtime.db.lock().await;
                let _ = crate::business::db::insert_supervisor_chat_message(&conn, &agent_msg);
                return;
            }
        };

        let mut messages = vec![];
        messages.push(crate::agent::llm::LlmMessage::system(
            "You are the Moat Operational Supervisor. You assist the business owner with system queries. Keep answers concise."
        ));

        {
            let conn = runtime.db.lock().await;
            if let Ok(history) =
                crate::business::db::get_supervisor_chat_messages(&conn, &biz_id)
            {
                for h in history.into_iter().take(10) {
                    let role = if h.role == "owner" {
                        "user".to_string()
                    } else {
                        "assistant".to_string()
                    };
                    messages.push(crate::agent::llm::LlmMessage {
                        role,
                        content: h.content,
                        tool_call_id: None,
                        tool_calls: None,
                    });
                }
            }
        }

        if messages.len() <= 1 {
            messages.push(crate::agent::llm::LlmMessage::user(user_message_text));
        }

        let response = match client.chat(messages, None).await {
            Ok(r) => r.choices.first().and_then(|c| c.message.content.clone()).unwrap_or_default(),
            Err(e) => format!("Error connecting to LLM: {}", e),
        };

        let agent_msg = crate::business::db::SupervisorChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            business_id: biz_id,
            role: "supervisor".to_string(),
            content: response,
            created_at: chrono::Utc::now().timestamp(),
        };

        let conn = runtime.db.lock().await;
        let _ = crate::business::db::insert_supervisor_chat_message(&conn, &agent_msg);
    });

    Ok(Json(serde_json::json!({"status": "success"})))
}
