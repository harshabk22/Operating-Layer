pub mod executor;

pub use executor::ToolExecutor;

use crate::agent::interaction_orchestrator::{ActionIntent, TaskStatus};
use crate::memory::meta_brain::PermissionFlags;
use crate::MoatResult;
use serde_json::Value;
use std::ops::Deref;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters(&self) -> Value;
    fn reversibility(&self) -> ToolReversibility;
    fn required_permission(&self) -> PermissionFlags;
    async fn execute(&self, intent: &ActionIntent) -> MoatResult<TaskStatus>;
}

pub struct ToolEntry(Arc<dyn Tool>);

impl ToolEntry {
    pub fn new<T: Tool + 'static>(tool: T) -> Self {
        Self(Arc::new(tool))
    }
}

impl Deref for ToolEntry {
    type Target = dyn Tool;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Clone for ToolEntry {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

use std::collections::HashMap;

pub struct ToolRegistry {
    tools: HashMap<String, ToolEntry>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: ToolEntry) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolReversibility {
    Reversible,
    Staged,
    Committed,
}

impl ToolReversibility {
    pub fn classify(action: &str) -> Self {
        match action {
            "fs:read" | "memory:search" | "session:search" | "memory:learn"
            | "meta:query_brain" | "done" | "identity:resolve" | "knowledge:query" => {
                Self::Reversible
            }
            "fs:write" | "fs:mkdir" | "opportunity:classify" | "engagement:propose" => Self::Staged,
            "process:spawn"
            | "net:curl"
            | "engagement:request_authorization"
            | "engagement:authorize"
            | "operator:escalate"
            | "propose_system_skill" => Self::Committed,
            _ => Self::Committed,
        }
    }
}
