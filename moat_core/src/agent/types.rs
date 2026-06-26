use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;
use crate::agent::llm_types::LlmMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntent {
    pub thought: Option<String>,
    pub action: String,
    pub target: String,
    pub payload: Option<Value>,
    #[serde(default)]
    pub reason: Option<String>,
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
pub trait UiActionBridge: Send + Sync {
    fn click(&self, element_id: usize) -> Result<String, String>;
    fn type_text(&self, element_id: usize, text: &str) -> Result<String, String>;
    fn refresh_ui_tree(&self) -> Result<String, String>;
    fn active_app_name(&self) -> Option<String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Success {
        stdout: String,
    },
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
    Cancelled {
        stage: String,
    },
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
    pub session_id: Uuid,
    pub approvals: std::sync::atomic::AtomicUsize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecution {
    pub call_hash: [u8; 32],
    pub result_hash: [u8; 32],
    pub is_success: bool,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub reason: Option<String>,
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
    Terminated(String),
}
