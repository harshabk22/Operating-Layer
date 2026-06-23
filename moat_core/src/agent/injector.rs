use crate::agent::llm_types::{AgentSpec, UserProfile};
use crate::agent::system::HostEnvironment;
use sha2::{Digest, Sha256};

use crate::MoatResult;
use chrono::Local;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub struct InjectorConfig {
    pub top_k_memories: usize,
    pub max_memory_chars: usize,
}

#[derive(Debug, Clone)]
pub struct FrozenPrefix {
    pub persona_id: String,
    pub persona_version: u32,
    pub content_hash: [u8; 32],
    pub system_prompt: String,
    pub resolved_persona_text: String,
    pub generated_at: i64,
}

impl FrozenPrefix {
    pub fn build(agent_name: &str, agent_spec: Option<&AgentSpec>) -> Self {
        let persona_text = agent_spec
            .map(|s| s.raw_markdown.clone())
            .unwrap_or_default();
        let persona_id = agent_spec
            .map(|s| s.name.clone())
            .unwrap_or_else(|| agent_name.to_string());
        let persona_version = 1u32;
        let system_prompt = crate::planner::prompt::build_react_system_prompt(agent_spec, &[], &[]);

        let mut hasher = Sha256::new();
        hasher.update(persona_text.as_bytes());
        hasher.update(system_prompt.as_bytes());
        let content_hash: [u8; 32] = hasher.finalize().into();

        Self {
            persona_id,
            persona_version,
            content_hash,
            system_prompt,
            resolved_persona_text: persona_text,
            generated_at: chrono::Utc::now().timestamp(),
        }
    }
}

fn host_environment_xml() -> String {
    let env = HostEnvironment::detect();
    let os_name = env.os_name;
    let username = env.username;
    let home_dir = env.home_dir.display().to_string();
    format!(
        "<host_environment>\nOS: {os_name}\nUser: {username}\nHome Directory: {home_dir}\n</host_environment>"
    )
}

pub fn build_injected_prompt(user_query: &str, memory_context_xml: &str) -> MoatResult<String> {
    build_injected_prompt_full(user_query, memory_context_xml, None, None, None, None)
}

pub fn build_injected_prompt_with_context(
    user_query: &str,
    memory_context_xml: &str,
    precomputed_context: Option<&str>,
    user_profile: Option<&UserProfile>,
    agent_spec: Option<&AgentSpec>,
    shared_execution_notes: Option<&str>,
) -> MoatResult<String> {
    build_injected_prompt_full(
        user_query,
        memory_context_xml,
        precomputed_context,
        user_profile,
        agent_spec,
        shared_execution_notes,
    )
}

fn build_injected_prompt_full(
    user_query: &str,
    memory_context_xml: &str,
    precomputed_context: Option<&str>,
    user_profile: Option<&UserProfile>,
    agent_spec: Option<&AgentSpec>,
    shared_execution_notes: Option<&str>,
) -> MoatResult<String> {
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let system_state = format!(
        "<system_state>\nCurrent Time: {}\nOS: {}\n</system_state>",
        current_time,
        HostEnvironment::detect().os_name
    );
    let host_environment = host_environment_xml();

    let capacity = 160 + user_query.len();
    let mut prompt = String::with_capacity(
        capacity + memory_context_xml.len() + system_state.len() + host_environment.len(),
    );
    prompt.push_str(memory_context_xml.trim());
    if let Some(profile) = user_profile {
        prompt.push_str("\n<user_profile>\n");
        prompt.push_str(&profile.raw_markdown);
        prompt.push_str("\n</user_profile>");
    }
    if let Some(spec) = agent_spec {
        prompt.push_str("\n<agent_spec>\n");
        prompt.push_str(&spec.raw_markdown);
        prompt.push_str("\n</agent_spec>");
    }
    if let Some(context) = precomputed_context {
        let trimmed = context.trim();
        if !trimmed.is_empty() {
            prompt.push_str("\n[SESSION CONTEXT]\n");
            prompt.push_str(trimmed);
        }
    }
    if let Some(notes) = shared_execution_notes {
        let trimmed = notes.trim();
        if !trimmed.is_empty() {
            prompt.push_str("\n[EXECUTION NOTES]\n");
            prompt.push_str(trimmed);
        }
    }

    prompt.push_str("[USER QUERY]\n");
    prompt.push_str(user_query);
    prompt.push('\n');
    prompt.push_str(&host_environment);
    prompt.push('\n');
    prompt.push_str(&system_state);
    Ok(prompt)
}

/// Slim context injection for atomic/leaf workflow nodes.
/// Saves ~1,500-2,000 tokens per invocation by omitting recalled knowledge.
pub fn build_slim_prompt(
    user_query: &str,
    precomputed_context: Option<&str>,
    agent_spec: Option<&AgentSpec>,
    shared_execution_notes: Option<&str>,
) -> MoatResult<String> {
    let env = HostEnvironment::detect();
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut prompt = String::with_capacity(512 + user_query.len());

    if let Some(spec) = agent_spec {
        prompt.push_str("<agent_spec>\n");
        prompt.push_str(&spec.raw_markdown);
        prompt.push_str("\n</agent_spec>\n");
    }
    if let Some(ctx) = precomputed_context {
        let trimmed = ctx.trim();
        if !trimmed.is_empty() {
            prompt.push_str("[SESSION CONTEXT]\n");
            prompt.push_str(trimmed);
            prompt.push('\n');
        }
    }
    if let Some(notes) = shared_execution_notes {
        let trimmed = notes.trim();
        if !trimmed.is_empty() {
            prompt.push_str("[EXECUTION NOTES]\n");
            prompt.push_str(trimmed);
            prompt.push('\n');
        }
    }
    prompt.push_str("[USER QUERY]\n");
    prompt.push_str(user_query);
    prompt.push('\n');
    let _ = write!(
        prompt,
        "<host_environment>\nOS: {}\nUser: {}\nHome Directory: {}\n</host_environment>\n\
         <system_state>\nCurrent Time: {}\n</system_state>",
        env.os_name,
        env.username,
        env.home_dir.display(),
        current_time
    );
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::{build_injected_prompt, build_injected_prompt_with_context};
    use crate::agent::llm_types::{AgentSpec, UserProfile};

    #[test]
    fn injected_prompt_places_system_state_after_query() {
        let prompt = build_injected_prompt(
            "Who am I?",
            "<knowledge_memory><facts><fact>Name: Harsha</fact></facts><search_hints>harsha identity</search_hints></knowledge_memory>",
        )
        .expect("build prompt");

        let meta_pos = prompt.find("<knowledge_memory>").expect("knowledge memory");
        let query_pos = prompt.find("[USER QUERY]").expect("query");
        let host_pos = prompt.find("<host_environment>").expect("host environment");
        let system_pos = prompt.find("<system_state>").expect("system state");
        assert!(meta_pos < query_pos);
        assert!(query_pos < host_pos);
        assert!(host_pos < system_pos);
        assert!(!prompt.contains("<rules>"));
        assert!(prompt.contains("Home Directory: "));
        assert!(prompt.ends_with("</system_state>"));
    }

    #[test]
    fn injected_prompt_includes_agent_context() {
        let prompt = build_injected_prompt_with_context(
            "Plan the task",
            "<knowledge_memory><facts></facts><search_hints>task</search_hints></knowledge_memory>",
            Some("Prior context summary."),
            Some(&UserProfile {
                name: "Harsha".into(),
                core_preferences: "prefers concise summaries".into(),
                chat_model: None,
                planner_model: None,
                worker_model: None,
                summary: "Harsha is the operator.".into(),
                facts: vec!["prefers concise summaries".into()],
                body_markdown: "Harsha is the operator.\n- prefers concise summaries".into(),
                raw_markdown: "# User\nHarsha is the operator.\n- prefers concise summaries".into(),
            }),
            Some(&AgentSpec {
                name: "planner".into(),
                role: "Plans work".into(),
                authoritative: false,
                api_provider: "google".into(),
                model_id: "meta/llama-3.1-405b-instruct".into(),
                allowed_tools: vec!["done".into()],
                raw_markdown: "## planner\nPlans work.\nFocus on decomposition".into(),
            }),
            None,
        )
        .expect("build prompt");

        assert!(prompt.contains("<user_profile>"));
        assert!(prompt.contains("Harsha is the operator."));
        assert!(prompt.contains("[SESSION CONTEXT]"));
        assert!(prompt.contains("<agent_spec>"));
        assert!(prompt.contains("Focus on decomposition"));
        assert!(!prompt.contains("executor"));
    }
}
