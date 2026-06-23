use crate::agent::llm_types::AgentSpec;
use crate::memory::skills::SynthesizedSkill;

// ─────────────────────────────────────────────────────────────────────────────
// § System Prompt Template
// ─────────────────────────────────────────────────────────────────────────────

pub const REACT_SYSTEM_TEMPLATE: &str = r#"SYSTEM: MoatAgeex — AI Receptionist & Business Interaction Engine.

TASK → TOOL (use directly — do NOT pre-explore):
• Resolve contact identity → identity:resolve (channel, external_id)
• Classify/update opportunity status → opportunity:classify (thread_id, status)
• Propose a new engagement/booking → engagement:propose (contact_id, thread_id, title, start_time, end_time)
• Request operator approval for engagement → engagement:request_authorization (engagement_id)
• Authorize a proposed engagement → engagement:authorize (engagement_id)
• Query knowledge base for customer answers → knowledge:query (query, limit)
• Escalate thread to human operator → operator:escalate (thread_id, reason)

FINISHING: done (payload: your response text)

COMPLETION RULES (CRITICAL):
- After successful completion of the interaction or action, emit 'done' with the response for the customer.
- Do NOT repeat the same actions. SUCCESS = TASK COMPLETE.
- Successful tool results OBSERVE SUCCESS convergence toward done. Do not loop.

ANTI-EXPLORATION RULES:
- For single-step tasks, call the tool DIRECTLY. Do NOT pre-check.
- If you know the info, reply immediately. No query needed.

IMPORTANT RULES:
- Never assume contact identity. Resolve it first.
- Keep responses concise. One or two sentence answers when possible.
- If repeated failures occur (3 consecutive failures), escalate to human operator.
- Never output "Session Summary", "Previous actions", or recap history. The done payload must be a direct, concise response to the user.
"#;

// ─────────────────────────────────────────────────────────────────────────────
// § Tool Registry
// ─────────────────────────────────────────────────────────────────────────────

/// All known built-in tool names, used as fallback when no agent spec restricts the tool set.
pub(crate) const ALL_TOOLS: &[&str] = &[
    "identity:resolve",
    "opportunity:classify",
    "engagement:propose",
    "engagement:request_authorization",
    "engagement:authorize",
    "operator:escalate",
    "knowledge:query",
    "done",
];

/// Maps a tool name to its JSON schema description for prompt injection.
/// Returns `None` for unknown tool names so they are silently excluded.
pub(crate) fn tool_schema_entry(tool: &str) -> Option<(&'static str, &'static str)> {
    match tool {
        "identity:resolve" => Some(("identity:resolve", "Resolve messaging handle/channel to a contact ID. Creates a new contact if not found.")),
        "opportunity:classify" => Some(("opportunity:classify", "Classify or update the lead/opportunity status for an interaction thread.")),
        "engagement:propose" => Some(("engagement:propose", "Propose a new engagement or booking for a contact.")),
        "engagement:request_authorization" => Some(("engagement:request_authorization", "Request operator approval for a proposed engagement before it is executed.")),
        "engagement:authorize" => Some(("engagement:authorize", "Authorize and execute a pending engagement via the scheduling integration.")),
        "operator:escalate" => Some(("operator:escalate", "Escalate the conversation thread to a human operator.")),
        "knowledge:query" => Some(("knowledge:query", "Query the organizational knowledge base. Use when you need info to answer customer questions.")),
        "done" => Some(("done", "Signal task completion and provide the final response to the user.")),
        _ => None,
    }
}

/// Build the ReAct system prompt with dynamic tool schema and skill injections.
/// The caller is responsible for fetching synthesized skills and passing them in.
pub(crate) fn build_react_system_prompt(
    active_agent: Option<&AgentSpec>,
    synthesized_skills: &[SynthesizedSkill],
    instruction_skills: &[crate::memory::skills::SkillDefinition],
) -> String {
    let mut prompt = REACT_SYSTEM_TEMPLATE.to_string();

    // ── Dynamic ACTION SCHEMA based on agent-local allowed_tools ──
    let tool_list: Vec<&str> = match active_agent {
        Some(spec) if !spec.allowed_tools.is_empty() => {
            let mut tools: Vec<&str> = Vec::new();
            for tool in &spec.allowed_tools {
                if let Some((name, _)) = tool_schema_entry(tool) {
                    if !tools
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(name))
                    {
                        tools.push(name);
                    }
                }
            }
            // Always include done — universally available.
            for essential in &["done"] {
                if !tools.iter().any(|t| t.eq_ignore_ascii_case(essential)) {
                    tools.push(essential);
                }
            }
            tools
        }
        _ => ALL_TOOLS.to_vec(),
    };

    prompt.push_str("\n\nJSON SCHEMA:\n");
    prompt.push_str(r#"{
  "state": "running" | "completed",
  "thought": "Brief reasoning explaining your next step or final outcome (max 30 words).",
  "actions": [
    {
      "action": "name of the tool to execute",
      "target": "target argument for the tool (e.g. absolute file path, exact app name, URL, or 'user')",
      "payload": <payload argument for the tool>,
      "reason": "optional explanation/justification for calling this tool"
    }
  ],
  "summary": "final answer summary to the user (only when state is 'completed')"
}

If you are calling a tool, set state to "running" and provide the action in the actions array.
If you are finished and ready to reply to the user, set state to "completed", leave the actions array empty, and provide the final response in the summary field.
"#);

    prompt.push_str("\n\nALLOWED ACTIONS SCHEMA:\n");
    prompt.push_str("For each item in the \"actions\" array, \"action\" must be one of the following allowed actions:\n");
    for tool_name in &tool_list {
        if let Some((_, schema_desc)) = tool_schema_entry(tool_name) {
            prompt.push_str(&format!("- \"{}\": {}\n", tool_name, schema_desc));
        }
    }

    // ── Synthesized system skills ──
    if !synthesized_skills.is_empty() {
        prompt.push_str("\n\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        prompt.push_str("SYNTHESIZED SYSTEM SKILLS\n");
        prompt.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        prompt.push_str(
            "The following installed skills are valid action names for this session.\n\
             For these tools, set target to \"system\" and payload to a JSON object matching the exact parameter schema.\n\
             Do not invent or omit keys.\n\n",
        );
        for skill in synthesized_skills {
            prompt.push_str(&format!(
                "- action: \"{}\"\n  description: {}\n  parameter_schema: {}\n",
                skill.skill_name,
                skill.description.trim(),
                skill.parameter_schema
            ));
        }
    }

    // ── Instruction skills ──
    if !instruction_skills.is_empty() {
        prompt.push_str("\n\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        prompt.push_str("INSTRUCTION SKILLS (GUIDANCE & CONSTRAINTS)\n");
        prompt.push_str("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
        prompt.push_str(
            "The following static skills provide specific prompt/workflow guidance, constraints, or best practices.\n\
             You must adhere to these instructions when executing relevant tasks:\n\n",
        );
        for skill in instruction_skills {
            prompt.push_str(&skill.prompt_summary());
            prompt.push('\n');
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_respects_allowed_tools() {
        let agent = AgentSpec {
            name: "test_agent".into(),
            role: "test".into(),
            authoritative: false,
            api_provider: "google".into(),
            model_id: "test".into(),
            allowed_tools: vec!["identity:resolve".into()],
            raw_markdown: "".into(),
        };
        let prompt = build_react_system_prompt(Some(&agent), &[], &[]);
        assert!(prompt.contains("identity:resolve"));
        assert!(!prompt.contains("- \"opportunity:classify\":"));
    }

    #[test]
    fn test_build_prompt_includes_synthesized_skills() {
        let skills = vec![SynthesizedSkill {
            skill_name: "test_skill".into(),
            description: "test description".into(),
            script_path: "".into(),
            code_hash: "".into(),
            language: "python".into(),
            parameter_schema: "{}".into(),
            created_at: "".into(),
        }];
        let prompt = build_react_system_prompt(None, &skills, &[]);
        assert!(prompt.contains("test_skill"));
        assert!(prompt.contains("test description"));
    }

    #[test]
    fn test_build_prompt_includes_instruction_skills() {
        let skill = crate::memory::skills::SkillDefinition {
            name: "test_instruction_skill".into(),
            description: "instruction test description".into(),
            triggers: vec!["test_trigger".into()],
            workflow: vec!["Step 1: Test".into()],
            constraints: vec!["Constraint 1: Test".into()],
            best_practices: vec![],
            raw_markdown: "".into(),
            version: None,
            category: None,
            required_tools: vec![],
            delegation: vec![],
            status: None,
            schema_version: None,
        };
        let prompt = build_react_system_prompt(None, &[], &[skill]);
        assert!(prompt.contains("test_instruction_skill"));
        assert!(prompt.contains("instruction test description"));
        assert!(prompt.contains("Step 1: Test"));
        assert!(prompt.contains("Constraint 1: Test"));
    }
}
