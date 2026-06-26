use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use rusqlite::Connection;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use uuid::Uuid;

use crate::agent::llm_types::{LlmMessage, LlmProvider, AgentSpec};
use crate::agent::types::{UiActionBridge, ActionIntent, TaskStatus, SubTaskResult};
use crate::agent::consent::ConsentBridge;
use crate::agent::execution::DelExecutor;
use crate::agent::capabilities::Capability;
use crate::agent::loop_detector::LoopDetector;
use crate::memory::meta_brain::MetaBrain;
use crate::agent::prompt::build_react_system_prompt;
use crate::MoatResult;

pub struct AIAgent {
    pub provider: Arc<dyn LlmProvider>,
    pub db: Arc<AsyncMutex<Connection>>,
    pub meta_brain: Arc<AsyncRwLock<MetaBrain>>,
    pub consent_bridge: Option<Arc<ConsentBridge>>,
    pub ui_bridge: Option<Arc<dyn UiActionBridge>>,
    pub del: DelExecutor,
    pub loop_detector: LoopDetector,
    pub cancel_flag: Arc<AtomicBool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VirtualScope {
    pub agent_name: String,
    pub role_description: String,
    pub allowed_tools: Vec<String>,
    pub max_turns: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentExecutionState {
    pub session_id: Uuid,
    pub objective: String,
    pub context_trace: Vec<LlmMessage>,
    pub turns: usize,
    pub allowed_tools: Vec<String>,
    pub scope_stack: Vec<VirtualScope>,
    pub total_tokens: usize,
    #[serde(default)]
    pub history_tokens: Vec<usize>,
}

fn agent_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "state": {
                "type": "string",
                "enum": ["running", "completed"],
                "description": "running if more tools need to be called, completed if the task is done"
            },
            "thought": {
                "type": "string",
                "description": "Brief reasoning about current state and next steps"
            },
            "actions": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string" },
                        "target": { "type": "string" },
                        "payload": {},
                        "reason": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            "summary": {
                "type": "string",
                "description": "Final answer when state is completed"
            }
        },
        "required": ["state", "thought", "actions"]
    })
}

fn resolve_workspace_target(target: &str) -> Result<std::path::PathBuf, String> {
    let expanded = crate::agent::execution::gate2_policy::Gate2Policy::expand_tilde(target.trim());
    let path = std::path::Path::new(&expanded);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let root = crate::config::workspace_root();
        Ok(root.join(path))
    }
}

impl AIAgent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        db: Arc<AsyncMutex<Connection>>,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        consent_bridge: Option<Arc<ConsentBridge>>,
    ) -> Result<Self, anyhow::Error> {
        let bridge = consent_bridge.clone().unwrap_or_else(|| {
            Arc::new(ConsentBridge::new(Box::new(|_| {})))
        });
        let del = DelExecutor::new(Arc::clone(&meta_brain), bridge)?;
        Ok(Self {
            provider,
            db,
            meta_brain,
            consent_bridge,
            ui_bridge: None,
            del,
            loop_detector: LoopDetector::new(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn count_messages_tokens(&self, messages: &[LlmMessage]) -> usize {
        let mut total = 0;
        for msg in messages {
            if let Some(ref content) = msg.content {
                total += crate::agent::context::TokenBudget::count_tokens(content).unwrap_or(0);
            }
            if let Some(ref calls) = msg.tool_calls {
                for call in calls {
                    total +=
                        crate::agent::context::TokenBudget::count_tokens(&call.function.name)
                            .unwrap_or(0);
                    total += crate::agent::context::TokenBudget::count_tokens(
                        &call.function.arguments,
                    )
                    .unwrap_or(0);
                }
            }
        }
        total
    }

    pub fn count_single_message_tokens(&self, msg: &LlmMessage) -> usize {
        let mut total = 0;
        if let Some(ref content) = msg.content {
            total += crate::agent::context::TokenBudget::count_tokens(content).unwrap_or(0);
        }
        if let Some(ref calls) = msg.tool_calls {
            for call in calls {
                total +=
                    crate::agent::context::TokenBudget::count_tokens(&call.function.name)
                        .unwrap_or(0);
                total += crate::agent::context::TokenBudget::count_tokens(
                    &call.function.arguments,
                )
                .unwrap_or(0);
            }
        }
        total
    }

    pub fn push_message(&self, state: &mut AgentExecutionState, msg: LlmMessage) {
        let tokens = self.count_single_message_tokens(&msg);
        state.context_trace.push(msg);
        state.history_tokens.push(tokens);
        state.total_tokens += tokens;
    }

    pub fn insert_message(&self, state: &mut AgentExecutionState, index: usize, msg: LlmMessage) {
        let tokens = self.count_single_message_tokens(&msg);
        state.context_trace.insert(index, msg);
        state.history_tokens.insert(index, tokens);
        state.total_tokens += tokens;
    }

    pub fn set_system_prompt(&self, state: &mut AgentExecutionState, prompt: String) {
        let msg = LlmMessage {
            role: "system".into(),
            content: Some(prompt),
            tool_calls: None,
            tool_call_id: None,
        };
        let tokens = self.count_single_message_tokens(&msg);
        if !state.context_trace.is_empty() && state.context_trace[0].role == "system" {
            let old_tokens = state.history_tokens.get(0).copied().unwrap_or(0);
            state.total_tokens = state.total_tokens.saturating_sub(old_tokens).saturating_add(tokens);
            state.context_trace[0] = msg;
            if !state.history_tokens.is_empty() {
                state.history_tokens[0] = tokens;
            } else {
                state.history_tokens.push(tokens);
            }
        } else {
            self.insert_message(state, 0, msg);
        }
    }

    pub fn ensure_token_cache(&self, state: &mut AgentExecutionState) {
        if state.history_tokens.len() != state.context_trace.len() {
            state.history_tokens.clear();
            let mut total = 0;
            for msg in &state.context_trace {
                let t = self.count_single_message_tokens(msg);
                state.history_tokens.push(t);
                total += t;
            }
            state.total_tokens = total;
        }
    }

    pub fn compress_context_if_needed(&self, state: &mut AgentExecutionState) -> MoatResult<()> {
        self.ensure_token_cache(state);
        
        let total_tokens = state.total_tokens;

        const PROACTIVE_THRESHOLD: usize = 115_200;
        const HARD_TOKEN_LIMIT: usize = 128_000;

        if total_tokens > PROACTIVE_THRESHOLD {
            let history = state.context_trace.clone();
            if history.len() <= 6 {
                return Ok(());
            }

            let has_two_system_headers = history.len() >= 2
                && history[0].role == "system"
                && history[1].role == "system";

            let (start_idx, min_start) = if has_two_system_headers {
                (history.len().saturating_sub(4), 2)
            } else {
                (history.len().saturating_sub(5), 1)
            };

            let mut adjusted_start = start_idx;
            while adjusted_start > min_start {
                let msg = &history[adjusted_start];
                if msg.role == "tool" {
                    adjusted_start -= 1;
                } else if msg.role == "assistant" && msg.tool_calls.is_some() {
                    break;
                } else {
                    break;
                }
            }

            let mut replacement = Vec::new();
            let mut replacement_tokens = Vec::new();
            if let Some(first) = history.first() {
                replacement.push(first.clone());
                replacement_tokens.push(state.history_tokens[0]);
            }
            if has_two_system_headers {
                if let Some(second) = history.get(1) {
                    replacement.push(second.clone());
                    replacement_tokens.push(state.history_tokens[1]);
                }
            }
            replacement.extend(history[adjusted_start..].iter().cloned());
            replacement_tokens.extend(state.history_tokens[adjusted_start..].iter().copied());
            
            state.context_trace = replacement;
            state.history_tokens = replacement_tokens;
            state.total_tokens = state.history_tokens.iter().sum();

            if state.total_tokens > HARD_TOKEN_LIMIT {
                return Err(crate::MoatError::Orchestrator(format!(
                    "Token limit Safety Margin violated: {} tokens",
                    state.total_tokens
                )));
            }
        }
        Ok(())
    }

    pub async fn load_active_memory_prefix(&self, objective: &str) -> MoatResult<Option<String>> {
        Self::load_active_memory_prefix_static(&self.db, objective).await
    }

    async fn load_active_memory_prefix_static(db_mutex: &AsyncMutex<Connection>, objective: &str) -> MoatResult<Option<String>> {
        let db = db_mutex.lock().await;
        let mut stmt = db.prepare(
            "SELECT km.title, km.text
             FROM knowledge_memory km
             JOIN knowledge_memory_fts fts ON km.rowid = fts.rowid
             WHERE fts.knowledge_memory_fts MATCH ?1
             ORDER BY fts.rank
             LIMIT 5",
        )?;

        let cleaned = objective.replace('"', " ").trim().to_string();
        if cleaned.is_empty() {
            return Ok(None);
        }
        let fts_query = format!("\"{}\"", cleaned);
        let mut hits = Vec::new();
        if let Ok(mut rows) = stmt.query(rusqlite::params![fts_query]) {
            while let Some(row) = rows.next()? {
                let title: String = row.get(0)?;
                let text: String = row.get(1)?;
                hits.push(format!("[{}] {}", title, text));
            }
        }

        if hits.is_empty() {
            Ok(None)
        } else {
            let context_block = format!(
                "<active_memory_prelude>\n{}\n</active_memory_prelude>",
                hits.join("\n---\n")
            );
            Ok(Some(context_block))
        }
    }

    pub async fn distill_and_save_workflow(
        &self,
        state: &AgentExecutionState,
        results: &[SubTaskResult],
    ) -> MoatResult<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut steps = Vec::new();
        for (i, res) in results.iter().enumerate() {
            let status_str = match &res.status {
                TaskStatus::Success { stdout } => {
                    let clipped: String = stdout.chars().take(100).collect();
                    format!("Success: {}", clipped)
                }
                other => format!("{:?}", other),
            };
            steps.push(format!("Step {}: action={} target={} -> {}", i + 1, res.action, res.target, status_str));
        }

        let workflow_text = format!(
            "[LEARNED WORKFLOW]\nObjective: {}\nSteps Taken:\n{}",
            state.objective,
            steps.join("\n")
        );

        let db = self.db.lock().await;
        crate::memory::store_knowledge(
            &db,
            crate::memory::KnowledgeKind::Fact,
            "memory:learn",
            Some(&format!("Workflow: {}", state.objective)),
            &workflow_text,
            Some(timestamp),
        )?;

        Ok(())
    }

    pub fn build_llm_tools(&self) -> Vec<crate::agent::llm_types::LlmTool> {
        let tools = vec![
            ("identity:resolve", "Resolve messaging handle/channel to a contact ID. Creates a new contact if not found.", serde_json::json!({"type":"object","properties":{"channel":{"type":"string","enum":["whatsapp","instagram","messenger","webchat"]},"external_id":{"type":"string"}},"required":["channel","external_id"]})),
            ("opportunity:classify", "Classify or update the lead/opportunity status for an interaction thread.", serde_json::json!({"type":"object","properties":{"thread_id":{"type":"string"},"status":{"type":"string","enum":["discovered","qualified","disqualified","engagement_pending","converted","archived"]}},"required":["thread_id","status"]})),
            ("engagement:propose", "Propose a new engagement or booking for a contact.", serde_json::json!({"type":"object","properties":{"contact_id":{"type":"string"},"thread_id":{"type":"string"},"title":{"type":"string"},"start_time":{"type":"integer"},"end_time":{"type":"integer"}},"required":["contact_id","thread_id","title","start_time","end_time"]})),
            ("engagement:request_authorization", "Request operator approval for a proposed engagement before it is executed.", serde_json::json!({"type":"object","properties":{"engagement_id":{"type":"string"}},"required":["engagement_id"]})),
            ("engagement:authorize", "Authorize and execute a pending engagement via the scheduling integration.", serde_json::json!({"type":"object","properties":{"engagement_id":{"type":"string"}},"required":["engagement_id"]})),
            ("operator:escalate", "Escalate the conversation thread to a human operator.", serde_json::json!({"type":"object","properties":{"thread_id":{"type":"string"},"reason":{"type":"string"}},"required":["thread_id","reason"]})),
            ("knowledge:query", "Query the organizational knowledge base. Use when you need info to answer customer questions.", serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]})),
            ("done", "Signal task completion and provide the final response to the user.", serde_json::json!({"type":"object","properties":{"payload":{"type":"string"},"_thought":{"type":"string"}},"required":["payload"]})),
        ];

        tools
            .into_iter()
            .map(|(name, desc, mut params)| {
                if let Some(obj) = params.as_object_mut() {
                    if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
                        props.insert(
                            "reason".to_string(),
                            serde_json::json!({
                                "type": "string",
                                "description": "Optional explanation or justification for performing this action"
                            })
                        );
                    }
                }
                crate::agent::llm_types::LlmTool {
                    type_: "function".into(),
                    function: crate::agent::llm_types::LlmFunctionDef {
                        name: name.into(),
                        description: Some(desc.into()),
                        parameters: params,
                    },
                }
            })
            .collect()
    }

    fn tool_to_capability(action: &str) -> Capability {
        match action {
            "read_file" | "fs_read" | "fs:read" => Capability::FileRead,
            "write_file" | "fs_write" | "fs:write" => Capability::FileWrite,
            "delete_file" | "fs_delete" | "fs:delete" => Capability::FileDelete,
            "fs_exists" | "fs:exists" => Capability::FileExists,
            "list_files" | "fs_list" | "fs:list" => Capability::FileRead,
            "fs_mkdir" | "fs:mkdir" => Capability::FileWrite,
            "net:curl" => Capability::UrlOpen,
            _ => Capability::FileRead,
        }
    }

    pub async fn execute_tool(
        &self,
        intent: &ActionIntent,
        session_id: &str,
        agent_label: &str,
    ) -> MoatResult<TaskStatus> {
        let db = self.db.clone();
        let del = self.del.clone();
        let meta_brain = self.meta_brain.clone();
        let intent = intent.clone();
        let session_id = session_id.to_string();
        let agent_label = agent_label.to_string();

        let join_handle = tokio::spawn(async move {
            Self::execute_tool_inner(db, del, meta_brain, intent, session_id, agent_label).await
        });

        match join_handle.await {
            Ok(res) => res,
            Err(e) if e.is_panic() => {
                Ok(TaskStatus::Failed {
                    exit_code: None,
                    stderr: "[SYSTEM ERROR: Tool execution panicked]".to_string(),
                })
            }
            Err(e) => Err(crate::MoatError::Orchestrator(format!("Task execution failed: {}", e))),
        }
    }

    async fn execute_tool_inner(
        db: Arc<AsyncMutex<Connection>>,
        del: DelExecutor,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        intent: ActionIntent,
        session_id: String,
        agent_label: String,
    ) -> MoatResult<TaskStatus> {
        let action = intent.action.as_str();
        let payload = intent.payload.as_ref().unwrap_or(&serde_json::Value::Null);

        match action {
            "read_file" | "fs_read" | "fs:read" |
            "write_file" | "fs_write" | "fs:write" |
            "delete_file" | "fs_delete" | "fs:delete" |
            "fs_exists" | "fs:exists" |
            "list_files" | "fs_list" | "fs:list" |
            "fs_mkdir" | "fs:mkdir" |
            "net:curl" => {
                let capability = Self::tool_to_capability(action);

                let mut args = serde_json::Map::new();
                let resolved_target = if action.starts_with("fs") || action.contains("file") {
                    match resolve_workspace_target(&intent.target) {
                        Ok(p) => p.to_string_lossy().to_string(),
                        Err(e) => {
                            return Ok(TaskStatus::Failed {
                                exit_code: None,
                                stderr: e,
                            });
                        }
                    }
                } else {
                    intent.target.clone()
                };

                args.insert("path".to_string(), serde_json::json!(resolved_target));
                if action.contains("write") {
                    args.insert("content".to_string(), payload.clone());
                } else if action == "net:curl" {
                    args.insert("url".to_string(), serde_json::json!(resolved_target));
                } else {
                    if let serde_json::Value::Object(map) = payload.clone() {
                        args = map;
                    } else {
                        args.insert("payload".to_string(), payload.clone());
                    }
                }

                let req = crate::agent::execution::ExecutionRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    agent_id: agent_label.to_string(),
                    tool_name: intent.action.clone(),
                    capability,
                    args: serde_json::Value::Object(args),
                    working_dir: crate::config::workspace_root(),
                    timestamp: chrono::Utc::now().timestamp(),
                    session_id: session_id.to_string(),
                };

                let res = del.execute(req).await;
                if res.success {
                    Ok(TaskStatus::Success { stdout: res.output })
                } else {
                    match res.error_code {
                        crate::agent::execution::result::DelErrorCode::YieldedForConsent { request_id, .. } => {
                            Ok(TaskStatus::YieldedForConsent { request_id })
                        }
                        crate::agent::execution::result::DelErrorCode::PermissionDenied { .. }
                        | crate::agent::execution::result::DelErrorCode::CommandBlocked { .. }
                        | crate::agent::execution::result::DelErrorCode::PathBlocked { .. } => {
                            Ok(TaskStatus::Blocked { blocked_by: 0 })
                        }
                        _ => Ok(TaskStatus::Failed {
                            exit_code: res.exit_code,
                            stderr: if !res.output.is_empty() {
                                res.output
                            } else {
                                res.denied_reason.unwrap_or_else(|| "Capability denied".to_string())
                            },
                        }),
                    }
                }
            }

            "opportunity:classify" => {
                let thread_id = payload["thread_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'thread_id'".into()))?;
                let status_str = payload["status"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'status'".into()))?;
                let status = crate::business::models::OpportunityStatus::from_str(status_str)
                    .ok_or_else(|| crate::MoatError::Orchestrator(format!("Invalid status '{}'", status_str)))?;

                let conn = db.lock().await;
                crate::business::db::update_thread_opportunity_status(&conn, thread_id, status)?;
                crate::business::db::update_opportunity_status(&conn, thread_id, status)?;
                Ok(TaskStatus::Success {
                    stdout: format!(
                        "[opportunity:classify] Opportunity status updated to '{}' for thread {}",
                        status_str, thread_id
                    ),
                })
            }

            "engagement:propose" => {
                let contact_id = payload["contact_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'contact_id'".into()))?;
                let thread_id = payload["thread_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'thread_id'".into()))?;
                let title = payload["title"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'title'".into()))?;
                let start_time = payload["start_time"]
                    .as_i64()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'start_time'".into()))?;
                let end_time = payload["end_time"]
                    .as_i64()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'end_time'".into()))?;

                let conn = db.lock().await;
                let eng = crate::business::workflow::propose_engagement(
                    &conn, contact_id, thread_id, title, start_time, end_time,
                )?;
                Ok(TaskStatus::Success {
                    stdout: format!(
                        "[engagement:propose] Propose engagement success: ID {}, status proposed",
                        eng.id
                    ),
                })
            }

            "engagement:request_authorization" => {
                let engagement_id = payload["engagement_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'engagement_id'".into()))?;
                let conn = db.lock().await;
                crate::business::workflow::request_authorization(&conn, engagement_id, "agent")?;
                Ok(TaskStatus::Success {
                    stdout: format!(
                        "[engagement:request_authorization] Engagement {} transitioned to pending_authorization",
                        engagement_id
                    ),
                })
            }

            "engagement:authorize" => {
                let engagement_id = payload["engagement_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'engagement_id'".into()))?;
                let conn = db.lock().await;
                crate::business::workflow::authorize_engagement(&conn, engagement_id)?;

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
                    .map_err(|e| crate::MoatError::Orchestrator(e))?;

                Ok(TaskStatus::Success {
                    stdout: format!(
                        "[engagement:authorize] Engagement {} authorized. Execution result: {}",
                        engagement_id, exec_result
                    ),
                })
            }

            "operator:escalate" => {
                let thread_id = payload["thread_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'thread_id'".into()))?;
                let reason = payload["reason"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'reason'".into()))?;
                let conn = db.lock().await;
                let esc = crate::business::escalation::escalate_thread(&conn, thread_id, reason)?;
                Ok(TaskStatus::Success {
                    stdout: format!(
                        "[operator:escalate] Escalation created: ID {}, thread {} escalated",
                        esc.id, thread_id
                    ),
                })
            }

            "knowledge:query" => {
                let query = payload["query"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'query'".into()))?;
                let limit = payload["limit"].as_u64().unwrap_or(3) as usize;
                let conn = db.lock().await;
                let results = crate::business::knowledge::query_knowledge_store_ranked(&conn, query, limit)?;
                let json_res = serde_json::to_string(&results)?;
                Ok(TaskStatus::Success { stdout: json_res })
            }

            "identity:resolve" => {
                let channel = payload["channel"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'channel'".into()))?;
                let external_id = payload["external_id"]
                    .as_str()
                    .ok_or_else(|| crate::MoatError::Orchestrator("Missing 'external_id'".into()))?;
                let conn = db.lock().await;
                let contact = crate::business::identity::resolve_contact(&conn, channel, external_id)?;
                Ok(TaskStatus::Success {
                    stdout: format!("[identity:resolve] Resolved to Contact ID: {}", contact.id),
                })
            }

            "memory:search" => {
                let query = match payload {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if let Some(prefix) = Self::load_active_memory_prefix_static(&db, &query).await? {
                    Ok(TaskStatus::Success { stdout: prefix })
                } else {
                    Ok(TaskStatus::Success { stdout: "No relevant memories found.".to_string() })
                }
            }

            "memory:learn" => {
                let keywords = payload["keywords"].as_str().unwrap_or("").trim();
                let skill_description = payload["skill_description"].as_str().unwrap_or("").trim();

                if keywords.is_empty() || skill_description.is_empty() {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: "memory:learn requires keywords and skill_description".to_string(),
                    });
                }

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let skill_block = format!(
                    "[LEARNED SKILL]\nKeywords: {}\nSkill: {}",
                    keywords, skill_description
                );

                let db = db.lock().await;
                crate::memory::store_knowledge(
                    &db,
                    crate::memory::KnowledgeKind::Fact,
                    "memory:learn",
                    Some(&format!("Skill: {}", keywords)),
                    &skill_block,
                    Some(timestamp),
                )?;

                Ok(TaskStatus::Success {
                    stdout: format!("Learned procedural skill.\n{}", skill_block),
                })
            }

            "meta:query_brain" => {
                let keyword = payload.as_str().unwrap_or("").to_string();
                let brain = meta_brain.read().await;
                let result = brain.extract_relevant_subgraph(&keyword);
                Ok(TaskStatus::Success { stdout: result })
            }

            unknown => {
                let tool_opt = {
                    let registry = crate::agent::tools::global_registry().read().unwrap();
                    registry.get(unknown).cloned()
                };

                if let Some(tool) = tool_opt {
                    let intent = ActionIntent {
                        thought: None,
                        action: unknown.to_string(),
                        target: payload.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        payload: Some(payload.clone()),
                        reason: None,
                    };
                    let res = tool.execute(&intent).await?;
                    Ok(res)
                } else {
                    Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!("unsupported action: {unknown}"),
                    })
                }
            }
        }
    }

    async fn visible_synthesized_skills(&self, query: &str) -> MoatResult<Vec<crate::memory::skills::SynthesizedSkill>> {
        let brain = self.meta_brain.read().await;
        let all_skills = brain.get_synthesized_skills();
        if query.trim().is_empty() {
            return Ok(all_skills);
        }
        let query_words: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(|w| w.to_string())
            .collect();
        if query_words.is_empty() {
            return Ok(all_skills);
        }
        let mut ranked: Vec<(usize, crate::memory::skills::SynthesizedSkill)> = all_skills
            .into_iter()
            .map(|skill| {
                let name_lower = skill.skill_name.to_lowercase();
                let desc_lower = skill.description.to_lowercase();
                let mut score = 0;
                for word in &query_words {
                    if name_lower.contains(word) {
                        score += 10;
                    }
                    if desc_lower.contains(word) {
                        score += 2;
                    }
                }
                (score, skill)
            })
            .filter(|(score, _)| *score > 0)
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(ranked.into_iter().map(|(_, skill)| skill).collect())
    }

    pub async fn run_agent_loop(
        &self,
        state: &mut AgentExecutionState,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> MoatResult<crate::agent::types::ReactOutcome> {
        use tracing::Instrument;
        let session_id_str = state.session_id.to_string();
        let react_span = tracing::info_span!(
            "react_loop",
            session_id = %session_id_str,
            objective = %state.objective
        );

        self.run_agent_loop_instrumented(state, status_tx)
            .instrument(react_span)
            .await
    }

    async fn run_agent_loop_instrumented(
        &self,
        state: &mut AgentExecutionState,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> MoatResult<crate::agent::types::ReactOutcome> {
        let mut results = Vec::new();
        let max_turns = state.scope_stack.last().map(|s| s.max_turns).unwrap_or(15);

        self.ensure_token_cache(state);

        if state.turns == 0 && !state.context_trace.iter().any(|m| m.content.as_deref().unwrap_or("").contains("active_memory_prelude")) {
            if let Some(prefix) = self.load_active_memory_prefix(&state.objective).await? {
                self.push_message(state, LlmMessage {
                    role: "system".into(),
                    content: Some(prefix),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            self.push_message(state, LlmMessage {
                role: "user".into(),
                content: Some(state.objective.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        loop {
            if self.cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(crate::agent::types::ReactOutcome::Completed {
                    results,
                    context_snapshot: state.context_trace.clone(),
                });
            }

            state.turns += 1;
            if state.turns > max_turns {
                results.push(SubTaskResult {
                    subtask_id: state.turns,
                    action: "system:limit".into(),
                    target: "react".into(),
                    status: TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!("Max turns ({}) exceeded.", max_turns),
                    },
                    elapsed: std::time::Duration::ZERO,
                    tokens_used: 0,
                });
                break;
            }

            self.compress_context_if_needed(state)?;

            let (active_agent_name, active_role_description, allowed_tools) = if let Some(scope) = state.scope_stack.last() {
                (scope.agent_name.clone(), scope.role_description.clone(), scope.allowed_tools.clone())
            } else {
                ("primary".to_string(), "Operator Assistant".to_string(), state.allowed_tools.clone())
            };

            let agent_spec = AgentSpec {
                name: active_agent_name.clone(),
                role: active_role_description.clone(),
                api_provider: "google".to_string(),
                model_id: "default-model".to_string(),
                allowed_tools: allowed_tools.clone(),
                raw_markdown: String::new(),
                authoritative: false,
            };

            let restricted_skills = self.visible_synthesized_skills(&state.objective).await?;
            let instruction_skills = {
                let brain = self.meta_brain.read().await;
                let all_skills = brain.get_instruction_skills();
                let mut relevant = Vec::new();
                for skill in all_skills {
                    let is_approved = skill.status.as_deref() != Some("draft");
                    if is_approved {
                        let mut score = 0;
                        let mut has_trigger_match = false;
                        let query_lower = state.objective.to_lowercase();
                        if skill.triggers.iter().any(|t| query_lower.contains(&t.to_lowercase())) {
                            score += 10;
                            has_trigger_match = true;
                        }
                        if query_lower.contains(&skill.name.to_lowercase()) {
                            score += 5;
                        }
                        if let Some(ref cat) = skill.category {
                            if query_lower.contains(&cat.to_lowercase()) {
                                score += 2;
                            }
                        }
                        if has_trigger_match || score >= 5 {
                            relevant.push((score, skill));
                        }
                    }
                }
                relevant.sort_by(|a, b| b.0.cmp(&a.0));
                relevant.into_iter().map(|(_, skill)| skill).collect::<Vec<_>>()
            };

            let system_prompt = build_react_system_prompt(
                Some(&agent_spec),
                &restricted_skills,
                &instruction_skills,
            );

            self.set_system_prompt(state, system_prompt);

            let mut cached_tools = self.build_llm_tools();
            if !allowed_tools.is_empty() {
                cached_tools.retain(|t| allowed_tools.contains(&t.function.name));
            }

            if let Some(ref tx) = status_tx {
                let _ = tx.send("__TOOL_STATUS__:🤔 Thinking...".to_string()).await;
            }

            let resp = self.provider.chat_complete(
                "gpt-4-turbo",
                state.context_trace.clone(),
                Some(cached_tools),
                2000,
                0.1,
                Some(agent_response_schema()),
            ).await?;

            state.total_tokens += resp.tokens as usize;

            let mut structured_opt = resp.structured.clone();
            if structured_opt.is_none() {
                if let Some(ref content) = resp.content {
                    if let Ok(parsed) = serde_json::from_str::<crate::agent::llm_types::AgentStructuredResponse>(content) {
                        structured_opt = Some(parsed);
                    }
                }
            }

            let mut tool_intents = Vec::new();
            let mut is_completed = false;
            let mut final_summary = String::new();

            if let Some(structured) = structured_opt {
                if structured.state == "completed" {
                    is_completed = true;
                    final_summary = structured.summary.unwrap_or_else(|| "Task completed.".to_string());
                } else {
                    for a in structured.actions {
                        let target = a.target.unwrap_or_else(|| "user".to_string());
                        let payload = a.payload.unwrap_or(serde_json::Value::Null);
                        tool_intents.push(ActionIntent {
                            action: a.action,
                            target,
                            payload: Some(payload),
                            thought: Some(structured.thought.clone()),
                            reason: a.reason,
                        });
                    }
                }
            } else if let Some(tool_calls) = resp.tool_calls.clone() {
                for t in tool_calls {
                    let action = t.function.name;
                    let mut target = "user".to_string();
                    let mut payload = serde_json::Value::Null;
                    let mut reason = None;
                    if let Ok(args) = serde_json::from_str::<serde_json::Value>(&t.function.arguments) {
                        if let Some(tgt) = args.get("target").and_then(|v| v.as_str()) {
                            target = tgt.to_string();
                        }
                        if let Some(pld) = args.get("payload") {
                            payload = pld.clone();
                        }
                        if let Some(r) = args.get("reason").and_then(|v| v.as_str()) {
                            reason = Some(r.to_string());
                        }
                    }
                    tool_intents.push(ActionIntent {
                        action,
                        target,
                        payload: Some(payload),
                        thought: resp.content.clone(),
                        reason,
                    });
                }
            }

            if is_completed {
                let _ = self.distill_and_save_workflow(state, &results).await;

                results.push(SubTaskResult {
                    subtask_id: state.turns,
                    action: "done".into(),
                    target: "user".into(),
                    status: TaskStatus::Success {
                        stdout: final_summary.clone(),
                    },
                    elapsed: std::time::Duration::ZERO,
                    tokens_used: resp.tokens,
                });
                break;
            }

            if tool_intents.is_empty() {
                self.push_message(state, LlmMessage {
                    role: "system".into(),
                    content: Some("[SYSTEM: No action determined. Please select an action to run or complete with state 'completed'.]".to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                });
                continue;
            }

            let mut pending_consent_yield = None;
            for intent in tool_intents {
                if !allowed_tools.is_empty() && !allowed_tools.contains(&intent.action) && intent.action != "done" {
                    let err = format!("Action '{}' is not allowed for this agent.", intent.action);
                    self.push_message(state, LlmMessage {
                        role: "system".into(),
                        content: Some(format!("FAILED: {}", err)),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                    results.push(SubTaskResult {
                        subtask_id: state.turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: err,
                        },
                        elapsed: std::time::Duration::ZERO,
                        tokens_used: 0,
                    });
                    continue;
                }

                if intent.action == "system:spawn_worker" || intent.action == "swarm:delegate" {
                    if let Some(ref payload) = intent.payload {
                        if let Ok(spawn_args) = serde_json::from_value::<crate::agent::types::SubSpawnPayload>(payload.clone()) {
                            let new_scope = VirtualScope {
                                agent_name: spawn_args.agent_id.clone(),
                                role_description: spawn_args.specialization.unwrap_or_else(|| spawn_args.task_instruction.clone()),
                                allowed_tools: Vec::new(),
                                max_turns: 10,
                            };
                            state.scope_stack.push(new_scope);
                            self.push_message(state, LlmMessage {
                                role: "system".into(),
                                content: Some(format!(
                                    "[SYSTEM: Delegating to agent '{}' with task: '{}']",
                                    spawn_args.agent_id, spawn_args.task_instruction
                                )),
                                tool_calls: None,
                                tool_call_id: None,
                            });
                            results.push(SubTaskResult {
                                subtask_id: state.turns,
                                action: intent.action.clone(),
                                target: intent.target.clone(),
                                status: TaskStatus::Success {
                                    stdout: format!("Delegated to {}", spawn_args.agent_id),
                                },
                                elapsed: std::time::Duration::ZERO,
                                tokens_used: 0,
                            });
                            continue;
                        }
                    }
                }

                if intent.action == "done" {
                    if !state.scope_stack.is_empty() {
                        let completed_scope = state.scope_stack.pop().unwrap();
                        let response_text = intent.payload.as_ref().map(|v| v.as_str().unwrap_or("")).unwrap_or("");
                        self.push_message(state, LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "[SYSTEM: Sub-agent '{}' completed task. Output: '{}']",
                                completed_scope.agent_name, response_text
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                        results.push(SubTaskResult {
                            subtask_id: state.turns,
                            action: intent.action.clone(),
                            target: intent.target.clone(),
                            status: TaskStatus::Success {
                                stdout: format!("Sub-agent completed: {}", response_text),
                            },
                            elapsed: std::time::Duration::ZERO,
                            tokens_used: 0,
                        });
                        continue;
                    } else {
                        is_completed = true;
                        final_summary = intent.payload.as_ref().map(|v| v.as_str().unwrap_or("")).unwrap_or("").to_string();
                        break;
                    }
                }

                if let Some(ref tx) = status_tx {
                    let _ = tx.send(format!("__TOOL_STATUS__:🛠️ {} → {}", intent.action, intent.target)).await;
                }

                let status = self.execute_tool(&intent, &state.session_id.to_string(), &active_agent_name).await?;

                if let TaskStatus::YieldedForConsent { ref request_id } = status {
                    pending_consent_yield = Some((intent.clone(), request_id.clone()));
                    break;
                }

                let obs_content = match &status {
                    TaskStatus::Success { stdout } => format!("SUCCESS: {}", stdout),
                    TaskStatus::SoftDenial { reason } => format!("SOFT DENIAL: {}", reason),
                    TaskStatus::Failed { stderr, .. } => format!("FAILED: {}", stderr),
                    TaskStatus::Blocked { .. } => "BLOCKED".to_string(),
                    TaskStatus::Cancelled { stage } => format!("CANCELLED: {}", stage),
                    _ => "UNKNOWN".to_string(),
                };

                self.push_message(state, LlmMessage {
                    role: "system".into(),
                    content: Some(format!(
                        "Turn {} INTENT: {} | TARGET: {}\nOBSERVATION: {}",
                        state.turns, intent.action, intent.target, obs_content
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                });

                results.push(SubTaskResult {
                    subtask_id: state.turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status,
                    elapsed: std::time::Duration::ZERO,
                    tokens_used: resp.tokens,
                });
            }

            if is_completed {
                let _ = self.distill_and_save_workflow(state, &results).await;

                results.push(SubTaskResult {
                    subtask_id: state.turns,
                    action: "done".into(),
                    target: "user".into(),
                    status: TaskStatus::Success {
                        stdout: final_summary,
                    },
                    elapsed: std::time::Duration::ZERO,
                    tokens_used: 0,
                });
                break;
            }

            if let Some((yield_intent, request_id)) = pending_consent_yield {
                let continuation = crate::agent::types::ReactContinuation {
                    context_trace: state.context_trace.clone(),
                    turns: state.turns,
                    searches: 0,
                    session_searches: 0,
                    total_tokens: state.total_tokens as u32,
                    delegate_count: 0,
                    high_risk_approvals: 0,
                    loop_window: self.loop_detector.snapshot(),
                    pending_tool_call: yield_intent.clone(),
                    delegation_stack: state.scope_stack.iter().map(|s| s.agent_name.clone()).collect(),
                };

                return Ok(crate::agent::types::ReactOutcome::Yielded(
                    crate::agent::types::YieldStatus::AwaitingConsent {
                        request_id,
                        tool_name: yield_intent.action,
                        target: yield_intent.target,
                        continuation,
                    }
                ));
            }
        }

        Ok(crate::agent::types::ReactOutcome::Completed {
            results,
            context_snapshot: state.context_trace.clone(),
        })
    }
}
