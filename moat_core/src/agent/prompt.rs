use crate::agent::llm_types::AgentSpec;
use crate::memory::skills::SynthesizedSkill;

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

pub fn build_react_system_prompt(
    active_agent: Option<&AgentSpec>,
    synthesized_skills: &[SynthesizedSkill],
    instruction_skills: &[crate::memory::skills::SkillDefinition],
) -> String {
    let mut prompt = REACT_SYSTEM_TEMPLATE.to_string();

    let registry = crate::agent::tools::global_registry().read().unwrap();

    let tool_list: Vec<String> = match active_agent {
        Some(spec) if !spec.allowed_tools.is_empty() => {
            let mut tools: Vec<String> = Vec::new();
            for tool in &spec.allowed_tools {
                if registry.get(tool).is_some() {
                    if !tools.iter().any(|existing| existing.eq_ignore_ascii_case(tool)) {
                        tools.push(tool.clone());
                    }
                }
            }
            if !tools.iter().any(|t| t.eq_ignore_ascii_case("done")) {
                tools.push("done".to_string());
            }
            tools
        }
        _ => registry.names(),
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
        if let Some(tool) = registry.get(tool_name) {
            prompt.push_str(&format!("- \"{}\": {}\n", tool_name, tool.description()));
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
