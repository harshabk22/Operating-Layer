pub mod prompt;

use crate::agent::llm_types::LlmFunctionCall;
use crate::agent::llm_types::{
    LlmFunctionDef, LlmMessage, LlmProvider, LlmResponse, LlmTool, LlmToolCall,
};
use crate::planner::prompt::REACT_SYSTEM_TEMPLATE;
use crate::MoatError;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::integration::external_api::ExternalApiConfig;

// ─────────────────────────────────────────────────────────────────────────────
// § Configuration
// ─────────────────────────────────────────────────────────────────────────────

fn agent_response_schema() -> serde_json::Value {
    json!({
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

#[derive(Debug, Clone)]
pub struct PlannerConfig {
    pub model: String,
    pub max_subtasks: usize,
    pub temperature: f32,
    pub max_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub model: String,
    pub command_timeout: Duration,
    pub allow_high_risk: bool,
    pub external_apis: ExternalApiConfig,
}

// ─────────────────────────────────────────────────────────────────────────────
// § PlannerAgent
// ─────────────────────────────────────────────────────────────────────────────

use crate::agent::interaction_orchestrator::ActionIntent;

#[cfg(not(test))]
pub(crate) const PLANNER_REPLY_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(test)]
pub(crate) const PLANNER_REPLY_TIMEOUT: Duration = Duration::from_millis(50);

#[derive(Clone)]
pub struct PlannerAgent {
    pub config: PlannerConfig,
    pub provider: Arc<dyn LlmProvider>,
    model_override: Arc<std::sync::RwLock<String>>,
}

impl PlannerAgent {
    pub fn new(config: PlannerConfig, provider: Arc<dyn LlmProvider>) -> Self {
        let model_override = Arc::new(std::sync::RwLock::new(config.model.clone()));
        Self {
            config,
            provider,
            model_override,
        }
    }

    pub fn set_model(&self, model: impl Into<String>) {
        if let Ok(mut slot) = self.model_override.write() {
            *slot = model.into();
        }
    }

    pub fn current_model(&self) -> String {
        self.model_override
            .read()
            .map(|slot| slot.clone())
            .unwrap_or_else(|_| self.config.model.clone())
    }

    pub async fn generate_intent_raw(
        &self,
        rolling_context: &str,
        tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> crate::MoatResult<(String, u32)> {
        self.generate_intent_raw_with_system(REACT_SYSTEM_TEMPLATE, rolling_context, tx)
            .await
    }

    pub async fn generate_intent_raw_with_system(
        &self,
        system_prompt: &str,
        rolling_context: &str,
        _tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> crate::MoatResult<(String, u32)> {
        let model = self.current_model();
        let response = timeout(
            PLANNER_REPLY_TIMEOUT,
            self.provider.chat_complete(
                &model,
                vec![
                    LlmMessage {
                        role: "system".into(),
                        content: Some(system_prompt.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                    LlmMessage {
                        role: "user".into(),
                        content: Some(rolling_context.to_string()),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                ],
                None,
                self.config.max_tokens,
                self.config.temperature,
                None,
            ),
        )
        .await
        .map_err(|_| {
            MoatError::Orchestrator(format!(
                "planner provider timed out after {}s",
                PLANNER_REPLY_TIMEOUT.as_secs()
            ))
        })??;
        Ok((response.content.unwrap_or_default(), response.tokens))
    }

    pub async fn generate_intent_tools(
        &self,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
    ) -> crate::MoatResult<LlmResponse> {
        let model = self.current_model();
        let mut resp = timeout(
            PLANNER_REPLY_TIMEOUT,
            self.provider.chat_complete(
                &model,
                messages,
                tools,
                self.config.max_tokens,
                self.config.temperature,
                Some(agent_response_schema()),
            ),
        )
        .await
        .map_err(|_| {
            MoatError::Orchestrator(format!(
                "planner provider timed out after {}s",
                PLANNER_REPLY_TIMEOUT.as_secs()
            ))
        })??;

        if let Some(structured) = resp.structured.take() {
            if structured.state == "completed" {
                let summary = structured
                    .summary
                    .as_deref()
                    .unwrap_or("Task completed.")
                    .to_string();
                return Ok(LlmResponse {
                    content: Some(summary),
                    tool_calls: None,
                    tokens: resp.tokens,
                    structured: None,
                });
            }

            let tool_calls: Vec<LlmToolCall> = structured
                .actions
                .into_iter()
                .enumerate()
                .map(|(i, a)| {
                    let target = a.target.unwrap_or_else(|| "user".to_string());
                    let payload = a.payload.unwrap_or(serde_json::Value::Null);
                    LlmToolCall {
                        id: format!("call_{}", i),
                        type_: "function".into(),
                        function: LlmFunctionCall {
                            name: a.action,
                            arguments: serde_json::json!({"target": target, "payload": payload})
                                .to_string(),
                        },
                    }
                })
                .collect();

            return Ok(LlmResponse {
                content: Some(structured.thought),
                tool_calls: Some(tool_calls),
                tokens: resp.tokens,
                structured: None,
            });
        }

        Ok(resp)
    }

    pub fn build_llm_tools(&self) -> Vec<LlmTool> {
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
                LlmTool {
                    type_: "function".into(),
                    function: LlmFunctionDef {
                        name: name.into(),
                        description: Some(desc.into()),
                        parameters: params,
                    },
                }
            })
            .collect()
    }

    pub fn parse_intent(raw: &str) -> ActionIntent {
        serde_json::from_str(raw).unwrap_or_else(|_| Self::safe_retry_intent())
    }

    fn safe_retry_intent() -> ActionIntent {
        ActionIntent {
            reason: None,
            thought: None,
            action: "system:parse_error".to_string(),
            target: "parser".to_string(),
            payload: Some(Value::String(
                "[SYSTEM: Your last response could not be parsed. You MUST respond with a valid <action> JSON block. Try again.]".to_string(),
            )),
        }
    }
}
