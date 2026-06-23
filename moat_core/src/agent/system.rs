// moat_core/src/system.rs
//
// System and Environment Context abstractions for the Business Communication Runtime.

use chrono::Local;
use std::{collections::HashMap, env, path::PathBuf};

#[derive(Debug, Clone)]
pub struct HostEnvironment {
    pub os_name: String,
    pub username: String,
    pub home_dir: PathBuf,
}

impl HostEnvironment {
    pub fn detect() -> Self {
        let os_name = match env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        }
        .to_string();
        let username = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unknown".to_string());
        let home_dir = dirs::home_dir()
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            os_name,
            username,
            home_dir,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            "<host_environment>\nCurrent Time: {}\nOS: {}\nUser: {}\nHome Directory: {}\n</host_environment>",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            self.os_name,
            self.username,
            self.home_dir.display()
        )
    }
}

/// Typed shared execution state for a single workflow run.
/// Replaces the unstructured String scratchpad with structured fields
/// that provide mechanical capability denial filtering and clean prompt injection.
#[derive(Debug, Clone, Default)]
pub struct ExecutionWorldState {
    /// Capability+target pairs denied during this workflow run.
    /// Format: "action:target" e.g. "fs:read:/tmp/secret", or bare "process:spawn".
    pub denied_capabilities: Vec<String>,
    /// Key-value facts discovered by workers (e.g. OS version, file paths found).
    pub discovered_facts: HashMap<String, String>,
    /// Task IDs that completed with a one-line summary. Format: "[SUCCESS|FAILED] task_id (worker): summary"
    pub completed_subtasks: Vec<String>,
    /// Artifact paths produced by workers, keyed by task_id.
    pub live_artifacts: HashMap<String, String>,
}

impl ExecutionWorldState {
    /// Maximum characters allowed in the serialized XML to prevent token budget exhaustion.
    const MAX_XML_CHARS: usize = 4000;

    /// Serialize to structured XML for prompt injection.
    /// Caps output at `MAX_XML_CHARS` by truncating oldest completed_subtasks first,
    /// since denied_capabilities are the most critical data for downstream workers.
    pub fn to_xml_context(&self) -> String {
        let mut xml = String::with_capacity(Self::MAX_XML_CHARS);
        xml.push_str("<execution_world_state>\n");

        if !self.denied_capabilities.is_empty() {
            xml.push_str("<denied_capabilities>\n");
            for cap in &self.denied_capabilities {
                xml.push_str("- ");
                xml.push_str(cap);
                xml.push('\n');
            }
            xml.push_str("</denied_capabilities>\n");
        }

        if !self.completed_subtasks.is_empty() {
            xml.push_str("<completed_subtasks>\n");
            // If we're over budget, skip oldest entries (keep denials + newest completions)
            let skip = if xml.len() > Self::MAX_XML_CHARS / 2 {
                self.completed_subtasks.len().saturating_sub(3)
            } else {
                0
            };
            for entry in self.completed_subtasks.iter().skip(skip) {
                xml.push_str("- ");
                xml.push_str(entry);
                xml.push('\n');
            }
            xml.push_str("</completed_subtasks>\n");
        }

        if !self.discovered_facts.is_empty() && xml.len() < Self::MAX_XML_CHARS.saturating_sub(256)
        {
            xml.push_str("<discovered_facts>\n");
            for (key, value) in &self.discovered_facts {
                if xml.len() >= Self::MAX_XML_CHARS.saturating_sub(48) {
                    break;
                }
                xml.push_str("- ");
                xml.push_str(key);
                xml.push_str(": ");
                xml.push_str(value);
                xml.push('\n');
            }
            xml.push_str("</discovered_facts>\n");
        }

        if !self.live_artifacts.is_empty() && xml.len() < Self::MAX_XML_CHARS.saturating_sub(256) {
            xml.push_str("<live_artifacts>\n");
            for (task_id, path) in &self.live_artifacts {
                if xml.len() >= Self::MAX_XML_CHARS.saturating_sub(48) {
                    break;
                }
                xml.push_str("- ");
                xml.push_str(task_id);
                xml.push_str(": ");
                xml.push_str(path);
                xml.push('\n');
            }
            xml.push_str("</live_artifacts>\n");
        }

        xml.push_str("</execution_world_state>");

        xml
    }

    /// Check if a specific action+target combination was already denied.
    /// Matches exact "action:target" combination only — does NOT broaden to entire action namespace.
    pub fn is_capability_denied(&self, action: &str, target: &str) -> bool {
        let exact_key = if target.trim().is_empty() {
            action.to_string()
        } else {
            format!("{}:{}", action, target.trim())
        };
        self.denied_capabilities.iter().any(|d| d == &exact_key)
    }

    /// Record a denied capability. Deduplicates automatically.
    pub fn record_denial(&mut self, action: &str, target: &str) {
        let key = if target.trim().is_empty() {
            action.to_string()
        } else {
            format!("{}:{}", action, target.trim())
        };
        if !self.denied_capabilities.contains(&key) {
            self.denied_capabilities.push(key);
        }
    }

    /// Record a completed subtask with summary.
    pub fn record_completion(&mut self, task_id: &str, worker: &str, summary: &str, success: bool) {
        let tag = if success { "SUCCESS" } else { "FAILED" };
        let clipped_summary: String = summary.chars().take(200).collect();
        self.completed_subtasks.push(format!(
            "[{}] {} ({}): {}",
            tag, task_id, worker, clipped_summary
        ));
    }

    /// Record a produced artifact.
    pub fn record_artifact(&mut self, task_id: &str, path: &str) {
        self.live_artifacts
            .insert(task_id.to_string(), path.to_string());
    }

    /// Extract denied capabilities from a worker failure summary.
    /// Parses the "CAPABILITY DENIED" prefix used consistently across the orchestrator.
    pub fn extract_denials_from_summary(&mut self, summary: &str) {
        for line in summary.lines() {
            let line = line.trim();
            if let Some(rest) = line
                .strip_prefix("CAPABILITY DENIED:")
                .or_else(|| line.strip_prefix("PERMISSION DENIED:"))
            {
                let denial = rest.trim();
                if !denial.is_empty() && !self.denied_capabilities.contains(&denial.to_string()) {
                    self.denied_capabilities.push(denial.to_string());
                }
            }
        }
    }

    /// Check if the world state is empty (no data recorded yet).
    pub fn is_empty(&self) -> bool {
        self.denied_capabilities.is_empty()
            && self.discovered_facts.is_empty()
            && self.completed_subtasks.is_empty()
            && self.live_artifacts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_environment_detects_properly() {
        let env = HostEnvironment::detect();
        assert!(!env.os_name.is_empty());
        assert!(!env.username.is_empty());
    }

    #[test]
    fn execution_world_state_denial_deduplication() {
        let mut state = ExecutionWorldState::default();
        state.record_denial("fs:read", "/tmp/secret");
        state.record_denial("fs:read", "/tmp/secret");
        state.record_denial("process:spawn", "");
        assert_eq!(state.denied_capabilities.len(), 2);
        assert_eq!(state.denied_capabilities[0], "fs:read:/tmp/secret");
        assert_eq!(state.denied_capabilities[1], "process:spawn");
    }

    #[test]
    fn execution_world_state_is_capability_denied_exact_only() {
        let mut state = ExecutionWorldState::default();
        state.record_denial("fs:read", "/tmp/secret");
        state.record_denial("process:spawn", "");

        assert!(state.is_capability_denied("fs:read", "/tmp/secret"));
        assert!(state.is_capability_denied("process:spawn", ""));
        assert!(!state.is_capability_denied("process:spawn", "anything"));
        assert!(!state.is_capability_denied("fs:write", "/tmp/secret"));
        assert!(!state.is_capability_denied("fs:read", "/other/path"));
    }

    #[test]
    fn execution_world_state_to_xml_output_format() {
        let mut state = ExecutionWorldState::default();
        state.record_denial("fs:read", "/tmp/secret");
        state.record_completion("task_1", "core_os", "Opened Safari", true);
        state.record_completion("task_2", "coder", "Permission denied", false);
        state.record_artifact("task_1", "/tmp/output.txt");

        let xml = state.to_xml_context();
        assert!(xml.contains("<execution_world_state>"));
        assert!(xml.contains("</execution_world_state>"));
        assert!(xml.contains("<denied_capabilities>"));
        assert!(xml.contains("fs:read:/tmp/secret"));
        assert!(xml.contains("<completed_subtasks>"));
        assert!(xml.contains("[SUCCESS] task_1 (core_os): Opened Safari"));
        assert!(xml.contains("[FAILED] task_2 (coder): Permission denied"));
        assert!(xml.contains("<live_artifacts>"));
        assert!(xml.contains("/tmp/output.txt"));
    }

    #[test]
    fn execution_world_state_extract_denials_from_summary() {
        let mut state = ExecutionWorldState::default();
        let summary = "CAPABILITY DENIED: path '/etc/shadow' is outside allowed paths\nSome other line\nPERMISSION DENIED: write access blocked";
        state.extract_denials_from_summary(summary);

        assert_eq!(state.denied_capabilities.len(), 2);
        assert!(state
            .denied_capabilities
            .iter()
            .any(|d| d.contains("path '/etc/shadow'")));
        assert!(state
            .denied_capabilities
            .iter()
            .any(|d| d.contains("write access blocked")));

        // Dedup on re-extract
        state.extract_denials_from_summary(summary);
        assert_eq!(state.denied_capabilities.len(), 2);
    }
}
