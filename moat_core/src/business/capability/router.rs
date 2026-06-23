use super::plugin::CapabilityPlugin;
use crate::MoatResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct CapabilityRouter {
    plugins: HashMap<String, Arc<dyn CapabilityPlugin>>,
}

impl Default for CapabilityRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRouter {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: &str, plugin: Arc<dyn CapabilityPlugin>) {
        self.plugins.insert(name.to_string(), plugin);
    }

    pub fn execute(
        &self,
        guard: &crate::v2::identity::core::CapabilityGuard,
        capability: &str,
        params: &Value,
    ) -> MoatResult<Value> {
        // Phase 6: Precondition check via CapabilityGuard and RBAC
        guard.authorize("capability", capability).map_err(|e| crate::MoatError::Orchestrator(e))?;

        for plugin in self.plugins.values() {
            for cap_def in plugin.capabilities() {
                if cap_def.name == capability {
                    return plugin.execute(capability, params);
                }
            }
        }
        Err(crate::MoatError::Orchestrator(format!(
            "No plugin registered for capability: {}",
            capability
        )))
    }

    pub fn has_capability(&self, capability: &str) -> bool {
        self.plugins
            .values()
            .any(|p| p.capabilities().iter().any(|c| c.name == capability))
    }
}
