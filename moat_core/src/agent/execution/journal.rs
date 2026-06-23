use super::gate2_policy::normalize_workspace_path;
use super::request::ExecutionRequest;
use super::result::ExecutionResult;
use anyhow::Result;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ExecutionJournal {
    pub path: PathBuf,
    pub(crate) denial_counts: Arc<Mutex<HashMap<String, usize>>>,
}

impl ExecutionJournal {
    pub fn open() -> Result<Self> {
        let journal_path = Self::runtime_root()?.join("execution.journal");

        if let Some(parent) = journal_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(Self {
            path: journal_path,
            denial_counts: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn runtime_root() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("MOAT_RUNTIME_ROOT") {
            return Ok(PathBuf::from(path));
        }

        #[cfg(test)]
        {
            let root = std::env::var_os("MOAT_TEST_ROOT")
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(root.join("test_scratch").join("runtime"))
        }

        #[cfg(not(test))]
        {
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
            Ok(home.join(".ageex"))
        }
    }

    pub fn consecutive_denials(&self, agent_id: &str) -> usize {
        *self
            .denial_counts
            .lock()
            .unwrap()
            .get(agent_id)
            .unwrap_or(&0)
    }

    pub fn record_denied(&self, agent_id: &str) {
        let mut counts = self.denial_counts.lock().unwrap();
        *counts.entry(agent_id.to_string()).or_insert(0) += 1;
    }

    pub fn record_success(&self, agent_id: &str) {
        self.denial_counts.lock().unwrap().remove(agent_id);
    }

    pub fn log_request(&self, req: &ExecutionRequest) -> Result<()> {
        let mut normalized_args = req.args.clone();
        if let Some(path_val) = req.args.get("path").and_then(|v| v.as_str()) {
            normalized_args["path"] = serde_json::Value::String(normalize_workspace_path(path_val));
        }
        let entry = serde_json::json!({
            "type": "request",
            "request_id": req.request_id,
            "timestamp": req.timestamp,
            "agent_id": req.agent_id,
            "tool_name": req.tool_name,
            "capability": format!("{:?}", req.capability),
            "args": normalized_args,
        });
        self.append_line(&serde_json::to_string(&entry)?)
    }

    pub fn log_result(&self, res: &ExecutionResult) -> Result<()> {
        let entry = serde_json::json!({
            "type": "result",
            "request_id": res.request_id,
            "timestamp": Self::now(),
            "success": res.success,
            "gate1": res.gate1_passed,
            "gate2": res.gate2_passed,
            "gate3": res.gate3_passed,
            "exit_code": res.exit_code,
            "denied_reason": res.denied_reason,
            "execution_time_ms": res.execution_time_ms,
            "output_preview": &res.output[..res.output.len().min(200)],
        });
        self.append_line(&serde_json::to_string(&entry)?)
    }

    fn append_line(&self, line: &str) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    pub fn log_entry(
        &self,
        agent_id: &str,
        tool_name: &str,
        path: &str,
        success: bool,
        message: &str,
    ) -> Result<()> {
        let severity = match tool_name {
            "delete_file" | "fs_delete" => "destructive",
            _ => "normal",
        };

        let entry = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "agent_id": agent_id,
            "tool_name": tool_name,
            "path": path,
            "success": success,
            "severity": severity,
            "message": message,
        });

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", entry)?;
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!("[Journal] Open failed: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_logs_destructive_severity() {
        let tmp = std::env::temp_dir().join(format!(
            "test_delete_journal_{}.jsonl",
            uuid::Uuid::new_v4()
        ));
        let journal = ExecutionJournal {
            path: tmp.clone(),
            denial_counts: Arc::new(Mutex::new(HashMap::new())),
        };

        let _ = journal.log_entry(
            "business_runtime",
            "delete_file",
            "/tmp/test.txt",
            true,
            "Deleted file",
        );

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("\"severity\":\"destructive\""));
        assert!(content.contains("\"tool_name\":\"delete_file\""));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_journal_logs_normal_severity() {
        let tmp =
            std::env::temp_dir().join(format!("test_read_journal_{}.jsonl", uuid::Uuid::new_v4()));
        let journal = ExecutionJournal {
            path: tmp.clone(),
            denial_counts: Arc::new(Mutex::new(HashMap::new())),
        };

        let _ = journal.log_entry(
            "agent1",
            "read_file",
            "/tmp/test.txt",
            true,
            "Read 100 bytes",
        );

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("\"severity\":\"normal\""));

        std::fs::remove_file(&tmp).ok();
    }
}
