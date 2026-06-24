#![allow(dead_code)]
use crate::agent::approval::{ApprovalDecision, ApprovalFlow, ApprovalRequest};
use crate::agent::capabilities::Capability;
use crate::agent::consent::ConsentBridge;
use crate::agent::execution::journal::ExecutionJournal;
use crate::agent::execution::{DelExecutor, ExecutionError, ExecutionRequest};
use crate::memory::meta_brain::{MetaBrain, PermissionFlags};
use crate::session::SessionStore;
use anyhow::Result;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct ToolExecutor {
    pub meta_brain: Arc<Mutex<MetaBrain>>,
    pub agent_id: String,
    pub session_store: Arc<SessionStore>,
    pub del: DelExecutor,
    pub journal: ExecutionJournal,
    pub auto_deny: bool,
    pub consent_bridge: Option<Arc<ConsentBridge>>,
    pub async_brain: Arc<tokio::sync::RwLock<MetaBrain>>,
}

impl ToolExecutor {
    async fn sync_del_brain(&self) {
        let snapshot = self.meta_brain.lock().unwrap().snapshot();
        let mut del_brain = self.async_brain.write().await;
        del_brain.restore_from_snapshot(snapshot);
    }

    pub fn new(
        meta_brain: Arc<Mutex<MetaBrain>>,
        agent_id: impl Into<String>,
        session_store: Arc<SessionStore>,
    ) -> Result<Self> {
        Self::new_with_auto_deny(meta_brain, agent_id, session_store, false)
    }

    pub fn new_with_auto_deny(
        meta_brain: Arc<Mutex<MetaBrain>>,
        agent_id: impl Into<String>,
        session_store: Arc<SessionStore>,
        auto_deny: bool,
    ) -> Result<Self> {
        Self::build(meta_brain, agent_id, session_store, auto_deny, None)
    }

    pub fn new_with_consent(
        meta_brain: Arc<Mutex<MetaBrain>>,
        agent_id: impl Into<String>,
        session_store: Arc<SessionStore>,
        consent_bridge: Arc<ConsentBridge>,
    ) -> Result<Self> {
        Self::build(
            meta_brain,
            agent_id,
            session_store,
            false,
            Some(consent_bridge),
        )
    }

    fn build(
        meta_brain: Arc<Mutex<MetaBrain>>,
        agent_id: impl Into<String>,
        session_store: Arc<SessionStore>,
        auto_deny: bool,
        consent_bridge: Option<Arc<ConsentBridge>>,
    ) -> Result<Self> {
        let snapshot = meta_brain.lock().unwrap().snapshot();
        let mut sync_brain = MetaBrain::new();
        sync_brain.restore_from_snapshot(snapshot);
        let async_brain = Arc::new(tokio::sync::RwLock::new(sync_brain));
        let del = DelExecutor::new(
            Arc::clone(&async_brain),
            consent_bridge
                .clone()
                .unwrap_or_else(|| Arc::new(ConsentBridge::new(Box::new(|_req| {})))),
        )?;
        let journal = ExecutionJournal::open().unwrap_or_else(|e| {
            eprintln!("[ToolExecutor] Journal unavailable: {}", e);
            ExecutionJournal {
                path: std::path::PathBuf::new(),
                denial_counts: std::sync::Arc::new(std::sync::Mutex::new(
                    std::collections::HashMap::new(),
                )),
            }
        });
        Ok(Self {
            meta_brain,
            agent_id: agent_id.into(),
            session_store,
            del,
            journal,
            auto_deny,
            consent_bridge,
            async_brain,
        })
    }

    pub async fn execute(&self, tool_name: &str, args_json: &str) -> Result<String> {
        let raw_output = self.execute_inner(tool_name, args_json).await?;
        if raw_output.starts_with("__MOAT_DENIED__") {
            return Ok(raw_output);
        }
        let cap = crate::agent::context::budget::AgentBudgetProfile::for_agent(&self.agent_id)
            .observation_cap;
        Ok(crate::agent::context::budget::ContextBudget::truncate_observation(&raw_output, cap))
    }

    async fn execute_inner(&self, tool_name: &str, args_json: &str) -> Result<String> {
        let args: serde_json::Value = match serde_json::from_str(args_json) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[ToolExecutor] Failed to parse tool args: {}. Raw: {}",
                    e, args_json
                );
                serde_json::Value::Null
            }
        };

        match tool_name {
            "fs_read" | "read_file" => {
                let raw_path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("fs_read requires 'path' argument"))?;
                let resolved = Self::resolve_and_validate(raw_path)?;
                if !self
                    .check_path_permission(PermissionFlags::FILE_READ, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:read] No permission to read {}",
                        raw_path
                    ));
                }
                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                self.execute_del("read_file", del_args).await
            }
            "fs_exists" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("fs_exists requires 'path' argument"))?;
                let resolved_path = Self::resolve_path(path)?;
                let canonicalized = Self::canonicalize_new_path(Path::new(&resolved_path))?;
                let resolved = canonicalized.to_string_lossy().into_owned();
                if !self
                    .check_path_permission(PermissionFlags::FILE_READ, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:exists] No permission to read {}",
                        path
                    ));
                }
                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                self.execute_del("fs_exists", del_args).await
            }
            "fs_write" | "write_file" => {
                let raw_path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("fs_write requires 'path' argument"))?;
                let resolved_path = Self::resolve_path(raw_path)?;
                let canonicalized = Self::canonicalize_new_path(Path::new(&resolved_path))?;
                let resolved = canonicalized.to_string_lossy().into_owned();
                if Self::is_path_protected(Path::new(&resolved)) {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:write] {} is a protected file",
                        raw_path
                    ));
                }
                if !self
                    .check_path_permission(PermissionFlags::FILE_WRITE, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:write] No permission to write {}",
                        raw_path
                    ));
                }

                let content = args["content"].as_str().unwrap_or("");
                if content.is_empty() && Path::new(&resolved).exists() {
                    return Err(anyhow::anyhow!(
                        "__MOAT_DENIED__ [fs:write] Empty write to existing file detected. Use fs_delete to delete files."
                    ));
                }

                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                let result = self.execute_del("write_file", del_args).await?;
                let _ =
                    self.journal
                        .log_entry(&self.agent_id, "write_file", &resolved, true, &result);
                Ok(result)
            }
            "fs_delete" => {
                let raw_path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("fs_delete requires 'path' argument"))?;
                let consent_approved = if let Some(bridge) = &self.consent_bridge {
                    bridge
                        .request_consent("fs:delete", &format!("Delete file: {}", raw_path))
                        .await
                } else {
                    let request = ApprovalRequest {
                        agent_id: self.agent_id.clone(),
                        capability: Capability::FileDelete,
                        scope: raw_path.to_string(),
                        description: format!(
                            "Agent {} requested to delete file {}",
                            self.agent_id, raw_path
                        ),
                    };
                    let approved = matches!(
                        ApprovalFlow::request_approval(request)
                            .await
                            .unwrap_or(ApprovalDecision::Denied),
                        ApprovalDecision::Granted
                    );
                    if approved {
                        let mut brain = self.meta_brain.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = brain.grant_path_permission(raw_path, PermissionFlags::FILE_DELETE);
                        let _ = self.session_store.record_approval(
                            &self.agent_id,
                            &format!("{:?}", PermissionFlags::FILE_DELETE),
                            "path",
                            raw_path,
                        );
                    }
                    approved
                };
                if !consent_approved {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:delete] consent not granted for {}",
                        raw_path
                    ));
                }
                if !Path::new(raw_path).exists() {
                    return Ok(format!("[fs:delete] file not found: {}", raw_path));
                }
                let resolved = Self::resolve_and_validate(raw_path)?;
                if self.consent_bridge.is_some() {
                    let mut brain = self.meta_brain.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = brain.grant_path_permission(&resolved, PermissionFlags::FILE_DELETE);
                    let _ = self.session_store.record_approval(
                        &self.agent_id,
                        &format!("{:?}", PermissionFlags::FILE_DELETE),
                        "path",
                        &resolved,
                    );
                }
                if !self
                    .check_path_permission(PermissionFlags::FILE_DELETE, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:delete] No permission to delete {}",
                        raw_path
                    ));
                }

                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                let result = self.execute_del("fs_delete", del_args).await?;
                let _ =
                    self.journal
                        .log_entry(&self.agent_id, "delete_file", &resolved, true, &result);
                Ok(result)
            }
            "fs_mkdir" => {
                let raw_path = args["path"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("fs_mkdir requires 'path' argument"))?;
                let resolved_path = Self::resolve_path(raw_path)?;
                let canonicalized = Self::canonicalize_new_path(Path::new(&resolved_path))?;
                let resolved = canonicalized.to_string_lossy().into_owned();
                if !self
                    .check_path_permission(PermissionFlags::FILE_WRITE, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:mkdir] No permission to write {}",
                        raw_path
                    ));
                }
                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                self.execute_del("fs_mkdir", del_args).await
            }
            "fs:list" | "list_files" => {
                let raw_path = match args["path"].as_str() {
                    Some(p) => p,
                    None => match args["directory"].as_str() {
                        Some(p) => p,
                        None => {
                            return Err(anyhow::anyhow!(
                                "list_files requires 'path' or 'directory' argument"
                            ))
                        }
                    },
                };
                let resolved_path = Self::resolve_path(raw_path)?;
                let canonicalized = Self::canonicalize_new_path(Path::new(&resolved_path))?;
                let resolved = canonicalized.to_string_lossy().into_owned();
                if !self
                    .check_path_permission(PermissionFlags::FILE_READ, &resolved)
                    .await?
                {
                    return Ok(format!(
                        "__MOAT_DENIED__ [fs:list] No permission to list {}",
                        raw_path
                    ));
                }
                let mut del_args = args.clone();
                del_args["path"] = serde_json::Value::String(resolved.clone());
                self.execute_del("list_files", del_args).await
            }
            "opportunity:classify" | "opportunity_classify" => {
                let thread_id = args["thread_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'thread_id'"))?;
                let status_str = args["status"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'status'"))?;
                let status = crate::business::models::OpportunityStatus::from_str(status_str)
                    .ok_or_else(|| anyhow::anyhow!("Invalid status '{}'", status_str))?;

                self.session_store.with_conn(|conn| {
                    crate::business::db::update_thread_opportunity_status(conn, thread_id, status)?;
                    crate::business::db::update_opportunity_status(conn, thread_id, status)?;
                    Ok::<(), anyhow::Error>(())
                })?;
                Ok(format!(
                    "[opportunity:classify] Opportunity status updated to '{}' for thread {}",
                    status_str, thread_id
                ))
            }
            "engagement:propose" | "engagement_propose" => {
                let contact_id = args["contact_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'contact_id'"))?;
                let thread_id = args["thread_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'thread_id'"))?;
                let title = args["title"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'title'"))?;
                let start_time = args["start_time"]
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'start_time'"))?;
                let end_time = args["end_time"]
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'end_time'"))?;

                let eng = self.session_store.with_conn(|conn| {
                    crate::business::workflow::propose_engagement(
                        conn, contact_id, thread_id, title, start_time, end_time,
                    )
                })?;
                Ok(format!(
                    "[engagement:propose] Propose engagement success: ID {}, status proposed",
                    eng.id
                ))
            }
            "engagement:request_authorization" | "engagement_request_auth" => {
                let engagement_id = args["engagement_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'engagement_id'"))?;
                self.session_store.with_conn(|conn| {
                    crate::business::workflow::request_authorization(conn, engagement_id, "agent")
                })?;
                Ok(format!("[engagement:request_authorization] Engagement {} transitioned to pending_authorization", engagement_id))
            }
            "engagement:authorize" | "engagement_authorize" => {
                let engagement_id = args["engagement_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'engagement_id'"))?;
                self.session_store.with_conn(|conn| {
                    crate::business::workflow::authorize_engagement(conn, engagement_id)
                })?;

                // Run scheduling provider execution (mock interface execution)
                let provider = crate::business::integration::SchedulingProvider::new();
                let payload = serde_json::json!({
                    "contact_id": "resolved",
                    "title": "Authorized Engagement",
                    "start_time": 0,
                    "end_time": 0
                });
                use crate::business::integration::BusinessIntegrationProvider;
                let exec_result = provider
                    .execute("book_engagement", payload)
                    .map_err(|e| anyhow::anyhow!(e))?;

                Ok(format!(
                    "[engagement:authorize] Engagement {} authorized. Execution result: {}",
                    engagement_id, exec_result
                ))
            }
            "operator:escalate" | "operator_escalate" => {
                let thread_id = args["thread_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'thread_id'"))?;
                let reason = args["reason"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'reason'"))?;
                let esc = self.session_store.with_conn(|conn| {
                    crate::business::escalation::escalate_thread(conn, thread_id, reason)
                })?;
                Ok(format!(
                    "[operator:escalate] Escalation created: ID {}, thread {} escalated",
                    esc.id, thread_id
                ))
            }
            "knowledge:query" | "knowledge_query" => {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'query'"))?;
                let limit = args["limit"].as_u64().unwrap_or(3) as usize;
                let results = self.session_store.with_conn(|conn| {
                    crate::business::knowledge::query_knowledge_store_ranked(conn, query, limit)
                })?;
                let json_res = serde_json::to_string(&results)?;
                Ok(json_res)
            }
            "identity:resolve" | "identity_resolve" => {
                let channel = args["channel"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'channel'"))?;
                let external_id = args["external_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'external_id'"))?;
                let contact = self.session_store.with_conn(|conn| {
                    crate::business::identity::resolve_contact(conn, channel, external_id)
                })?;
                Ok(format!(
                    "[identity:resolve] Resolved to Contact ID: {}",
                    contact.id
                ))
            }
            "property_tool" => {
                let action = args["action"].as_str().unwrap_or("schedule_viewing");
                if action == "schedule_viewing" {
                    let time_str = args["time"].as_str().unwrap_or("tomorrow");
                    let prop_id = args["property_id"].as_str().unwrap_or("prop_default");
                    let visit_id = format!("visit_{}", uuid::Uuid::new_v4());
                    self.session_store.with_conn(|conn| {
                        conn.execute(
                            "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
                             ON CONFLICT(id) DO UPDATE SET version = version + 1, timestamp = ?4",
                            rusqlite::params![&visit_id, "default_business", "PropertyVisitScheduled", chrono::Utc::now().timestamp_millis()]
                        )
                    })?;
                    Ok(format!("[property_tool] Scheduled property viewing for property {} at {}.", prop_id, time_str))
                } else {
                    Ok("[property_tool] Property action processed.".to_string())
                }
            }
            "deal_tool" => {
                let deal_id = format!("deal_{}", uuid::Uuid::new_v4());
                let value = args["value"].as_f64().or_else(|| args["amount"].as_f64()).unwrap_or(100000.0);
                self.session_store.with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
                         ON CONFLICT(id) DO UPDATE SET version = version + 1, timestamp = ?4",
                        rusqlite::params![&deal_id, "default_business", "DealCreated", chrono::Utc::now().timestamp_millis()]
                    )?;
                    let payload_str = serde_json::json!({ "id": deal_id, "value": value, "status": "pending" }).to_string();
                    conn.execute(
                        "INSERT INTO business_event_store (id, business_id, event_type, aggregate_id, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![uuid::Uuid::new_v4().to_string(), "default_business", "DealCreated", &deal_id, payload_str, chrono::Utc::now().timestamp_millis()]
                    )
                })?;
                Ok(format!("[deal_tool] Created deal {} with value ${}.", deal_id, value))
            }
            "sales_lead_creator" => {
                let lead_id = format!("lead_{}", uuid::Uuid::new_v4());
                let name = args["name"].as_str().unwrap_or("New Lead");
                let email = args["email"].as_str().unwrap_or("");
                self.session_store.with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
                         ON CONFLICT(id) DO UPDATE SET version = version + 1, timestamp = ?4",
                        rusqlite::params![&lead_id, "default_business", "LeadCreated", chrono::Utc::now().timestamp_millis()]
                    )?;
                    let payload_str = serde_json::json!({ "id": lead_id, "name": name, "email": email }).to_string();
                    conn.execute(
                        "INSERT INTO business_event_store (id, business_id, event_type, aggregate_id, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![uuid::Uuid::new_v4().to_string(), "default_business", "LeadCreated", &lead_id, payload_str, chrono::Utc::now().timestamp_millis()]
                    )
                })?;
                Ok(format!("[sales_lead_creator] Created lead {} for {}.", lead_id, name))
            }
            "sales_lead_qualifier" => {
                let lead_id = args["lead_id"].as_str().unwrap_or("");
                if !lead_id.is_empty() {
                    self.session_store.with_conn(|conn| {
                        conn.execute(
                            "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
                             ON CONFLICT(id) DO UPDATE SET name = ?3, version = version + 1, timestamp = ?4",
                            rusqlite::params![lead_id, "default_business", "LeadQualified", chrono::Utc::now().timestamp_millis()]
                        )
                    })?;
                    Ok(format!("[sales_lead_qualifier] Qualified lead {}.", lead_id))
                } else {
                    Ok("[sales_lead_qualifier] No lead_id provided.".to_string())
                }
            }
            "notification_sender" => {
                let recipient = args["recipient"].as_str().unwrap_or("Unknown");
                let message = args["message"].as_str().unwrap_or("");
                let notif_id = format!("notif_{}", uuid::Uuid::new_v4());
                self.session_store.with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)
                         ON CONFLICT(id) DO UPDATE SET version = version + 1, timestamp = ?4",
                        rusqlite::params![&notif_id, "default_business", "NotificationSent", chrono::Utc::now().timestamp_millis()]
                    )?;
                    let payload_str = serde_json::json!({ "id": notif_id, "recipient": recipient, "message": message }).to_string();
                    conn.execute(
                        "INSERT INTO business_event_store (id, business_id, event_type, aggregate_id, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![uuid::Uuid::new_v4().to_string(), "default_business", "NotificationSent", &notif_id, payload_str, chrono::Utc::now().timestamp_millis()]
                    )
                })?;
                Ok(format!("[notification_sender] Notification sent to recipient {}.", recipient))
            }
            unknown => {
                let registry = crate::agent::tools::global_registry().read().unwrap();
                if let Some(tool) = registry.get(unknown) {
                    let intent = crate::agent::interaction_orchestrator::ActionIntent {
                        thought: None,
                        action: unknown.to_string(),
                        target: args.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        payload: Some(args.clone()),
                        reason: None,
                    };
                    match tool.execute(&intent).await {
                        Ok(crate::agent::interaction_orchestrator::TaskStatus::Success { stdout }) => Ok(stdout),
                        Ok(crate::agent::interaction_orchestrator::TaskStatus::Failed { stderr, .. }) => Err(anyhow::anyhow!("Dynamic tool execution failed: {}", stderr)),
                        Err(e) => Err(anyhow::anyhow!("Dynamic tool error: {:?}", e)),
                        _ => Err(anyhow::anyhow!("Dynamic tool execution did not succeed")),
                    }
                } else {
                    anyhow::bail!("Unknown tool: {}", unknown)
                }
            }
        }
    }

    pub(crate) async fn execute_via_del(
        &self,
        del_tool_name: &str,
        args: serde_json::Value,
    ) -> Result<String> {
        let raw_output = self.execute_del(del_tool_name, args).await?;
        if raw_output.starts_with("__MOAT_DENIED__") {
            return Ok(raw_output);
        }
        let cap = crate::agent::context::budget::AgentBudgetProfile::for_agent(&self.agent_id)
            .observation_cap;
        Ok(crate::agent::context::budget::ContextBudget::truncate_observation(&raw_output, cap))
    }

    async fn execute_del(&self, del_tool_name: &str, args: serde_json::Value) -> Result<String> {
        self.sync_del_brain().await;
        let capability = Self::tool_to_del_capability(del_tool_name);
        let request = ExecutionRequest::new(&self.agent_id, del_tool_name, capability, args);
        eprintln!(
            "[ToolExecutor] → dispatching to DEL: {} (cap={:?})",
            del_tool_name, capability
        );
        let result = self.del.execute(request).await;
        eprintln!(
            "[ToolExecutor] ← DEL returned success={} gate1={} gate2={} gate3={}",
            result.success, result.gate1_passed, result.gate2_passed, result.gate3_passed
        );
        if result.success {
            Ok(result.output)
        } else {
            Ok(format!(
                "Execution denied: {}",
                result.denied_reason.unwrap_or_default()
            ))
        }
    }

    pub(crate) fn tool_to_del_capability(tool_name: &str) -> Capability {
        match tool_name {
            "read_file" => Capability::FileRead,
            "write_file" => Capability::FileWrite,
            "delete_file" | "fs_delete" => Capability::FileDelete,
            "fs_exists" => Capability::FileRead,
            "list_files" => Capability::FileRead,
            _ => Capability::FileRead,
        }
    }

    async fn check_path_permission(&self, flags: PermissionFlags, path: &str) -> Result<bool> {
        let has_permission = {
            let brain = self.meta_brain.lock().unwrap();
            brain.path_has_permission(path, flags)
        };

        if has_permission {
            return Ok(true);
        }

        if self.auto_deny {
            let label = Self::flags_to_label(flags);
            eprintln!(
                "[ToolExecutor] auto-deny: {} attempted {} on {} (permission not pre-granted)",
                self.agent_id, label, path
            );
            return Ok(false);
        }

        if let Some(bridge) = &self.consent_bridge {
            let label = Self::flags_to_label(flags);
            let description = format!(
                "Agent {} requested {} access to {}",
                self.agent_id, label, path
            );
            let approved = bridge.request_consent(label, &description).await;
            if approved {
                let mut brain = self.meta_brain.lock().unwrap();
                brain
                    .grant_path_permission(path, flags)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                self.session_store.record_approval(
                    &self.agent_id,
                    &format!("{:?}", flags),
                    "path",
                    path,
                )?;
                return Ok(true);
            }
            return Ok(false);
        }

        let capability = Self::flags_to_capability(flags);
        let label = Self::flags_to_label(flags);

        let request = ApprovalRequest {
            agent_id: self.agent_id.clone(),
            capability,
            scope: path.to_string(),
            description: format!(
                "Agent {} requested {} access to {}",
                self.agent_id, label, path
            ),
        };

        let decision = ApprovalFlow::request_approval(request).await?;

        match decision {
            ApprovalDecision::Granted => {
                let mut brain = self.meta_brain.lock().unwrap();
                brain
                    .grant_path_permission(path, flags)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                self.session_store.record_approval(
                    &self.agent_id,
                    &format!("{:?}", flags),
                    "path",
                    path,
                )?;
                Ok(true)
            }
            ApprovalDecision::Denied | ApprovalDecision::TimedOut => Ok(false),
        }
    }

    fn is_path_protected(path: &Path) -> bool {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let path_str = path.to_string_lossy().to_string();

        for pattern in &[".log", ".db", ".sqlite", ".key", ".pem"] {
            if file_name.ends_with(pattern) {
                return true;
            }
        }
        if file_name == "Cargo.lock" {
            return true;
        }
        if path_str.contains(".git/") {
            return true;
        }
        false
    }

    fn resolve_path(raw: &str) -> Result<String> {
        let expanded = if raw.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.to_string_lossy().into_owned() + &raw[1..]
            } else {
                raw.to_string()
            }
        } else {
            raw.to_string()
        };
        let p = Path::new(&expanded);
        if p.is_absolute() {
            if let Ok(canonical) = std::fs::canonicalize(&expanded) {
                let canonical_str = canonical.to_string_lossy().into_owned();
                if canonical_str != expanded {
                    return Err(anyhow::anyhow!(
                        "[security] SYMLINK TRAVERSAL DENIED: {} -> {}",
                        expanded,
                        canonical_str
                    ));
                }
                Ok(canonical_str)
            } else {
                Ok(expanded)
            }
        } else {
            let workspace = dirs::home_dir()
                .unwrap_or_default()
                .join("Desktop")
                .join("ageex-workspace");
            Ok(workspace.join(raw).to_string_lossy().into_owned())
        }
    }

    fn lexical_normal(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::Prefix(prefix) => {
                    normalized.push(prefix.as_os_str());
                }
                std::path::Component::RootDir => {
                    normalized.push(std::path::Component::RootDir.as_os_str());
                }
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::Normal(c) => {
                    normalized.push(c);
                }
            }
        }
        normalized
    }

    fn canonicalize_new_path(path: &Path) -> Result<PathBuf> {
        let normalized_input = Self::lexical_normal(path);
        let mut current = normalized_input.clone();
        let mut suffix = PathBuf::new();

        while !current.exists() {
            if let Some(parent) = current.parent() {
                if let Some(name) = current.file_name() {
                    suffix = PathBuf::from(name).join(suffix);
                }
                current = parent.to_path_buf();
            } else {
                break;
            }
        }

        let canonical_parent = current.canonicalize()?;
        let resolved = canonical_parent.join(suffix);
        let normalized = Self::lexical_normal(&resolved);
        Ok(normalized)
    }

    #[cfg(target_os = "macos")]
    fn resolve_from_fd(fd: std::os::unix::io::RawFd) -> Result<String> {
        let mut buf = vec![0u8; libc::PATH_MAX as usize];
        let ret = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) };
        if ret == -1 {
            return Err(anyhow::anyhow!(
                "fcntl F_GETPATH failed (fd {}): {}",
                fd,
                std::io::Error::last_os_error()
            ));
        }
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..nul]).to_string())
    }

    #[cfg(target_os = "linux")]
    fn resolve_from_fd(fd: std::os::unix::io::RawFd) -> Result<String> {
        let path = format!("/proc/self/fd/{}", fd);
        std::fs::read_link(&path)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| anyhow::anyhow!("Failed to read_link {}: {}", path, e))
    }

    fn resolve_and_validate(raw_path: &str) -> Result<String> {
        let expanded = if raw_path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.to_string_lossy().into_owned() + &raw_path[1..]
            } else {
                raw_path.to_string()
            }
        } else {
            raw_path.to_string()
        };
        let p = Path::new(&expanded);
        let open_path = if p.is_absolute() {
            expanded.clone()
        } else {
            let workspace = dirs::home_dir()
                .unwrap_or_default()
                .join("Desktop")
                .join("ageex-workspace");
            workspace.join(raw_path).to_string_lossy().into_owned()
        };

        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&open_path)
            .map_err(|e| match e.raw_os_error() {
                Some(libc::ELOOP) => {
                    anyhow::anyhow!(ExecutionError::SymlinkDenied(PathBuf::from(&open_path)))
                }
                _ => anyhow::anyhow!("[fs] open failed for {}: {}", open_path, e),
            })?;

        let resolved = Self::resolve_from_fd(file.as_raw_fd())?;
        drop(file);

        if Self::is_path_protected(Path::new(&resolved)) {
            return Err(anyhow::anyhow!(
                "[security] PROTECTED FILE ACCESS DENIED: {}",
                resolved
            ));
        }

        let canonical =
            std::fs::canonicalize(&open_path).unwrap_or_else(|_| PathBuf::from(&open_path));
        let canonical_str = canonical.to_string_lossy().to_string();
        if canonical_str != resolved {
            eprintln!(
                "[ToolExecutor] Path mismatch: canonicalize={} vs fd-path={}",
                canonical_str, resolved
            );
        }

        Ok(resolved)
    }

    fn flags_to_capability(flags: PermissionFlags) -> Capability {
        if flags.contains(PermissionFlags::FILE_DELETE) {
            Capability::FileDelete
        } else if flags.contains(PermissionFlags::FILE_WRITE) {
            Capability::FileWrite
        } else {
            Capability::FileRead
        }
    }

    fn flags_to_label(flags: PermissionFlags) -> &'static str {
        if flags.contains(PermissionFlags::FILE_DELETE) {
            "fs:delete"
        } else if flags.contains(PermissionFlags::FILE_WRITE) {
            "fs:write"
        } else {
            "fs:read"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_to_del_capability_mappings() {
        assert_eq!(
            ToolExecutor::tool_to_del_capability("delete_file"),
            Capability::FileDelete
        );
        assert_eq!(
            ToolExecutor::tool_to_del_capability("read_file"),
            Capability::FileRead
        );
        assert_eq!(
            ToolExecutor::tool_to_del_capability("write_file"),
            Capability::FileWrite
        );
    }

    #[test]
    fn test_unknown_tool_defaults_to_file_read() {
        assert_eq!(
            ToolExecutor::tool_to_del_capability("nonexistent"),
            Capability::FileRead
        );
    }

    #[test]
    fn test_path_protection_detects_dangerous_extensions() {
        assert!(ToolExecutor::is_path_protected(Path::new("/tmp/test.log")));
        assert!(ToolExecutor::is_path_protected(Path::new("/tmp/test.db")));
        assert!(!ToolExecutor::is_path_protected(Path::new("/tmp/test.rs")));
    }

    #[test]
    fn test_cargo_lock_is_protected() {
        assert!(ToolExecutor::is_path_protected(Path::new(
            "/project/Cargo.lock"
        )));
    }

    #[test]
    fn test_all_del_tool_capability_mappings() {
        let mappings = &[
            ("read_file", Capability::FileRead),
            ("write_file", Capability::FileWrite),
            ("fs_delete", Capability::FileDelete),
            ("fs_exists", Capability::FileRead),
            ("list_files", Capability::FileRead),
        ];
        for (tool, expected) in mappings {
            assert_eq!(
                ToolExecutor::tool_to_del_capability(tool),
                *expected,
                "tool_to_del_capability mismatch for {}",
                tool
            );
        }
    }

    #[test]
    fn test_protected_patterns_prevent_log_deletion() {
        assert!(
            ToolExecutor::is_path_protected(Path::new("/workspace/debug.log")),
            "*.log must be protected"
        );
        assert!(
            ToolExecutor::is_path_protected(Path::new("/workspace/data.db")),
            "*.db must be protected"
        );
        assert!(
            ToolExecutor::is_path_protected(Path::new("/workspace/secrets.key")),
            "*.key must be protected"
        );
        assert!(
            ToolExecutor::is_path_protected(Path::new("/workspace/cert.pem")),
            "*.pem must be protected"
        );
    }

    #[test]
    fn test_git_directory_is_protected() {
        assert!(
            ToolExecutor::is_path_protected(Path::new("/workspace/.git/config")),
            ".git/** must be protected"
        );
    }

    #[test]
    fn test_regular_files_are_not_protected() {
        assert!(
            !ToolExecutor::is_path_protected(Path::new("/workspace/main.rs")),
            ".rs files should NOT be protected"
        );
        assert!(
            !ToolExecutor::is_path_protected(Path::new("/workspace/README.md")),
            ".md files should NOT be protected"
        );
    }

    #[test]
    fn test_fd_path_resolution_matches_real_path() {
        use std::io::Write;
        use std::os::unix::io::AsRawFd;

        let tmp = std::env::temp_dir().join("ageex_toctou_test.txt");
        let mut f = std::fs::File::create(&tmp).expect("create temp file");
        f.write_all(b"hello").expect("write");

        let fd_path = ToolExecutor::resolve_from_fd(f.as_raw_fd()).expect("resolve from fd");
        let canonical = std::fs::canonicalize(&tmp).expect("canonicalize");

        assert_eq!(
            fd_path,
            canonical.to_string_lossy().to_string(),
            "fd-derived path must match canonical path (TOCTOU safety)"
        );

        drop(f);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_destructive_fs_delete_requires_consent() {
        use crate::agent::consent::{ConsentBridge, ConsentDecision};
        use std::io::Write;

        let tmp = std::env::temp_dir().join("ageex_consent_test_delete.txt");
        let mut f = std::fs::File::create(&tmp).expect("create temp file");
        f.write_all(b"delete me").expect("write");
        drop(f);

        let (bridge_slot, bridge) = {
            let slot: Arc<Mutex<Option<std::sync::Weak<ConsentBridge>>>> =
                Arc::new(Mutex::new(None));
            let slot_for_emit = Arc::clone(&slot);
            let bridge = Arc::new(ConsentBridge::new(Box::new(move |req| {
                let weak = slot_for_emit
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
                    .expect("bridge stored before emit");
                let req_id = req.request_id.clone();
                std::thread::spawn(move || {
                    if let Some(bridge) = weak.upgrade() {
                        let _ = bridge.resolve(&req_id, ConsentDecision::Deny);
                    }
                });
            })));
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::downgrade(&bridge));
            (slot, bridge)
        };

        let brain = Arc::new(Mutex::new(MetaBrain::new()));
        let store =
            SessionStore::open_with_prefix_in_memory("test_consent_delete").expect("test store");
        let store = Arc::new(store);
        brain
            .lock()
            .unwrap()
            .grant_path_permission(&tmp.to_string_lossy(), PermissionFlags::FILE_DELETE)
            .expect("grant delete");

        let exec = ToolExecutor::new_with_consent(
            Arc::clone(&brain),
            "business_runtime",
            Arc::clone(&store),
            Arc::clone(&bridge),
        )
        .expect("create executor");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let json = format!(r#"{{"path":"{}"}}"#, tmp.to_string_lossy());
        let result = rt.block_on(exec.execute("fs_delete", &json));

        match result {
            Ok(output) => {
                assert!(
                    output.contains("DENIED")
                        || output.contains("denied")
                        || output.contains("consent"),
                    "fs_delete without consent should be denied: {}",
                    output
                );
            }
            Err(e) => {
                assert!(
                    e.to_string().contains("consent") || e.to_string().contains("denied"),
                    "Error should mention consent denial: {}",
                    e
                );
            }
        }

        let _ = std::fs::remove_file(&tmp);
        drop(bridge_slot);
    }

    #[test]
    fn test_path_traversal_on_fs_write_is_denied() {
        let brain = Arc::new(Mutex::new(MetaBrain::new()));
        let store = SessionStore::open_with_prefix_in_memory("test_path_traversal_write")
            .expect("test store");
        let store = Arc::new(store);

        brain
            .lock()
            .unwrap()
            .grant_path_permission("/workspace/src", PermissionFlags::FILE_WRITE)
            .expect("grant write");

        let exec =
            ToolExecutor::new_with_auto_deny(Arc::clone(&brain), "business_runtime", store, true)
                .expect("create executor");

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let json = r#"{"path":"/workspace/src/../docs/illegal.txt","content":"hack"}"#;
        let result = rt
            .block_on(exec.execute("fs_write", json))
            .expect("execute");

        assert!(
            result.contains("__MOAT_DENIED__") || result.contains("permission"),
            "Expected denial on directory traversal write, got: {}",
            result
        );
    }
}
