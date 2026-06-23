// § Orchestrator (The ReAct Loop)
// ─────────────────────────────────────────────────────────────────────────────

use crate::agent::llm_types::LlmMessage;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;

pub type SharedContext = Arc<AsyncRwLock<VersionedContext>>;

pub struct VersionedContext {
    history: Vec<LlmMessage>,
    generation: AtomicU64,
}

fn compress_bootstrap(bootstrap: &str) -> String {
    let mut compressed = String::new();

    let agent_info = if let Some(start) = bootstrap.find("<agent_spec>") {
        if let Some(end) = bootstrap[start..].find("</agent_spec>") {
            let spec_content = &bootstrap[start + 12..start + end];
            let mut role = "Assistant";
            let mut tools = "";
            for line in spec_content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("role:") {
                    role = trimmed.strip_prefix("role:").unwrap().trim();
                } else if trimmed.starts_with("allowed_tools:") {
                    tools = trimmed.strip_prefix("allowed_tools:").unwrap().trim();
                }
            }
            format!("Agent Identity: {} | Allowed Tools: {}", role, tools)
        } else {
            "Agent Identity: Unknown".to_string()
        }
    } else {
        "Agent Identity: Unknown".to_string()
    };
    compressed.push_str(&agent_info);
    compressed.push_str("\n\n");

    if let Some(start) = bootstrap.find("[USER QUERY]\n") {
        let after = &bootstrap[start + 13..];
        let query_end = after.find('\n').unwrap_or(after.len());
        compressed.push_str("[USER QUERY]\n");
        compressed.push_str(&after[..query_end]);
        compressed.push_str("\n\n");
    } else if let Some(start) = bootstrap.find("[USER QUERY]") {
        let after = &bootstrap[start + 12..];
        let query_end = after.find('\n').unwrap_or(after.len());
        compressed.push_str("[USER QUERY]\n");
        compressed.push_str(after[..query_end].trim());
        compressed.push_str("\n\n");
    }

    if let Some(start) = bootstrap.find("<host_environment>") {
        if let Some(end) = bootstrap[start..].find("</host_environment>") {
            compressed.push_str(&bootstrap[start..start + end + 19]);
            compressed.push_str("\n\n");
        }
    }

    if let Some(start) = bootstrap.find("<system_state>") {
        if let Some(end) = bootstrap[start..].find("</system_state>") {
            compressed.push_str(&bootstrap[start..start + end + 14]);
        }
    }

    compressed
}

impl VersionedContext {
    pub fn new(seed: Vec<LlmMessage>) -> Self {
        Self {
            history: seed,
            generation: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> Vec<LlmMessage> {
        self.history.clone()
    }

    pub fn push(&mut self, entry: LlmMessage) {
        self.history.push(entry);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn try_swap(&mut self, expected_generation: u64, replacement: Vec<LlmMessage>) -> bool {
        if self
            .generation
            .compare_exchange(
                expected_generation,
                expected_generation + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
        {
            self.history = replacement;
            true
        } else {
            false
        }
    }

    pub fn prune_if_needed(&mut self, token_budget: usize) {
        if self.history.len() <= 6 {
            return;
        }

        let generation = self.generation.load(Ordering::SeqCst);

        let has_two_system_headers = self.history.len() >= 2
            && self.history[0].role == "system"
            && self.history[1].role == "system";

        if token_budget <= 64_000 {
            let (start_idx, min_start) = if has_two_system_headers {
                (self.history.len().saturating_sub(4), 2)
            } else {
                (self.history.len().saturating_sub(5), 1)
            };
            let adjusted_start = adjust_tail_start(&self.history, start_idx, min_start);

            let mut replacement = Vec::new();
            if let Some(first) = self.history.first() {
                replacement.push(first.clone());
            }
            if has_two_system_headers {
                if let Some(second) = self.history.get(1) {
                    let mut bootstrap_msg = second.clone();
                    if let Some(content) = bootstrap_msg.content.as_deref() {
                        bootstrap_msg.content = Some(compress_bootstrap(content));
                    }
                    replacement.push(bootstrap_msg);
                }
            }
            replacement.extend(self.history[adjusted_start..].iter().cloned());
            let _ = self.try_swap(generation, replacement);
            return;
        }

        let facts = self.extract_facts();
        let summary = self.summarize_turns();

        let (start_idx, min_start) = if has_two_system_headers {
            (self.history.len().saturating_sub(2), 2)
        } else {
            (self.history.len().saturating_sub(3), 1)
        };
        let adjusted_start = adjust_tail_start(&self.history, start_idx, min_start);

        let mut replacement = Vec::new();
        if let Some(first) = self.history.first() {
            replacement.push(first.clone());
        }
        if has_two_system_headers {
            if let Some(second) = self.history.get(1) {
                let mut bootstrap_msg = second.clone();
                if let Some(content) = bootstrap_msg.content.as_deref() {
                    bootstrap_msg.content = Some(compress_bootstrap(content));
                }
                replacement.push(bootstrap_msg);
            }
        }
        if !facts.is_empty() {
            replacement.push(LlmMessage {
                role: "system".into(),
                content: Some(format!("[FACTS FROM PRIOR TURNS]\n{}", facts)),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        if !summary.is_empty() {
            replacement.push(LlmMessage {
                role: "system".into(),
                content: Some(format!("[SUMMARY OF DROPPED TURNS]\n{}", summary)),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        replacement.extend(self.history[adjusted_start..].iter().cloned());

        let _ = self.try_swap(generation, replacement);
    }

    fn extract_facts(&self) -> String {
        self.history
            .iter()
            .filter(|entry| {
                entry.content.as_deref().unwrap_or("").contains("SUCCESS:")
                    || entry.content.as_deref().unwrap_or("").contains("FAILED:")
            })
            .take(6)
            .cloned()
            .map(|m| m.content.clone().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn summarize_turns(&self) -> String {
        let has_two_system_headers = self.history.len() >= 2
            && self.history[0].role == "system"
            && self.history[1].role == "system";
        let skip_count = if has_two_system_headers { 2 } else { 1 };

        self.history
            .iter()
            .skip(skip_count)
            .take(self.history.len().saturating_sub(skip_count + 3))
            .filter_map(|entry| entry.content.as_deref())
            .filter(|text| !text.is_empty())
            .map(|text| {
                let truncated: String = text.chars().take(200).collect();
                if text.len() > 200 {
                    format!("{}...", truncated)
                } else {
                    truncated
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .take(2048)
            .collect()
    }
}

pub(crate) async fn snapshot_context(context: &SharedContext) -> Vec<LlmMessage> {
    context.read().await.snapshot()
}

pub(crate) async fn push_context(context: &SharedContext, entry: LlmMessage) {
    context.write().await.push(entry);
}

pub(crate) async fn prune_context_if_needed(context: &SharedContext, token_budget: usize) {
    context.write().await.prune_if_needed(token_budget);
}

pub(crate) async fn context_len(context: &SharedContext) -> usize {
    context.read().await.len()
}

fn adjust_tail_start(history: &[LlmMessage], mut start_idx: usize, min_start: usize) -> usize {
    while start_idx > min_start {
        let msg = &history[start_idx];
        if msg.role == "tool" {
            start_idx -= 1;
        } else if msg.role == "assistant" && msg.tool_calls.is_some() {
            break;
        } else {
            break;
        }
    }
    start_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prune_if_needed_avoids_splitting_tool_pair() {
        let mut context = VersionedContext::new(vec![
            LlmMessage {
                role: "system".into(),
                content: Some("Bootstrap".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage {
                role: "user".into(),
                content: Some("query 1".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage {
                role: "assistant".into(),
                content: Some("thought 1".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage {
                role: "user".into(),
                content: Some("query 2".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            LlmMessage {
                role: "assistant".into(),
                content: Some("Calling tool".into()),
                tool_calls: Some(vec![]),
                tool_call_id: None,
            },
            LlmMessage {
                role: "tool".into(),
                content: Some("Tool result 1".into()),
                tool_calls: None,
                tool_call_id: Some("call_1".into()),
            },
            LlmMessage {
                role: "tool".into(),
                content: Some("Tool result 2".into()),
                tool_calls: None,
                tool_call_id: Some("call_2".into()),
            },
            LlmMessage {
                role: "assistant".into(),
                content: Some("Done".into()),
                tool_calls: None,
                tool_call_id: None,
            },
        ]);

        // Heavy pruning (budget = 70_000). The last 3 starts at index 5 (Tool result 1).
        // Since index 5 is Tool role, the boundary adjustment must shift start_idx to index 4 (Calling tool).
        context.prune_if_needed(70_000);

        let snapshot = context.snapshot();
        // Structure should be:
        // 0: System bootstrap
        // 1: system message [SUMMARY OF DROPPED TURNS]
        // 2: Assistant (Calling tool)
        // 3: Tool result 1
        // 4: Tool result 2
        // 5: Assistant final (Done)
        assert_eq!(snapshot.len(), 6);
        assert_eq!(snapshot[2].role, "assistant");
        assert!(snapshot[2].tool_calls.is_some());
        assert_eq!(snapshot[3].role, "tool");
        assert_eq!(snapshot[4].role, "tool");
        assert_eq!(snapshot[5].role, "assistant");
        assert_eq!(snapshot[5].content.as_deref(), Some("Done"));
    }
}
