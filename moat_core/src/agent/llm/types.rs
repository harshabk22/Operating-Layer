use serde::{Deserialize, Serialize};

/// Provider selection
#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    OpenAi,
    Google,
    DeepSeek,
}

impl LlmProvider {
    /// Load from environment variable AGEEX_PROVIDER
    /// Defaults to OpenAI if not set
    pub fn from_env() -> Self {
        match std::env::var("AGEEX_PROVIDER")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "google" => LlmProvider::Google,
            "deepseek" => LlmProvider::DeepSeek,
            _ => LlmProvider::OpenAi,
        }
    }

    pub fn base_url(&self) -> &'static str {
        match self {
            LlmProvider::OpenAi => "https://api.openai.com/v1",
            LlmProvider::Google => "https://generativelanguage.googleapis.com/v1beta",
            LlmProvider::DeepSeek => "https://api.deepseek.com/v1",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            LlmProvider::OpenAi => "gpt-4o",
            LlmProvider::Google => "gemini-2.5-flash",
            LlmProvider::DeepSeek => "deepseek-chat",
        }
    }

    pub fn api_key_env_var(&self) -> &'static str {
        match self {
            LlmProvider::OpenAi => "OPENAI_API_KEY",
            LlmProvider::Google => "GEMINI_API_KEY",
            LlmProvider::DeepSeek => "DEEPSEEK_API_KEY",
        }
    }
}

/// OpenAI-compatible message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl LlmMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

/// Tool call emitted by LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String, // JSON string
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

/// DeepSeek thinking mode configuration
/// <https://api-docs.deepseek.com/guides/thinking_mode>
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Tool/function definition for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}
