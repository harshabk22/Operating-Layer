use crate::MoatResult;

/// A single message in an LLM conversation.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

impl LlmRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// Response metadata returned alongside the generated text.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub provider: String,
    pub latency_ms: u64,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

/// Confidence tier for response gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceTier {
    High,
    Medium,
    Low,
}

impl ConfidenceTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// Compute knowledge confidence from retrieval relevance scores.
pub fn knowledge_confidence(scores: &[f64]) -> (f64, ConfidenceTier) {
    if scores.is_empty() {
        return (0.25, ConfidenceTier::Medium); // No knowledge doesn't mean low confidence
    }
    let top_score = scores.iter().cloned().fold(0.0_f64, f64::max);
    let avg = scores.iter().sum::<f64>() / scores.len() as f64;
    // Weighted: 70% top score, 30% average
    let confidence = top_score * 0.7 + avg * 0.3;
    let tier = if confidence >= 0.7 {
        ConfidenceTier::High
    } else if confidence >= 0.4 {
        ConfidenceTier::Medium
    } else {
        ConfidenceTier::Low
    };
    (confidence, tier)
}

/// Compute response confidence from knowledge confidence + response quality signals.
/// Applies intent-aware floors: greetings and general conversation never drop below Medium.
pub fn response_confidence(
    knowledge_conf: f64,
    response_length: usize,
    has_knowledge_context: bool,
    intent: &str,
) -> (f64, ConfidenceTier) {
    let mut score = knowledge_conf;

    // Boost if knowledge context was available
    if has_knowledge_context {
        score += 0.1;
    }

    // Penalize very short responses only for non-trivial intents
    let is_trivial = matches!(intent, "greeting" | "general");
    if response_length < 20 && !is_trivial {
        score -= 0.2;
    }

    let score = score.clamp(0.0, 1.0);
    let tier = if score >= 0.65 {
        ConfidenceTier::High
    } else if score >= 0.35 {
        ConfidenceTier::Medium
    } else {
        ConfidenceTier::Low
    };

    // Confidence floors: greetings and general conversations never below Medium
    let tier = if matches!(intent, "greeting" | "general") && tier == ConfidenceTier::Low {
        ConfidenceTier::Medium
    } else {
        tier
    };

    (score, tier)
}

/// Observability record for every LLM generation.
#[derive(Debug, Clone)]
pub struct LlmGenerationRecord {
    pub id: String,
    pub thread_id: String,
    pub intent: String,
    pub knowledge_doc_ids: Vec<String>,
    pub knowledge_scores: Vec<f64>,
    pub knowledge_confidence: f64,
    pub response_confidence: f64,
    pub confidence_tier: String,
    pub provider: String,
    pub latency_ms: u64,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub response_length: usize,
    pub timestamp: i64,
}

/// Trait for LLM providers that generate text responses.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a response given a system prompt and conversation messages.
    /// Returns structured LlmResponse with metadata.
    async fn generate(
        &self,
        system_prompt: &str,
        messages: &[LlmMessage],
    ) -> MoatResult<LlmResponse>;
}


// ── DeepSeek Provider ────────────────────────────────────────

pub struct DeepSeekProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl DeepSeekProvider {
    pub fn new(api_key: String) -> MoatResult<Self> {
        if api_key.trim().is_empty() {
            return Err(crate::MoatError::Llm(
                "DeepSeek API key is empty".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| crate::MoatError::Llm(format!("Failed to build HTTP client: {e}")))?;
        Ok(Self {
            api_key,
            model: "deepseek-chat".to_string(),
            client,
        })
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[async_trait::async_trait]
impl LlmProvider for DeepSeekProvider {
    async fn generate(
        &self,
        system_prompt: &str,
        messages: &[LlmMessage],
    ) -> MoatResult<LlmResponse> {
        let start = std::time::Instant::now();

        let mut api_messages = Vec::with_capacity(messages.len() + 1);

        // System prompt
        api_messages.push(serde_json::json!({
            "role": "system",
            "content": system_prompt
        }));

        // Conversation messages
        for msg in messages {
            api_messages.push(serde_json::json!({
                "role": msg.role.as_str(),
                "content": msg.content
            }));
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": api_messages,
            "temperature": 0.7,
            "max_tokens": 1024,
            "stream": false
        });

        let response = self
            .client
            .post("https://api.deepseek.com/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::MoatError::Llm(format!("DeepSeek request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(crate::MoatError::Llm(format!(
                "DeepSeek API error ({}): {}",
                status, error_body
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| crate::MoatError::Llm(format!("DeepSeek response parse error: {e}")))?;

        let latency_ms = start.elapsed().as_millis() as u64;

        let content = json["choices"]
            .get(0)
            .and_then(|c| c["message"]["content"].as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            return Err(crate::MoatError::Llm(
                "DeepSeek returned empty response".to_string(),
            ));
        }

        let prompt_tokens = json["usage"]["prompt_tokens"].as_u64().map(|v| v as u32);
        let completion_tokens = json["usage"]["completion_tokens"]
            .as_u64()
            .map(|v| v as u32);

        Ok(LlmResponse {
            content,
            provider: format!("deepseek/{}", self.model),
            latency_ms,
            prompt_tokens,
            completion_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_returns_default() {
        let provider = MockLlmProvider::new();
        let result = provider.generate("system", &[]).await.unwrap();
        assert!(!result.content.is_empty());
        assert_eq!(result.provider, "mock");
    }

    #[tokio::test]
    async fn test_mock_provider_custom_response() {
        let provider = MockLlmProvider::with_response("Custom reply");
        let result = provider.generate("system", &[]).await.unwrap();
        assert_eq!(result.content, "Custom reply");
    }

    #[tokio::test]
    async fn test_mock_provider_with_messages() {
        let provider = MockLlmProvider::new();
        let messages = vec![
            LlmMessage {
                role: LlmRole::User,
                content: "Hello".to_string(),
            },
            LlmMessage {
                role: LlmRole::Assistant,
                content: "Hi there!".to_string(),
            },
        ];
        let result = provider
            .generate("You are helpful", &messages)
            .await
            .unwrap();
        assert!(!result.content.is_empty());
    }

    #[test]
    fn test_llm_role_as_str() {
        assert_eq!(LlmRole::System.as_str(), "system");
        assert_eq!(LlmRole::User.as_str(), "user");
        assert_eq!(LlmRole::Assistant.as_str(), "assistant");
    }

    #[test]
    fn test_knowledge_confidence_empty() {
        let (score, tier) = knowledge_confidence(&[]);
        assert!(score >= 0.0);
        assert_eq!(tier, ConfidenceTier::Medium); // Empty scores floor at Medium
    }

    #[test]
    fn test_knowledge_confidence_high() {
        let (score, tier) = knowledge_confidence(&[0.9, 0.8, 0.7]);
        assert!(score >= 0.7);
        assert_eq!(tier, ConfidenceTier::High);
    }

    #[test]
    fn test_knowledge_confidence_medium() {
        let (_, tier) = knowledge_confidence(&[0.5, 0.4]);
        assert_eq!(tier, ConfidenceTier::Medium);
    }

    #[test]
    fn test_knowledge_confidence_low() {
        let (_, tier) = knowledge_confidence(&[0.1, 0.05]);
        assert_eq!(tier, ConfidenceTier::Low);
    }

    #[test]
    fn test_response_confidence_high() {
        let (score, tier) = response_confidence(0.8, 100, true, "general");
        assert!(score >= 0.65);
        assert_eq!(tier, ConfidenceTier::High);
    }

    #[test]
    fn test_response_confidence_penalizes_short() {
        let (score_long, _) = response_confidence(0.5, 100, false, "booking");
        let (score_short, _) = response_confidence(0.5, 10, false, "booking");
        assert!(score_long > score_short); // Short responses penalized for non-trivial intents
    }

    #[test]
    fn test_response_confidence_clamps() {
        let (score, _) = response_confidence(0.0, 5, false, "general");
        assert!(score >= 0.0);
        let (score2, _) = response_confidence(1.0, 200, true, "booking");
        assert!(score2 <= 1.0);
    }

    #[test]
    fn test_deepseek_rejects_empty_key() {
        let result = DeepSeekProvider::new("".to_string());
        assert!(result.is_err());
        let result2 = DeepSeekProvider::new("   ".to_string());
        assert!(result2.is_err());
    }
}
