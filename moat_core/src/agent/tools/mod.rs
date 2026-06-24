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

use std::sync::RwLock;
use std::sync::OnceLock;

pub static GLOBAL_TOOL_REGISTRY: OnceLock<RwLock<ToolRegistry>> = OnceLock::new();

pub fn global_registry() -> &'static RwLock<ToolRegistry> {
    GLOBAL_TOOL_REGISTRY.get_or_init(|| {
        let mut reg = ToolRegistry::new();
        reg.register(ToolEntry::new(SimpleTool { name: "identity:resolve", description: "Resolve messaging handle/channel to a contact ID. Creates a new contact if not found.", reversibility: ToolReversibility::Reversible }));
        reg.register(ToolEntry::new(SimpleTool { name: "opportunity:classify", description: "Classify or update the lead/opportunity status for an interaction thread.", reversibility: ToolReversibility::Staged }));
        reg.register(ToolEntry::new(SimpleTool { name: "engagement:propose", description: "Propose a new engagement or booking for a contact.", reversibility: ToolReversibility::Staged }));
        reg.register(ToolEntry::new(SimpleTool { name: "engagement:request_authorization", description: "Request operator approval for a proposed engagement before it is executed.", reversibility: ToolReversibility::Reversible }));
        reg.register(ToolEntry::new(SimpleTool { name: "engagement:authorize", description: "Authorize and execute a pending engagement via the scheduling integration.", reversibility: ToolReversibility::Reversible }));
        reg.register(ToolEntry::new(SimpleTool { name: "operator:escalate", description: "Escalate the conversation thread to a human operator.", reversibility: ToolReversibility::Reversible }));
        reg.register(ToolEntry::new(SimpleTool { name: "knowledge:query", description: "Query the organizational knowledge base. Use when you need info to answer customer questions.", reversibility: ToolReversibility::Reversible }));
        reg.register(ToolEntry::new(SimpleTool { name: "done", description: "Signal task completion and provide the final response to the user.", reversibility: ToolReversibility::Reversible }));
        RwLock::new(reg)
    })
}

pub struct SimpleTool {
    pub name: &'static str,
    pub description: &'static str,
    pub reversibility: ToolReversibility,
}

#[async_trait::async_trait]
impl Tool for SimpleTool {
    fn name(&self) -> &'static str { self.name }
    fn description(&self) -> &'static str { self.description }
    fn parameters(&self) -> Value { serde_json::json!({}) }
    fn reversibility(&self) -> ToolReversibility { self.reversibility }
    fn required_permission(&self) -> PermissionFlags { PermissionFlags(0) }
    async fn execute(&self, _intent: &ActionIntent) -> MoatResult<TaskStatus> {
        Ok(TaskStatus::Success { stdout: "SimpleTool executed".to_string() })
    }
}

pub struct DynamicPackageTool {
    pub name: &'static str,
    pub description: &'static str,
    pub reversibility: ToolReversibility,
}

#[async_trait::async_trait]
impl Tool for DynamicPackageTool {
    fn name(&self) -> &'static str { self.name }
    fn description(&self) -> &'static str { self.description }
    fn parameters(&self) -> Value { serde_json::json!({}) }
    fn reversibility(&self) -> ToolReversibility { self.reversibility }
    fn required_permission(&self) -> PermissionFlags { PermissionFlags(0) }
    async fn execute(&self, _intent: &ActionIntent) -> MoatResult<TaskStatus> {
        Ok(TaskStatus::Success { stdout: format!("Dynamic tool {} executed successfully.", self.name) })
    }
}
