use super::{Message, Role, SummaryTemplate};
use anyhow::Result;

pub struct ContextCompressor;

impl ContextCompressor {
    pub fn precompress_tool_outputs(messages: &mut Vec<Message>) {
        for msg in messages.iter_mut() {
            if msg.role == Role::Tool {
                let original_len = msg.content.len();

                if original_len > 500 {
                    let lines = msg.content.lines().count();
                    msg.content = format!(
                        "[Tool output compressed: {} lines, {} bytes]",
                        lines, original_len
                    );
                }
            }
        }
    }

    pub fn compress_tiered(messages: Vec<Message>) -> Result<Vec<Message>> {
        let total = messages.len();

        if total <= 10 {
            return Ok(messages);
        }

        let head_size = 2;
        let tail_size = (total as f64 * 0.20) as usize;

        let compressible_start = head_size;
        let mut compressible_end = total.saturating_sub(tail_size);

        // Adjust compressible_end backward to avoid starting the tail with a Tool message
        // or splitting an assistant-tool sequence.
        while compressible_end > compressible_start {
            let msg = &messages[compressible_end];
            if msg.role == Role::Tool {
                compressible_end -= 1;
            } else {
                break;
            }
        }

        if compressible_end <= compressible_start {
            return Ok(messages);
        }

        let mut compressed = Vec::new();

        compressed.extend_from_slice(&messages[0..head_size]);

        // Only precompress tool outputs for the middle messages that are going to be summarized
        let mut middle_messages = messages[compressible_start..compressible_end].to_vec();
        Self::precompress_tool_outputs(&mut middle_messages);

        let summary = Self::build_summary_from_messages(&middle_messages);
        compressed.push(Message::system(summary.to_message_content()));

        compressed.extend_from_slice(&messages[compressible_end..]);

        Ok(compressed)
    }

    fn build_summary_from_messages(messages: &[Message]) -> SummaryTemplate {
        let mut completed = Vec::new();
        let mut pending = Vec::new();

        for msg in messages.iter() {
            if msg.role == Role::Tool {
                completed.push(format!(
                    "Executed tool operation ({}... chars)",
                    msg.content.len().min(50)
                ));
            } else if msg.role == Role::User {
                pending.push(format!(
                    "User query: {}...",
                    msg.content.chars().take(50).collect::<String>()
                ));
            }
        }

        SummaryTemplate {
            active_task: "Continuing from previous context".into(),
            completed_actions: completed,
            current_state: format!("{} messages compressed", messages.len()),
            pending_items: pending,
            blockers: vec![],
        }
    }

    pub fn compression_saved_enough(
        original_token_count: usize,
        compressed_token_count: usize,
        min_savings_ratio: f64,
    ) -> bool {
        let saved = original_token_count.saturating_sub(compressed_token_count);
        let savings_ratio = saved as f64 / original_token_count as f64;
        savings_ratio >= min_savings_ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precompress_large_tool_output() {
        let mut messages = vec![Message::tool("x".repeat(1000))];

        ContextCompressor::precompress_tool_outputs(&mut messages);

        assert!(messages[0].content.contains("compressed"));
        assert!(messages[0].content.len() < 200);
    }

    #[test]
    fn test_tiered_compression_protects_head_tail() {
        let messages: Vec<Message> = (0..20)
            .map(|i| {
                if i == 0 {
                    Message::system("System prompt")
                } else if i % 2 == 0 {
                    Message::user(format!("Query {}", i))
                } else {
                    Message::assistant(format!("Response {}", i))
                }
            })
            .collect();

        let compressed = ContextCompressor::compress_tiered(messages.clone()).unwrap();

        assert_eq!(compressed[0].role, Role::System);
        assert_eq!(
            compressed[0].content,
            "<|SYSTEM_START|>\nSystem prompt\n<|SYSTEM_END|>"
        );

        assert!(compressed
            .iter()
            .any(|m| m.content.contains("CONTEXT SUMMARY")));

        let last_original = messages.last().unwrap();
        let last_compressed = compressed.last().unwrap();
        assert_eq!(last_compressed.content, last_original.content);
    }

    #[test]
    fn test_tiered_compression_avoids_splitting_assistant_tool_pair() {
        use crate::agent::llm::{FunctionCall, ToolCall};

        // Create a sequence of 15 messages where the tail size would normally split a tool execution turn.
        // indices 0..1: System, User (head)
        // indices 2..10: some messages
        // index 11: Assistant with tool calls
        // index 12: Tool result
        // index 13: Tool result
        // index 14: Assistant final response
        let mut messages = vec![Message::system("System"), Message::user("First query")];

        for i in 0..9 {
            messages.push(Message::user(format!("Middle Query {}", i)));
        }

        let tool_calls = vec![ToolCall {
            id: "call_1".into(),
            call_type: "function".into(),
            function: FunctionCall {
                name: "fs_read".into(),
                arguments: r#"{"path":"/tmp/a.txt"}"#.into(),
            },
        }];

        messages.push(Message::assistant_with_tool_calls(
            "Calling tool",
            tool_calls,
        ));
        messages.push(Message::tool_result("call_1".into(), "tool output"));
        messages.push(Message::assistant("Done"));

        // total messages = 14
        // head_size = 2, tail_size = 14 * 0.20 = 2
        // normal compressible_end = 12 (which is Tool result).
        // Since messages[12] is Tool role, boundary adjustment must shift it back to index 11 (Assistant with tool calls).
        // So compressible_end becomes 11, meaning messages from index 11 to 13 (the entire tool sequence) are kept in the tail.
        let compressed = ContextCompressor::compress_tiered(messages.clone()).unwrap();

        // Let's verify that the tail portion (after summary system message) starts with the Assistant message
        // which has the tool calls, followed by the Tool result, then the final Assistant.
        // The structure of compressed should be:
        // [0..2]: System, User
        // [2]: System summary
        // [3..]: Assistant with tool calls, Tool result, Assistant final response
        assert_eq!(compressed.len(), 6);
        assert_eq!(compressed[3].role, Role::Assistant);
        assert!(compressed[3].tool_calls.is_some());
        assert_eq!(compressed[4].role, Role::Tool);
        assert_eq!(compressed[4].content, "tool output");
        assert_eq!(compressed[5].role, Role::Assistant);
        assert_eq!(compressed[5].content, "Done");
    }
}
