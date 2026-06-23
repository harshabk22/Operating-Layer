#![allow(dead_code)]
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use url::Url;

use crate::memory::meta_brain::{MetaBrain, PermissionFlags};

use crate::{MoatError, MoatResult};

pub use crate::agent::llm_types::*;

pub use crate::planner::{PlannerAgent, PlannerConfig, WorkerConfig};

// ─────────────────────────────────────────────────────────────────────────────
// § ReAct Schema (Indestructible Payload)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntent {
    pub thought: Option<String>,
    pub action: String,
    pub target: String,
    pub payload: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AskChoicePayload {
    pub(crate) context: String,
    pub(crate) options: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LearnPayload {
    pub(crate) keywords: String,
    pub(crate) skill_description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SubSpawnPayload {
    #[serde(alias = "target_agent_id")]
    pub(crate) agent_id: String,
    #[serde(alias = "task")]
    pub(crate) task_instruction: String,
    #[serde(default, alias = "role")]
    pub(crate) specialization: Option<String>,
}

/// Bridge trait for Visual-Semantic Driver UI interactions.
/// Implemented in the Tauri layer where AX accessibility APIs are available.
pub trait UiActionBridge: Send + Sync {
    /// Click an element by its scraped ID. Returns a success message or error.
    fn click(&self, element_id: usize) -> Result<String, String>;
    /// Type text into an element by its scraped ID. Returns a success message or error.
    fn type_text(&self, element_id: usize, text: &str) -> Result<String, String>;
    /// Re-scrape the active window's accessibility tree.
    /// Returns the flat text representation for LLM context injection,
    /// and updates the internal element map for subsequent click/type_text calls.
    fn refresh_ui_tree(&self) -> Result<String, String>;
    /// Returns the name of the currently active (focused) application, if detectable.
    fn active_app_name(&self) -> Option<String>;
}

pub use crate::agent::tools::ToolReversibility;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Success {
        stdout: String,
    },
    /// Tool executed successfully but the semantic result is negative/empty.
    /// E.g., "No permissions found", "No relevant memories", empty search results.
    /// The orchestrator handles these mechanically — the LLM never decides routing.
    SoftDenial {
        reason: String,
    },
    Failed {
        exit_code: Option<i32>,
        stderr: String,
    },
    Blocked {
        blocked_by: usize,
    },
    /// Tool execution was aborted mid-flight by a barge-in signal.
    Cancelled {
        stage: String,
    },
    /// Agent must yield for user consent — loop MUST pause and persist state
    YieldedForConsent {
        request_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskResult {
    pub subtask_id: usize,
    pub action: String,
    pub target: String,
    pub status: TaskStatus,
    pub elapsed: Duration,
    pub tokens_used: u32,
}

#[derive(Debug)]
pub struct RiskBudget {
    pub session_id: uuid::Uuid,
    pub approvals: std::sync::atomic::AtomicUsize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactContinuation {
    pub context_trace: Vec<LlmMessage>,
    pub turns: usize,
    pub searches: usize,
    #[serde(default)]
    pub session_searches: usize,
    pub total_tokens: u32,
    pub delegate_count: usize,
    pub high_risk_approvals: usize,
    pub loop_window: Vec<ToolExecution>,
    pub pending_tool_call: ActionIntent,
    #[serde(default)]
    pub delegation_stack: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum YieldStatus {
    AwaitingSubAgent {
        agent_id: String,
        task_instruction: String,
        original_tool_call: ActionIntent,
        continuation: ReactContinuation,
    },
    AwaitingConsent {
        request_id: String,
        tool_name: String,
        target: String,
        continuation: ReactContinuation,
    },
}

#[derive(Debug, Clone)]
pub enum ReactOutcome {
    Completed {
        results: Vec<SubTaskResult>,
        context_snapshot: Vec<LlmMessage>,
    },
    Yielded(YieldStatus),
    /// Agent was hard-killed by the Execution Kernel for severe policy violations
    /// (3 consecutive execution denials). Parent must NOT treat this as success.
    Terminated(String),
}

use crate::agent::worker::consent::value_to_string;

pub(crate) fn extract_inject_text_payload(v: Option<&Value>) -> Option<(String, String)> {
    match v {
        Some(Value::Object(object)) => {
            let text = object
                .get("text_to_inject")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)?;
            let expected_app_name = object
                .get("expected_app_name")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)?;
            Some((text, expected_app_name))
        }
        _ => None,
    }
}

pub(crate) fn active_app_matches(expected_app_name: &str, bridge: &dyn UiActionBridge) -> bool {
    match bridge.active_app_name() {
        Some(name) => name.trim().eq_ignore_ascii_case(expected_app_name.trim()),
        None => false,
    }
}

pub(crate) fn extract_curl_options(v: Option<&Value>) -> (String, String, Vec<(String, String)>) {
    let default_method = "GET".to_string();
    let default_body = String::new();
    let default_headers = vec![];
    let obj = match v {
        Some(Value::Object(m)) => m,
        _ => return (default_method, default_body, default_headers),
    };
    let method = obj
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .to_ascii_uppercase();
    let body = obj
        .get("body")
        .map(|b| value_to_string(Some(b)))
        .unwrap_or_default();
    let headers = match obj.get("headers") {
        Some(Value::Object(hmap)) => hmap
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
            .collect(),
        _ => vec![],
    };
    (method, body, headers)
}

pub(crate) fn extract_host(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed.host_str().map(|h| h.to_ascii_lowercase())
}

#[derive(Debug, Clone)]
pub(crate) enum PermissionGrant {
    Path {
        path: String,
        permissions: PermissionFlags,
    },
    App {
        app: String,
        permissions: PermissionFlags,
    },
    UrlScheme {
        scheme: String,
        permissions: PermissionFlags,
    },
    Tool {
        tool: String,
        permissions: PermissionFlags,
    },
}

impl PermissionGrant {
    pub(crate) fn description(&self) -> (String, String) {
        match self {
            Self::Path { path, permissions } => (
                "permission:file".into(),
                format!(
                    "Grant {} access to {}",
                    permission_flags_label(*permissions),
                    path
                ),
            ),
            Self::App { app, permissions } => (
                "permission:app".into(),
                format!(
                    "Grant {} access to app {}",
                    permission_flags_label(*permissions),
                    app
                ),
            ),
            Self::UrlScheme {
                scheme,
                permissions,
            } => (
                "permission:url_scheme".into(),
                format!(
                    "Grant {} access to URL scheme {}",
                    permission_flags_label(*permissions),
                    scheme
                ),
            ),
            Self::Tool { tool, permissions } => (
                "permission:tool".into(),
                format!(
                    "Grant {} access to tool {}",
                    permission_flags_label(*permissions),
                    tool
                ),
            ),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn apply(&self, brain: &mut MetaBrain) -> MoatResult<()> {
        match self {
            Self::Path { path, permissions } => {
                brain.grant_path_permission(path, *permissions)?;
            }
            Self::App { app, permissions } => {
                brain.grant_app_permission(app, *permissions)?;
            }
            Self::UrlScheme {
                scheme,
                permissions,
            } => {
                brain.grant_url_scheme_permission(scheme, *permissions)?;
            }
            Self::Tool { tool, permissions } => {
                brain.grant_tool_permission(tool, *permissions)?;
            }
        }
        Ok(())
    }

    pub(crate) fn apply_scoped(
        &self,
        brain: &mut MetaBrain,
        agent_id: &str,
        session_id: &str,
    ) -> MoatResult<()> {
        match self {
            Self::Path { path, permissions } => {
                brain.grant_path_permission_scoped(path, *permissions, agent_id, session_id)?;
            }
            Self::App { app, permissions } => {
                brain.grant_app_permission_scoped(app, *permissions, agent_id, session_id)?;
            }
            Self::UrlScheme {
                scheme,
                permissions,
            } => {
                brain.grant_url_scheme_permission_scoped(
                    scheme,
                    *permissions,
                    agent_id,
                    session_id,
                )?;
            }
            Self::Tool { tool, permissions } => {
                brain.grant_tool_permission_scoped(tool, *permissions, agent_id, session_id)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn permission_flags_label(flags: PermissionFlags) -> String {
    let mut labels = Vec::new();
    if flags.contains(PermissionFlags::FILE_READ) {
        labels.push("read");
    }
    if flags.contains(PermissionFlags::FILE_WRITE) {
        labels.push("write");
    }
    if flags.contains(PermissionFlags::NET_OUTBOUND) {
        labels.push("network");
    }
    if flags.contains(PermissionFlags::APP_LAUNCH) {
        labels.push("app-launch");
    }
    if flags.contains(PermissionFlags::URL_OPEN) {
        labels.push("url-open");
    }
    if flags.contains(PermissionFlags::TOOL_EXEC) || flags.contains(PermissionFlags::EXEC) {
        labels.push("execute");
    }
    if labels.is_empty() {
        "no".into()
    } else {
        labels.join("+")
    }
}

#[allow(dead_code)]
pub(crate) fn run_command(cmd: &mut Command, timeout: Duration) -> MoatResult<TaskStatus> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        MoatError::Orchestrator(format!("Command execution failed to start: {}", e))
    })?;

    let Some(mut stdout_pipe) = child.stdout.take() else {
        return Err(MoatError::Orchestrator(
            "Command stdout pipe was not available".into(),
        ));
    };
    let Some(mut stderr_pipe) = child.stderr.take() else {
        return Err(MoatError::Orchestrator(
            "Command stderr pipe was not available".into(),
        ));
    };

    let out_thread = thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout_pipe.read_to_string(&mut s);
        s
    });
    let err_thread = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr_pipe.read_to_string(&mut s);
        s
    });

    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| MoatError::Orchestrator(format!("Command wait failed: {}", e)))?
        {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(TaskStatus::Failed {
                exit_code: None,
                stderr: format!("Command timed out after {:?}", timeout),
            });
        }
        thread::sleep(Duration::from_millis(10));
    };

    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    if status.success() {
        Ok(TaskStatus::Success { stdout })
    } else {
        Ok(TaskStatus::Failed {
            exit_code: status.code(),
            stderr,
        })
    }
}

pub(crate) fn expected_value_type(schema_value: &Value) -> &'static str {
    match schema_value {
        Value::String(kind) => match kind.trim().to_ascii_lowercase().as_str() {
            "string" => "string",
            "number" => "number",
            "integer" => "integer",
            "boolean" => "boolean",
            "array" => "array",
            "object" => "object",
            "any" => "any",
            _ => "unknown",
        },
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "any",
    }
}

fn normalize_lexically(path: &std::path::Path) -> std::path::PathBuf {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            std::path::Component::Normal(c) => {
                normalized.push(c);
            }
            std::path::Component::RootDir => {
                normalized.push(std::path::Component::RootDir);
            }
            std::path::Component::Prefix(p) => {
                normalized.push(std::path::Component::Prefix(p));
            }
        }
    }
    normalized
}

pub(crate) fn is_blocked_process_spawn_binary(binary: &str) -> bool {
    let trimmed = binary.trim();
    if trimmed.is_empty() {
        return true;
    }
    let path = std::path::Path::new(trimmed);
    let normalized = normalize_lexically(path);
    let mut existing = normalized.clone();
    let mut suffix = std::path::PathBuf::new();
    while !existing.as_os_str().is_empty() && !existing.exists() {
        if let Some(parent) = existing.parent() {
            if let Some(file_name) = existing.file_name() {
                suffix = std::path::PathBuf::from(file_name).join(suffix);
            }
            existing = parent.to_path_buf();
        } else {
            break;
        }
    }
    let resolved = if !existing.as_os_str().is_empty() && existing.exists() {
        if let Ok(canonical) = std::fs::canonicalize(&existing) {
            canonical.join(suffix)
        } else {
            normalized
        }
    } else {
        normalized
    };
    if let Some(basename) = resolved.file_name().and_then(|n| n.to_str()) {
        let lower = basename.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "bash" | "sh" | "zsh" | "python" | "python3" | "ruby" | "perl" | "node"
        )
    } else {
        let lower = trimmed.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "bash" | "sh" | "zsh" | "python" | "python3" | "ruby" | "perl" | "node"
        )
    }
}

pub(crate) fn actual_value_type(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(number) if number.as_i64().is_some() || number.as_u64().is_some() => {
            "integer"
        }
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

pub(crate) fn value_matches_schema(value: &Value, schema_value: &Value) -> bool {
    match expected_value_type(schema_value) {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "any" => true,
        "unknown" => true,
        _ => true,
    }
}

pub(crate) fn schema_value_for_error(schema_value: &Value) -> String {
    match schema_value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(crate) fn payload_value_to_cli_arg(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn build_synthesized_skill_args(
    payload: Option<&Value>,
    schema_json: &str,
) -> Result<Vec<String>, String> {
    let schema: Value = serde_json::from_str(schema_json)
        .map_err(|err| format!("invalid installed parameter schema: {err}"))?;
    let schema_object = schema
        .as_object()
        .ok_or_else(|| "installed parameter schema must be a JSON object".to_string())?;
    let payload_object = payload
        .and_then(Value::as_object)
        .ok_or_else(|| "synthesized skill payload must be a JSON object".to_string())?;

    for required_key in schema_object.keys() {
        if !payload_object.contains_key(required_key) {
            return Err(format!(
                "missing required skill parameter '{}'",
                required_key
            ));
        }
    }
    for key in payload_object.keys() {
        if !schema_object.contains_key(key) {
            return Err(format!("unexpected skill parameter '{}'", key));
        }
    }

    let mut ordered_keys = schema_object.keys().cloned().collect::<Vec<_>>();
    ordered_keys.sort();

    let mut args = Vec::with_capacity(ordered_keys.len() * 2);
    for key in ordered_keys {
        let schema_value = schema_object
            .get(&key)
            .ok_or_else(|| format!("missing installed schema for '{}'", key))?;
        let value = payload_object
            .get(&key)
            .ok_or_else(|| format!("missing payload value for '{}'", key))?;
        if !value_matches_schema(value, schema_value) {
            return Err(format!(
                "parameter '{}' expected {}, got {}",
                key,
                schema_value_for_error(schema_value),
                actual_value_type(value)
            ));
        }
        args.push(format!("--{}", key.replace('_', "-")));
        args.push(payload_value_to_cli_arg(value));
    }
    Ok(args)
}

pub(crate) struct TempWorkspaceSkillScript {
    pub(crate) absolute_path: PathBuf,
}

impl Drop for TempWorkspaceSkillScript {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.absolute_path);
    }
}

pub use crate::agent::worker::WorkerAgent;

// ─────────────────────────────────────────────────────────────────────────────
// § Re-exports from extracted submodules
// ─────────────────────────────────────────────────────────────────────────────

pub use crate::agent::interaction_loop::context::{SharedContext, VersionedContext};
pub use crate::agent::interaction_loop::loop_detector::{
    normalize_result_string, LoopDetector, LoopVerdict, ToolExecution,
};
pub use crate::agent::interaction_loop::Orchestrator;

#[cfg(test)]
mod tests {
    fn authorize_path(
        path_str: &str,
        allowed_roots: &[String],
        _is_write: bool,
    ) -> Result<(), String> {
        let path = std::path::PathBuf::from(path_str);
        let resolved = if let Ok(canonical) = std::fs::canonicalize(&path) {
            canonical
        } else {
            let parent = path.parent().unwrap_or(&path);
            if let Ok(canonical_parent) = std::fs::canonicalize(parent) {
                canonical_parent.join(path.file_name().unwrap_or_default())
            } else {
                path.clone()
            }
        };

        for root_str in allowed_roots {
            let root_path = std::path::PathBuf::from(root_str);
            if let Ok(canonical_root) = std::fs::canonicalize(&root_path) {
                if resolved.starts_with(&canonical_root) {
                    return Ok(());
                }
            } else if resolved.starts_with(&root_path) {
                return Ok(());
            }
        }
        Err(format!("Path not allowed: {:?}", resolved))
    }
    use super::{
        ActionIntent, Orchestrator, PlannerAgent, PlannerConfig, TaskStatus, WorkerAgent,
        WorkerConfig,
    };
    use crate::agent::consent::{ConsentBridge, ConsentDecision, ConsentRequest};
    use crate::agent::injector::InjectorConfig;
    use crate::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse, LlmTool, LlmToolCall};
    use crate::config;
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::{MetaBrain, PermissionFlags, SYSTEM_ROOT_ID};
    use crate::memory::{self, KnowledgeKind};
    use crate::planner::prompt::REACT_SYSTEM_TEMPLATE;
    use crate::planner::PLANNER_REPLY_TIMEOUT;

    use crate::MoatResult;
    use rusqlite::Connection;
    use serde_json::{json, Value};
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};

    fn temp_dir(name: &str) -> PathBuf {
        let root = config::workspace_root().join("test_scratch");
        fs::create_dir_all(&root).expect("create workspace test root");
        let p = root.join(format!(
            "moat_orchestrator_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp test dir");
        p
    }

    fn prepare_test_db(path: &PathBuf) -> Arc<AsyncMutex<Connection>> {
        let conn = Connection::open(path).expect("open sqlite db");
        memory::prepare_tables(&conn).expect("prepare knowledge tables");
        crate::memory::meta_brain::prepare_policy_table(&conn).expect("prepare policy table");
        Arc::new(AsyncMutex::new(conn))
    }

    fn consent_bridge_with_policy<F>(
        policy: F,
        counter: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<ConsentRequest>>>,
    ) -> Arc<ConsentBridge>
    where
        F: Fn(usize, &ConsentRequest) -> bool + Send + Sync + 'static,
    {
        let bridge_slot = Arc::new(Mutex::new(None::<std::sync::Weak<ConsentBridge>>));
        let bridge_slot_for_emit = Arc::clone(&bridge_slot);
        let policy = Arc::new(policy);
        let bridge = Arc::new(ConsentBridge::new(Box::new(move |req: ConsentRequest| {
            let approval_index = counter.fetch_add(1, Ordering::SeqCst) + 1;
            requests.lock().expect("lock requests").push(req.clone());
            let approved = policy(approval_index, &req);
            let weak = bridge_slot_for_emit
                .lock()
                .expect("lock bridge slot")
                .clone()
                .expect("bridge stored");
            std::thread::spawn(move || {
                if let Some(bridge) = weak.upgrade() {
                    let decision = if approved {
                        ConsentDecision::ApproveOnce
                    } else {
                        ConsentDecision::Deny
                    };
                    let _ = bridge.resolve(&req.request_id, decision);
                }
            });
        })));
        *bridge_slot.lock().expect("lock bridge slot") = Some(Arc::downgrade(&bridge));
        bridge
    }

    fn test_worker(
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
        consent_bridge: Option<Arc<ConsentBridge>>,
    ) -> WorkerAgent {
        let config = WorkerConfig {
            model: "test-model".into(),
            command_timeout: Duration::from_secs(1),
            allow_high_risk: true,
            external_apis: ExternalApiConfig::default(),
        };
        match consent_bridge {
            Some(bridge) => WorkerAgent::with_consent_bridge(config, meta_brain, db, bridge)
                .expect("test worker with consent"),
            None => WorkerAgent::new(config, meta_brain, db).expect("test worker"),
        }
    }

    fn test_injector_config() -> InjectorConfig {
        InjectorConfig {
            top_k_memories: 3,
            max_memory_chars: 512,
        }
    }

    struct StubProvider {
        responses: Arc<Mutex<VecDeque<(String, u32)>>>,
    }

    impl StubProvider {
        fn new(responses: Vec<(String, u32)>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
            }
        }
    }

    impl LlmProvider for StubProvider {
        fn chat_complete<'a>(
            &'a self,
            _model: &'a str,
            _messages: Vec<LlmMessage>,
            _tools: Option<Vec<LlmTool>>,
            _max_tokens: usize,
            _temperature: f32,
            _response_format: Option<serde_json::Value>,
        ) -> super::BoxFuture<'a, MoatResult<LlmResponse>> {
            Box::pin(async move {
                let next = self
                    .responses
                    .lock()
                    .expect("lock stub responses")
                    .pop_front()
                    .expect("stub response");

                let cleaned = next
                    .0
                    .replace("<action>", "")
                    .replace("</action>", "")
                    .trim()
                    .to_string();
                let intent: ActionIntent =
                    serde_json::from_str(&cleaned).unwrap_or_else(|_| ActionIntent {
                        reason: None,
                        thought: None,
                        action: "done".into(),
                        target: "user".into(),
                        payload: Some(serde_json::Value::String(cleaned)),
                    });

                Ok(LlmResponse {
                    content: None,
                    tool_calls: Some(vec![LlmToolCall {
                        id: "call_1".into(),
                        type_: "function".into(),
                        function: crate::agent::interaction_orchestrator::LlmFunctionCall {
                            name: intent.action,
                            arguments: serde_json::json!({"target": intent.target, "payload": intent.payload}).to_string()
                        }
                    }]),
                    tokens: next.1,
                    structured: None,
                })
            })
        }
    }

    struct HangingProvider;

    impl LlmProvider for HangingProvider {
        fn chat_complete<'a>(
            &'a self,
            _model: &'a str,
            _messages: Vec<LlmMessage>,
            _tools: Option<Vec<LlmTool>>,
            _max_tokens: usize,
            _temperature: f32,
            _response_format: Option<serde_json::Value>,
        ) -> super::BoxFuture<'a, MoatResult<LlmResponse>> {
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_secs(31)).await;
                Ok(LlmResponse {
                    content: Some("late".into()),
                    tool_calls: None,
                    tokens: 1,
                    structured: None,
                })
            })
        }
    }

    struct HangingPlannerProvider;

    impl LlmProvider for HangingPlannerProvider {
        fn chat_complete<'a>(
            &'a self,
            _model: &'a str,
            _messages: Vec<LlmMessage>,
            _tools: Option<Vec<LlmTool>>,
            _max_tokens: usize,
            _temperature: f32,
            _response_format: Option<serde_json::Value>,
        ) -> super::BoxFuture<'a, MoatResult<LlmResponse>> {
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Ok(LlmResponse {
                    content: None,
                    tool_calls: Some(vec![LlmToolCall {
                        id: "call_2".into(),
                        type_: "function".into(),
                        function: crate::agent::interaction_orchestrator::LlmFunctionCall {
                            name: "done".into(),
                            arguments: serde_json::json!({"target": "user", "payload": "late"})
                                .to_string(),
                        },
                    }]),
                    tokens: 1,
                    structured: None,
                })
            })
        }
    }

    #[test]
    fn authorize_write_blocks_prefix_confusion() {
        let base = temp_dir("prefix");
        let root = base.join("allowed");
        let evil = base.join("allowed_evil");
        fs::create_dir_all(&root).expect("create allowed dir");
        fs::create_dir_all(&evil).expect("create evil dir");

        let target = evil.join("x.txt");
        let result = authorize_path(
            target.to_str().expect("target path"),
            &[root.to_string_lossy().into_owned()],
            true,
        );
        assert!(result.is_err(), "prefix-confused path should be denied");
    }

    #[cfg(unix)]
    #[test]
    fn authorize_read_blocks_symlink_escape() {
        use std::os::unix::fs::symlink;

        let base = temp_dir("symlink");
        let root = base.join("allowed");
        let outside = base.join("outside");
        fs::create_dir_all(&root).expect("create allowed dir");
        fs::create_dir_all(&outside).expect("create outside dir");

        let outside_file = outside.join("secret.txt");
        fs::write(&outside_file, "top-secret").expect("write outside file");

        let link_path = root.join("link_out");
        symlink(&outside, &link_path).expect("create symlink");

        let escaped = link_path.join("secret.txt");
        let result = authorize_path(
            escaped.to_str().expect("escaped path"),
            &[root.to_string_lossy().into_owned()],
            false,
        );
        assert!(result.is_err(), "symlink escape must be denied");
    }

    #[test]
    fn can_open_url_accepts_mailto_and_denies_unlisted_schemes() {
        let mut brain = MetaBrain::new();
        brain
            .grant_url_scheme_permission("mailto", PermissionFlags::URL_OPEN)
            .expect("grant mailto");

        assert!(brain.url_scheme_has_permission("mailto", PermissionFlags::URL_OPEN));
        assert!(!brain.url_scheme_has_permission("ftp", PermissionFlags::URL_OPEN));
    }

    #[tokio::test]
    async fn file_write_permission_persists_and_skips_second_prompt() {
        let base = temp_dir("file_write_e2e");
        let db_path = base.join("memory.sqlite");
        let target = base.join("notes.txt");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let approval_count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let bridge = consent_bridge_with_policy(
            |_index, _req| true,
            Arc::clone(&approval_count),
            Arc::clone(&requests),
        );
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), Some(bridge));
        let intent = ActionIntent {
            reason: None,
            thought: None,
            action: "fs:write".into(),
            target: target.to_string_lossy().into_owned(),
            payload: Some(json!("hello from moat")),
        };
        let budget = Arc::new(super::RiskBudget {
            session_id: uuid::Uuid::new_v4(),
            approvals: AtomicUsize::new(0),
        });

        let first = worker
            .execute_capability_task(
                &intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("first write");
        assert!(matches!(first, TaskStatus::Success { .. }));
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let second = worker
            .execute_capability_task(
                &intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("second write");
        assert!(matches!(second, TaskStatus::Success { .. }));
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let parent = target
            .parent()
            .expect("parent")
            .to_string_lossy()
            .into_owned();
        let brain = meta_brain.read().await;
        assert!(brain.path_has_permission_scoped(
            &parent,
            PermissionFlags::FILE_WRITE,
            "tester",
            "test-session"
        ));
        drop(brain);

        let persisted = MetaBrain::load_or_new(db_path.with_extension("bin"))
            .await
            .expect("load persisted brain");
        assert!(persisted.path_has_permission_scoped(
            &parent,
            PermissionFlags::FILE_WRITE,
            "tester",
            "test-session"
        ));
        assert_eq!(requests.lock().expect("lock requests").len(), 1);
    }

    #[tokio::test]
    async fn file_read_permission_persists_and_skips_second_prompt() {
        let base = temp_dir("file_read_e2e");
        let db_path = base.join("memory.sqlite");
        let target = base.join("readme.txt");
        fs::write(&target, "read me").expect("seed read file");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let approval_count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let bridge = consent_bridge_with_policy(
            |_index, _req| true,
            Arc::clone(&approval_count),
            Arc::clone(&requests),
        );
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), Some(bridge));
        let intent = ActionIntent {
            reason: None,
            thought: None,
            action: "fs:read".into(),
            target: target.to_string_lossy().into_owned(),
            payload: None,
        };
        let budget = Arc::new(super::RiskBudget {
            session_id: uuid::Uuid::new_v4(),
            approvals: AtomicUsize::new(0),
        });

        let first = worker
            .execute_capability_task(
                &intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("first read");
        assert!(matches!(first, TaskStatus::Success { .. }));
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let second = worker
            .execute_capability_task(
                &intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("second read");
        assert!(matches!(second, TaskStatus::Success { .. }));
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let brain = meta_brain.read().await;
        assert!(brain.path_has_permission_scoped(
            target.to_string_lossy().as_ref(),
            PermissionFlags::FILE_READ,
            "tester",
            "test-session"
        ));
        assert_eq!(requests.lock().expect("lock requests").len(), 1);
    }

    #[tokio::test]
    async fn planner_agent_times_out_when_provider_stalls() {
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "stub".into(),
                max_subtasks: 4,
                temperature: 0.0,
                max_tokens: 64,
            },
            Arc::new(HangingPlannerProvider),
        );

        let err = planner
            .generate_intent_raw_with_system(REACT_SYSTEM_TEMPLATE, "Turn 1: hi", None)
            .await
            .expect_err("stalled planner provider should time out");

        assert!(
            err.to_string().contains(&format!(
                "planner provider timed out after {}s",
                PLANNER_REPLY_TIMEOUT.as_secs()
            )),
            "unexpected timeout error: {err}"
        );
    }

    #[tokio::test]
    async fn domain_permission_stays_scoped_to_approved_branch() {
        let base = temp_dir("domain_scope_e2e");
        let db_path = base.join("memory.sqlite");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let approval_count = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let bridge = consent_bridge_with_policy(
            |index, _req| index == 1,
            Arc::clone(&approval_count),
            Arc::clone(&requests),
        );
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), Some(bridge));
        let allowed_intent = ActionIntent {
            reason: None,
            thought: None,
            action: "net:curl".into(),
            target: "https://api.github.com/".into(),
            payload: Some(json!({ "method": "GET" })),
        };
        let sibling_intent = ActionIntent {
            reason: None,
            thought: None,
            action: "net:curl".into(),
            target: "https://example.com/".into(),
            payload: Some(json!({ "method": "GET" })),
        };

        let budget = Arc::new(super::RiskBudget {
            session_id: uuid::Uuid::new_v4(),
            approvals: AtomicUsize::new(0),
        });

        let _ = worker
            .execute_capability_task(
                &allowed_intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("first curl");
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let _ = worker
            .execute_capability_task(
                &allowed_intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("second curl same domain");
        assert_eq!(approval_count.load(Ordering::SeqCst), 1);

        let denied = worker
            .execute_capability_task(
                &sibling_intent,
                SYSTEM_ROOT_ID,
                "test-session",
                "tester",
                Arc::clone(&budget),
                None,
            )
            .await
            .expect("sibling domain curl");
        assert_eq!(approval_count.load(Ordering::SeqCst), 2);
        match denied {
            TaskStatus::Failed { stderr, .. } => {
                assert!(stderr.contains("example.com") || stderr.contains("denied"));
            }
            other => panic!("expected denied sibling domain, got {other:?}"),
        }

        let brain = meta_brain.read().await;
        let subtree = brain
            .focused_subgraph(SYSTEM_ROOT_ID, "curl api github", 4, 3)
            .expect("extract subtree");
        assert!(subtree
            .allowed_domains
            .iter()
            .any(|domain| domain == "api.github.com"));
        assert!(!subtree
            .allowed_domains
            .iter()
            .any(|domain| domain == "example.com"));
    }

    #[tokio::test]
    async fn meta_query_brain_returns_focused_permission_context() {
        let base = temp_dir("meta_query_brain");
        let db_path = base.join("memory.sqlite");
        let mut seed_brain = MetaBrain::new();
        seed_brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(seed_brain));
        {
            let mut brain = meta_brain.write().await;
            brain
                .grant_path_permission("/workspace/src", PermissionFlags::FILE_READ)
                .expect("grant src");
            brain
                .grant_path_permission("/workspace/docs", PermissionFlags::FILE_READ)
                .expect("grant docs");
        }
        let db = prepare_test_db(&db_path);
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), None);
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "stub".into(),
                max_subtasks: 4,
                temperature: 0.0,
                max_tokens: 256,
            },
            Arc::new(StubProvider::new(vec![
                (
                    r#"{"action":"meta:query_brain","target":"metabrain","payload":"src"}"#.into(),
                    11,
                ),
                (
                    r#"{"action":"done","target":"user","payload":"done"}"#.into(),
                    7,
                ),
            ])),
        );
        let orchestrator = Orchestrator::new(
            planner,
            worker,
            Arc::clone(&meta_brain),
            Arc::clone(&db),
            test_injector_config(),
        );

        let (results, _) = orchestrator
            .run_react("Inspect the src area", "bootstrap", SYSTEM_ROOT_ID, None)
            .await
            .expect("run react");

        let meta_query = &results[0];
        assert_eq!(meta_query.action, "meta:query_brain");
        match &meta_query.status {
            TaskStatus::Success { stdout } => {
                assert!(stdout.contains("(allow file-read* (subpath \"/workspace/src\"))"));
                assert!(!stdout.contains("/workspace/docs"));
            }
            other => panic!("expected successful meta query, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn ask_choice_yields_control_with_formatted_options() {
        let base = temp_dir("ask_choice");
        let db_path = base.join("memory.sqlite");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), None);
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "stub".into(),
                max_subtasks: 3,
                temperature: 0.0,
                max_tokens: 256,
            },
            Arc::new(StubProvider::new(vec![(
                r#"{"action":"user:ask_choice","target":"user","payload":{"context":"I found two ways to do this.","options":["Option A: use curl","Option B: write a python script"]}}"#
                    .into(),
                9,
            )])),
        );
        let orchestrator = Orchestrator::new(
            planner,
            worker,
            Arc::clone(&meta_brain),
            Arc::clone(&db),
            test_injector_config(),
        );

        let (results, _) = orchestrator
            .run_react("Choose a strategy", "bootstrap", SYSTEM_ROOT_ID, None)
            .await
            .expect("run react");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].action, "user:ask_choice");
        match &results[0].status {
            TaskStatus::Success { stdout } => {
                assert!(stdout.contains("I found two ways to do this."));
                assert!(stdout.contains("1. Option A: use curl"));
                assert!(stdout.contains("2. Option B: write a python script"));
            }
            other => panic!("expected successful ask_choice, got {other:?}"),
        }
    }

    #[test]
    fn ask_choice_telemetry_is_clean_json() {
        let parsed = crate::agent::interaction_loop::agent::Agent::parse_ask_choice_payload(Some(
            &serde_json::json!({
                "context": "I found two ways to do this.",
                "options": ["Option A: use curl", "Option B: write a python script"]
            }),
        ))
        .expect("parse ask choice payload");

        let telemetry = crate::agent::interaction_loop::agent::Agent::ask_choice_telemetry(
            &parsed,
            "test-request-id",
        );
        let v: serde_json::Value = serde_json::from_str(&telemetry).expect("valid json");
        assert_eq!(v["type"], "ask_choice");
        assert_eq!(v["request_id"], "test-request-id");
        assert_eq!(v["context"], "I found two ways to do this.");
        assert_eq!(v["options"][0], "Option A: use curl");
        assert_eq!(v["options"][1], "Option B: write a python script");
    }

    #[tokio::test]
    async fn disallowed_swarm_tool_is_blocked_by_agent_policy() {
        let base = temp_dir("agent_tool_policy");
        let db_path = base.join("memory.sqlite");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), None);
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "stub".into(),
                max_subtasks: 2,
                temperature: 0.0,
                max_tokens: 128,
            },
            Arc::new(StubProvider::new(vec![
                (
                    r#"{"action":"fs:read","target":"/tmp/demo.txt","payload":null}"#.into(),
                    8,
                ),
                (
                    r#"{"action":"done","target":"user","payload":"stopped"}"#.into(),
                    6,
                ),
            ])),
        );
        let orchestrator = Orchestrator::new(
            planner,
            worker,
            Arc::clone(&meta_brain),
            Arc::clone(&db),
            test_injector_config(),
        )
        .with_agent_spec(crate::agent::llm_types::AgentSpec {
            name: "connector".into(),
            role: "channels".into(),
            api_provider: "deepseek".into(),
            model_id: "meta/llama-3.1-70b-instruct".into(),
            allowed_tools: vec![
                "memory:search".into(),
                "memory:learn".into(),
                "done".into(),
                "user:ask_choice".into(),
            ],
            raw_markdown: "## connector".into(),
            authoritative: false,
        });

        let (results, _) = orchestrator
            .run_react("inspect the file", "bootstrap", SYSTEM_ROOT_ID, None)
            .await
            .expect("run react");

        assert!(!results.is_empty());
        match &results[0].status {
            TaskStatus::Failed { stderr, .. } => {
                assert!(stderr.contains("not allowed for swarm agent 'connector'"));
            }
            other => panic!("expected agent policy failure, got {other:?}"),
        }
    }

    #[test]
    fn parse_intent_falls_back_safely_when_unrecoverable() {
        let intent = PlannerAgent::parse_intent("totally malformed planner output");
        assert_eq!(intent.action, "system:parse_error");
        assert_eq!(intent.target, "parser");
        assert!(matches!(intent.payload, Some(Value::String(_))));
    }

    #[test]
    fn normalize_result_string_strips_dynamic_runtime_noise() {
        let raw =
            "2026-04-14T10:44:55Z pid=4821 retry 2/3 after 15ms UUID 123e4567-e89b-12d3-a456-426614174000";
        let normalized = super::normalize_result_string(raw);
        assert!(!normalized.contains("2026-04-14"));
        assert!(!normalized.contains("4821"));
        assert!(!normalized.contains("123e4567-e89b-12d3-a456-426614174000"));
        assert!(normalized.contains("<TIMESTAMP>"));
        assert!(normalized.contains("<PID>"));
        assert!(normalized.contains("<COUNTER>"));
        assert!(normalized.contains("<ELAPSED>"));
        assert!(normalized.contains("<UUID>"));
    }

    #[test]
    fn loop_detector_hash_ignores_timestamp_only_stderr_differences() {
        let intent = ActionIntent {
            reason: None,
            thought: None,
            action: "process:spawn".into(),
            target: "cargo".into(),
            payload: Some(json!(["test"])),
        };
        let first = TaskStatus::Failed {
            exit_code: Some(1),
            stderr: "2026-04-14T10:44:55Z pid=4821 retry 1/3 after 15ms".into(),
        };
        let second = TaskStatus::Failed {
            exit_code: Some(1),
            stderr: "2026-04-14T10:45:01Z pid=5199 retry 2/3 after 27ms".into(),
        };

        let first_hash = super::LoopDetector::hash_execution(&intent, &first);
        let second_hash = super::LoopDetector::hash_execution(&intent, &second);

        assert_eq!(first_hash.call_hash, second_hash.call_hash);
        assert_eq!(first_hash.result_hash, second_hash.result_hash);
    }

    #[tokio::test]
    async fn memory_learn_persists_skill_to_hdc_and_sqlite() {
        let base = temp_dir("memory_learn");
        let db_path = base.join("memory.sqlite");
        let mut brain = MetaBrain::new();
        brain.attach_persistence(db_path.with_extension("bin"));
        let meta_brain = Arc::new(AsyncRwLock::new(brain));
        let db = prepare_test_db(&db_path);
        let worker = test_worker(Arc::clone(&meta_brain), Arc::clone(&db), None);
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "stub".into(),
                max_subtasks: 4,
                temperature: 0.0,
                max_tokens: 256,
            },
            Arc::new(StubProvider::new(vec![
                (
                    r#"{"action":"memory:learn","target":"procedural","payload":{"keywords":"github api curl token","skill_description":"To authenticate with GitHub API, send header Authorization: Bearer <token>."}}"#
                        .into(),
                    11,
                ),
                (
                    r#"{"action":"done","target":"user","payload":"done"}"#.into(),
                    7,
                ),
            ])),
        );
        let orchestrator = Orchestrator::new(
            planner,
            worker,
            Arc::clone(&meta_brain),
            Arc::clone(&db),
            test_injector_config(),
        );

        let (results, _) = orchestrator
            .run_react(
                "Figure out GitHub auth syntax",
                "bootstrap",
                SYSTEM_ROOT_ID,
                None,
            )
            .await
            .expect("run react");

        assert_eq!(results[0].action, "memory:learn");
        match &results[0].status {
            TaskStatus::Success { stdout } => {
                assert!(stdout.contains("[LEARNED SKILL]"));
                assert!(stdout.contains("github api curl token"));
            }
            other => panic!("expected successful memory learn, got {other:?}"),
        }
        let conn = db.lock().await;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM knowledge_memory WHERE kind = ?1 AND text LIKE '%Authorization: Bearer <token>%'",
                [KnowledgeKind::Fact.as_str()],
                |row| row.get(0),
            )
            .expect("count learned skills");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn cancel_flag_stops_react_loop_immediately() {
        let db_path =
            std::env::temp_dir().join(format!("moat_cancel_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db_path);
        let db = prepare_test_db(&db_path);
        let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));
        let provider = Arc::new(StubProvider::new(vec![(
            r#"{"action":"done","target":"user","payload":"ok","_thought":""}"#.into(),
            4,
        )]));
        let planner = PlannerAgent::new(
            PlannerConfig {
                model: "cancel-test".into(),
                max_subtasks: 1,
                temperature: 0.0,
                max_tokens: 64,
            },
            provider.clone(),
        );
        let worker = WorkerAgent::new(
            WorkerConfig {
                model: "cancel-test".into(),
                command_timeout: Duration::from_secs(1),
                allow_high_risk: true,
                external_apis: ExternalApiConfig::default(),
            },
            Arc::clone(&meta_brain),
            Arc::clone(&db),
        )
        .expect("worker");
        let orchestrator =
            Orchestrator::new(planner, worker, meta_brain, db, test_injector_config());

        orchestrator.cancel_flag.store(true, Ordering::SeqCst);

        let (results, _) = orchestrator
            .run_react("test", "bootstrap", SYSTEM_ROOT_ID, None)
            .await
            .expect("cancelled react should return Ok");

        assert!(
            results.is_empty(),
            "cancelled run should return empty results, got {:?}",
            results
        );
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_is_blocked_process_spawn_binary_validation() {
        // 1. Basic blocklist matching
        assert!(super::is_blocked_process_spawn_binary("bash"));
        assert!(super::is_blocked_process_spawn_binary("python"));
        assert!(super::is_blocked_process_spawn_binary("sh"));
        assert!(super::is_blocked_process_spawn_binary("python3"));
        assert!(super::is_blocked_process_spawn_binary("node"));
        assert!(!super::is_blocked_process_spawn_binary("ls"));
        assert!(!super::is_blocked_process_spawn_binary("git"));

        // 2. Whitespace and empty inputs
        assert!(super::is_blocked_process_spawn_binary("   "));
        assert!(super::is_blocked_process_spawn_binary(""));

        // 3. Path components and normalization
        assert!(super::is_blocked_process_spawn_binary("/bin/bash"));
        assert!(super::is_blocked_process_spawn_binary("./bash"));
        assert!(super::is_blocked_process_spawn_binary(
            "path/to/../../bin/bash"
        ));
        assert!(super::is_blocked_process_spawn_binary(
            "/usr/bin/env/../python3"
        ));
        assert!(!super::is_blocked_process_spawn_binary("/bin/ls"));

        // 4. Directory climbing and symlinks
        let temp = temp_dir("spawn_validation_test");
        let blocked_target = temp.join("my_custom_sh");

        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&blocked_target);
            if std::os::unix::fs::symlink("/bin/sh", &blocked_target).is_ok() {
                assert!(super::is_blocked_process_spawn_binary(
                    blocked_target.to_str().unwrap()
                ));
            }
        }

        // A non-existent file path should still check the basename fallback
        assert!(super::is_blocked_process_spawn_binary(
            "/nonexistent/path/to/python"
        ));
        assert!(!super::is_blocked_process_spawn_binary(
            "/nonexistent/path/to/ls"
        ));
    }

    #[tokio::test]
    async fn test_spawn_consent_rate_limiter() {
        let bridge_slot = Arc::new(Mutex::new(None::<std::sync::Weak<ConsentBridge>>));
        let bridge_slot_for_emit = Arc::clone(&bridge_slot);
        let bridge = Arc::new(ConsentBridge::new(Box::new(move |req: ConsentRequest| {
            let weak = bridge_slot_for_emit.lock().unwrap().clone().unwrap();
            std::thread::spawn(move || {
                if let Some(br) = weak.upgrade() {
                    let _ = br.resolve(&req.request_id, ConsentDecision::ApproveOnce);
                }
            });
        })));
        *bridge_slot.lock().unwrap() = Some(Arc::downgrade(&bridge));

        for _ in 0..5 {
            let res = bridge
                .request_sub_spawn_policy(
                    "session-rate-limit",
                    "assistant",
                    "coder",
                    "process:spawn",
                )
                .await;
            assert!(res.is_ok());
        }
        let res6 = bridge
            .request_sub_spawn_policy("session-rate-limit", "assistant", "coder", "process:spawn")
            .await;
        assert!(res6.is_ok());
        assert_eq!(res6.unwrap(), ConsentDecision::Deny);
    }
}
