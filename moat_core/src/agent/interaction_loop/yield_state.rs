use crate::agent::execution::DelErrorCode;
use crate::agent::interaction_orchestrator::SubTaskResult;
use crate::MoatResult;
use serde::{Deserialize, Serialize};

/// Captures the complete agent state for later resumption.
/// Written to the Chain Journal so the agent can be re-hydrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldSnapshot {
    pub session_id: String,
    pub yield_reason: DelErrorCode,
    pub context_trace: Vec<crate::agent::llm_types::LlmMessage>,
    pub subtask_results: Vec<SubTaskResult>,
    pub turns: usize,
    pub total_tokens: u32,
    pub loop_window: Vec<crate::agent::interaction_loop::loop_detector::ToolExecution>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub agent_label: String,
    pub objective: String,
}

impl YieldSnapshot {
    /// Create a snapshot from current agent state
    pub fn capture(
        session_id: &str,
        yield_reason: DelErrorCode,
        context_trace: &[crate::agent::llm_types::LlmMessage],
        results: &[SubTaskResult],
        turns: usize,
        total_tokens: u32,
        loop_window: Vec<crate::agent::interaction_loop::loop_detector::ToolExecution>,
        agent_label: &str,
        objective: &str,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            yield_reason,
            context_trace: context_trace.to_vec(),
            subtask_results: results.to_vec(),
            turns,
            total_tokens,
            loop_window,
            created_at: chrono::Utc::now(),
            agent_label: agent_label.to_string(),
            objective: objective.to_string(),
        }
    }

    /// Serialize to JSON for journal storage
    pub fn to_json(&self) -> MoatResult<String> {
        serde_json::to_string(self).map_err(|e| {
            crate::MoatError::Orchestrator(format!("failed to serialize yield snapshot: {}", e))
        })
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> MoatResult<Self> {
        serde_json::from_str(json).map_err(|e| {
            crate::MoatError::Orchestrator(format!("failed to deserialize yield snapshot: {}", e))
        })
    }
}
