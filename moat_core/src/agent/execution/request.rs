use crate::agent::capabilities::Capability;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub request_id: String,
    pub agent_id: String,
    pub tool_name: String,
    pub capability: Capability,
    pub args: serde_json::Value,
    pub working_dir: PathBuf,
    pub timestamp: i64,
    #[serde(default)]
    pub session_id: String,
}

impl ExecutionRequest {
    pub fn new(
        agent_id: &str,
        tool_name: &str,
        capability: Capability,
        args: serde_json::Value,
    ) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.to_string(),
            tool_name: tool_name.to_string(),
            capability,
            args,
            working_dir: Self::default_workspace(),
            timestamp: Self::now(),
            session_id: String::new(),
        }
    }

    pub fn with_session_id(mut self, session_id: String) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = request_id;
        self
    }

    fn default_workspace() -> PathBuf {
        dirs::home_dir()
            .expect("Cannot determine home directory")
            .join("Desktop")
            .join("ageex-workspace")
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }
}
