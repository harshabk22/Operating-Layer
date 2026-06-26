use crate::{MoatError, MoatResult};

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct UserProfile {
    pub name: String,
    pub core_preferences: String,
    pub chat_model: Option<String>,
    pub planner_model: Option<String>,
    pub worker_model: Option<String>,
    pub summary: String,
    pub facts: Vec<String>,
    pub body_markdown: String,
    pub raw_markdown: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AgentSpec {
    pub name: String,
    pub role: String,
    pub api_provider: String,
    pub model_id: String,
    pub allowed_tools: Vec<String>,
    pub raw_markdown: String,
    #[serde(default)]
    pub authoritative: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmTool {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: LlmFunctionDef,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmFunctionDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: LlmFunctionCall,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStructuredResponse {
    pub state: String,
    pub thought: String,
    pub actions: Vec<ActionIntentRaw>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionIntentRaw {
    pub action: String,
    pub target: Option<String>,
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<LlmToolCall>>,
    pub tokens: u32,
    pub structured: Option<AgentStructuredResponse>,
}

/// Kernel-grade LLM provider — must return complete, deterministic payloads.
/// The orchestration state machine requires full chat_complete responses.
/// Streaming (`complete_stream`) is NOT used by the kernel path.
pub trait LlmProvider: Send + Sync {
    /// REQUIRED: Return a complete LlmResponse with text and/or tool_calls.
    /// The kernel treats this as a deterministic state transition.
    fn chat_complete<'a>(
        &'a self,
        _model: &'a str,
        _messages: Vec<LlmMessage>,
        _tools: Option<Vec<LlmTool>>,
        _max_tokens: usize,
        _temperature: f32,
        _response_format: Option<serde_json::Value>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<LlmResponse, MoatError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(MoatError::Llm(
                "chat_complete not implemented for this provider".into(),
            ))
        })
    }

    fn check_health<'a>(&'a self) -> BoxFuture<'a, MoatResult<()>> {
        Box::pin(async { Ok(()) })
    }
}

use std::sync::Arc;
use std::time::Duration;

fn is_transient_error(err: &MoatError) -> bool {
    let err_str = err.to_string().to_lowercase();
    err_str.contains("429") 
        || err_str.contains("rate limit") 
        || err_str.contains("timeout") 
        || err_str.contains("503") 
        || err_str.contains("502") 
        || err_str.contains("500")
}

pub struct FallbackLlmProvider {
    pub primary: Arc<dyn LlmProvider>,
    pub fallback: Arc<dyn LlmProvider>,
    pub fallback_model: String,
}

impl FallbackLlmProvider {
    pub fn new(primary: Arc<dyn LlmProvider>, fallback: Arc<dyn LlmProvider>, fallback_model: String) -> Self {
        Self { primary, fallback, fallback_model }
    }
}

impl LlmProvider for FallbackLlmProvider {
    fn chat_complete<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        max_tokens: usize,
        temperature: f32,
        response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        Box::pin(async move {
            let mut _last_err = None;
            for attempt in 0..3 {
                match self.primary.chat_complete(model, messages.clone(), tools.clone(), max_tokens, temperature, response_format.clone()).await {
                    Ok(resp) => return Ok(resp),
                    Err(err) => {
                        if !is_transient_error(&err) {
                            return Err(err);
                        }
                        _last_err = Some(err);
                        tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                    }
                }
            }
            self.fallback.chat_complete(&self.fallback_model, messages, tools, max_tokens, temperature, response_format).await
        })
    }

    fn check_health<'a>(&'a self) -> BoxFuture<'a, MoatResult<()>> {
        Box::pin(async move {
            if self.primary.check_health().await.is_ok() {
                Ok(())
            } else {
                self.fallback.check_health().await
            }
        })
    }
}

