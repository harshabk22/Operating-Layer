use crate::agent::execution::journal_chain::{self, JournalEntry, JournalOperation};
use crate::agent::llm_types::LlmMessage;
use crate::MoatResult;
use std::fs;

/// Reconstructs an agent's context window from the chain journal.
///
/// Maps historical ActionIntent + ExecutionResult pairs into
/// standard observation messages the LLM can understand,
/// verified by the journal's cryptographic hash chain.
pub struct ContextRehydrator;

impl ContextRehydrator {
    /// Read the journal from disk, verify the hash chain, and rebuild
    /// the context trace (Vec\<LlmMessage\>) that the agent would have
    /// accumulated during the original session.
    pub fn rehydrate(journal_path: &str, session_id: &str) -> MoatResult<Vec<LlmMessage>> {
        let entries = read_chain_entries_scoped(journal_path, session_id)?;
        journal_chain::verify_chain_scoped(journal_path, session_id).map_err(|e| {
            crate::MoatError::Orchestrator(format!("Chain journal verification failed: {}", e))
        })?;

        if entries.is_empty() {
            return Ok(Vec::new());
        }

        let mut trace = Vec::new();
        for (turn, entry) in entries.iter().enumerate() {
            let observation = match &entry.operation {
                JournalOperation::Allow { tool_name, args } => {
                    let target = extract_target(tool_name, args);
                    format!(
                        "Turn {} ACTION: {} | TARGET: {} | RESULT: Allowed\nARGS: {}",
                        turn, tool_name, target, args
                    )
                }
                JournalOperation::Deny { tool_name, reason } => {
                    format!(
                        "Turn {} ACTION: {} | RESULT: Denied — {}\nDETAIL: {}",
                        turn, tool_name, reason, reason
                    )
                }
            };
            trace.push(LlmMessage {
                role: "system".into(),
                content: Some(observation),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        Ok(trace)
    }
}

fn read_chain_entries_scoped(path: &str, _session_id: &str) -> MoatResult<Vec<JournalEntry>> {
    let raw = fs::read(path).map_err(|e| {
        crate::MoatError::Orchestrator(format!("Failed to read chain journal at {}: {}", path, e))
    })?;

    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let data_len = if raw.len() >= 32 {
        raw.len() - 32
    } else {
        raw.len()
    };
    let data = &raw[..data_len];

    let mut entries = Vec::new();
    let mut cursor = 0usize;
    while cursor < data.len() {
        match bincode::deserialize::<JournalEntry>(&data[cursor..]) {
            Ok(entry) => {
                let entry_bytes = bincode::serialize(&entry).unwrap_or_default();
                if entry_bytes.is_empty() {
                    break;
                }
                cursor += entry_bytes.len();
                entries.push(entry);
            }
            Err(_) => break,
        }
    }
    Ok(entries)
}

fn extract_target(tool_name: &str, args: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
        for key in &["path", "url", "app_name", "command", "target"] {
            if let Some(v) = val.get(key).and_then(|v| v.as_str()) {
                return v.to_string();
            }
        }
        if let Some(v) = val.as_str() {
            return v.to_string();
        }
    }
    tool_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::execution::journal_chain::ChainJournal;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let uuid = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("recovery_test_{}_{}.bin", name, uuid))
    }

    #[test]
    fn test_rehydrate_empty_journal() {
        let path = temp_path("empty_recovery");
        let _journal = ChainJournal::open_scoped(&path, "test-session").expect("open");
        drop(_journal);

        let trace = ContextRehydrator::rehydrate(path.to_str().unwrap(), "test-session")
            .expect("rehydrate");
        assert!(trace.is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_rehydrate_with_allow_entries() {
        let path = temp_path("allow_entries");
        let mut journal = ChainJournal::open_scoped(&path, "test-session").expect("open");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_read".into(),
                    args: r#"{"path":"/tmp/readme.txt"}"#.into(),
                },
            )
            .expect("append 1");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "process_spawn".into(),
                    args: r#"{"command":"cargo build"}"#.into(),
                },
            )
            .expect("append 2");
        drop(journal);

        let trace = ContextRehydrator::rehydrate(path.to_str().unwrap(), "test-session")
            .expect("rehydrate");
        assert_eq!(trace.len(), 2);
        assert!(trace[0].content.as_ref().unwrap().contains("fs_read"));
        assert!(trace[0]
            .content
            .as_ref()
            .unwrap()
            .contains("/tmp/readme.txt"));
        assert!(trace[1].content.as_ref().unwrap().contains("cargo build"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_rehydrate_with_deny_entries() {
        let path = temp_path("deny_entries");
        let mut journal = ChainJournal::open_scoped(&path, "test-session").expect("open");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Deny {
                    tool_name: "fs_delete".into(),
                    reason: "consent denied".into(),
                },
            )
            .expect("append");
        drop(journal);

        let trace = ContextRehydrator::rehydrate(path.to_str().unwrap(), "test-session")
            .expect("rehydrate");
        assert_eq!(trace.len(), 1);
        assert!(trace[0]
            .content
            .as_ref()
            .unwrap()
            .contains("consent denied"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_rehydrate_detects_tampered_journal() {
        let path = temp_path("tampered_recovery");
        let mut journal = ChainJournal::open_scoped(&path, "test-session").expect("open");
        journal
            .append(
                "agent1".into(),
                JournalOperation::Allow {
                    tool_name: "fs_read".into(),
                    args: r#"{"path":"/tmp/a.txt"}"#.into(),
                },
            )
            .expect("append");
        drop(journal);

        let mut raw = std::fs::read(&path).expect("read");
        if raw.len() > 10 {
            raw[10] ^= 0xFF;
        }
        std::fs::write(&path, &raw).expect("write tampered");

        let result = ContextRehydrator::rehydrate(path.to_str().unwrap(), "test-session");
        assert!(result.is_err(), "Tampered journal must fail re-hydration");
        std::fs::remove_file(&path).ok();
    }
}
