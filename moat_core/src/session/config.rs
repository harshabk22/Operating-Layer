use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct AgentConfig;

impl AgentConfig {
    pub fn load_agent1_prompt() -> Result<String> {
        Self::load_prompt("agent1", "agents/agent1.md")
    }

    pub fn load_business_runtime_prompt() -> Result<String> {
        Self::load_prompt("business runtime", "prompts/business_runtime.md")
    }

    pub fn load_operator_assistant_prompt() -> Result<String> {
        Self::load_prompt("operator assistant", "prompts/operator_assistant.md")
    }

    fn load_prompt(display_name: &str, relative_path: &str) -> Result<String> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut candidates: Vec<PathBuf> = vec![
            cwd.join(relative_path),
            cwd.join("moat_core").join(relative_path),
            PathBuf::from(relative_path),
            PathBuf::from(format!("moat_core/{}", relative_path)),
        ];
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join(relative_path));
            candidates.push(parent.join("moat_core").join(relative_path));
        }

        for path in &candidates {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                println!(
                    "[Config] Loaded {} prompt from {}",
                    display_name,
                    path.display()
                );
                return Ok(content);
            }
        }

        println!(
            "[Config] {} not found, using built-in defaults",
            relative_path
        );
        Ok(Self::default_prompt(display_name))
    }

    fn default_prompt(display_name: &str) -> String {
        match display_name {
            "agent1" => "You are a read-only assistant. You can read files, check if files exist, open applications, and open URLs. You CANNOT write files, execute terminal commands, or use skills. If asked to write or execute, tell the user you cannot do that.".into(),
            "business runtime" => "You are the primary customer-facing conversation runtime. You handle customer communication, lead qualification, knowledge retrieval, opportunity progression, booking proposal generation, escalation detection, and authorization request creation. Never claim to be human, pretend to be the owner, or invent company policies.".into(),
            "operator assistant" => "You are the internal operator support agent. You handle authorization review, escalation review, suggested responses, opportunity summaries, and customer history summaries.".into(),
            _ => "You are a helpful assistant.".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_prompt_loads_when_file_missing() {
        let prompt = AgentConfig::default_prompt("agent1");
        assert!(!prompt.is_empty());
        assert!(prompt.contains("read-only"));
    }
}
