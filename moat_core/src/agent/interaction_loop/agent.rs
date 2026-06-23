use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use uuid::Uuid;

use crate::agent::consent::clean_consent_summary;
use crate::agent::execution::journal_chain::ChainJournal;
use crate::agent::execution::DelErrorCode;
use crate::agent::injector::InjectorConfig;
use crate::agent::interaction_loop::{
    context::{
        context_len, prune_context_if_needed, push_context, snapshot_context, SharedContext,
        VersionedContext,
    },
    loop_detector::{LoopDetector, LoopVerdict},
    ContextRehydrator, YieldSnapshot,
};
use crate::agent::interaction_orchestrator::{
    ActionIntent, AskChoicePayload, LearnPayload, LlmMessage, ReactContinuation, ReactOutcome,
    RiskBudget, SubTaskResult, TaskStatus, UiActionBridge,
};
use crate::agent::llm_types::AgentSpec;
use crate::agent::worker::consent::{
    cascade_denial_count, cascade_denial_increment, value_to_string,
};
use crate::agent::worker::WorkerAgent;
use crate::hooks::run_active_memory_prelude;
use crate::integration::external_api::{
    missing_key_provider_from_error, provider_display_name,
};
use crate::memory;
use crate::memory::meta_brain::MetaBrain;
use crate::memory::skills::{get_skill, SynthesizedSkill};
use crate::planner::prompt::build_react_system_prompt;
use crate::planner::PlannerAgent;
use crate::{MoatError, MoatResult};

pub struct Agent {
    pub spec: Option<AgentSpec>,
    pub planner: PlannerAgent,
    pub shared_ctx: SharedContext,
    pub loop_detector: LoopDetector,
    pub cancel_flag: Arc<AtomicBool>,
    pub injector_config: InjectorConfig,
    pub worker: Arc<WorkerAgent>,
    pub meta_brain: Arc<AsyncRwLock<MetaBrain>>,
    pub db: Arc<AsyncMutex<Connection>>,
    pub ui_bridge: Option<Arc<dyn UiActionBridge>>,
    pub max_turns: usize,
    pub max_memory_searches: usize,
    pub max_session_searches: usize,
    pub token_limit: usize,
    pub execution_world_state:
        Option<Arc<tokio::sync::RwLock<crate::agent::system::ExecutionWorldState>>>,
    pub session_id: Option<Uuid>,
    pub high_risk_approvals: Option<Arc<RiskBudget>>,
}

impl Agent {
    pub fn new(
        spec: Option<AgentSpec>,
        worker: Arc<WorkerAgent>,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
        planner: PlannerAgent,
        injector_config: InjectorConfig,
    ) -> Self {
        Self {
            spec,
            planner,
            worker,
            meta_brain,
            db,
            shared_ctx: Arc::new(AsyncRwLock::new(VersionedContext::new(Vec::new()))),
            loop_detector: LoopDetector::new(),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            injector_config,
            ui_bridge: None,
            max_turns: 15,
            max_memory_searches: 10,
            max_session_searches: 6,
            token_limit: 24_000,
            execution_world_state: None,
            session_id: None,
            high_risk_approvals: None,
        }
    }

    pub fn is_tool_allowed(allowed_tools: &[String], tool_name: &str) -> bool {
        let normalized_target = tool_name
            .replace("::", ":")
            .replace(':', "_")
            .to_lowercase();
        allowed_tools.iter().any(|allowed| {
            allowed.replace("::", ":").replace(':', "_").to_lowercase() == normalized_target
        })
    }

    fn agent_allows_process_spawn(&self) -> bool {
        self.spec
            .as_ref()
            .map(|spec| {
                spec.allowed_tools
                    .iter()
                    .any(|tool: &String| tool.eq_ignore_ascii_case("process:spawn"))
            })
            .unwrap_or(false)
    }

    async fn visible_synthesized_skills(
        &self,
        query: &str,
    ) -> MoatResult<Vec<SynthesizedSkill>> {
        if !self.agent_allows_process_spawn() {
            return Ok(Vec::new());
        }
        let db = self.db.lock().await;
        crate::memory::skills::select_relevant_synthesized_skills(&db, query)
    }

    async fn synthesized_skill_for_action(
        &self,
        action: &str,
    ) -> MoatResult<Option<SynthesizedSkill>> {
        if !self.agent_allows_process_spawn() {
            return Ok(None);
        }
        let db = self.db.lock().await;
        get_skill(&db, action)
    }

    async fn enforce_agent_tool_policy(&self, intent: &ActionIntent) -> Option<String> {
        let spec = self.spec.as_ref()?;
        if intent.action.eq_ignore_ascii_case("done") {
            return None;
        }
        if spec.allowed_tools.is_empty() {
            return Some(format!(
                "Action '{}' is not allowed: allowed_tools is empty for swarm agent '{}'.",
                intent.action, spec.name
            ));
        }
        if spec
            .allowed_tools
            .iter()
            .any(|tool: &String| tool.eq_ignore_ascii_case(&intent.action))
        {
            return None;
        }
        if self.agent_allows_process_spawn() {
            match self.synthesized_skill_for_action(&intent.action).await {
                Ok(Some(_)) => return None,
                Ok(None) => {}
                Err(err) => {
                    return Some(format!(
                        "Action '{}' could not be validated against synthesized skills: {}",
                        intent.action, err
                    ))
                }
            }
        }
        Some(format!(
            "Action '{}' is not allowed for swarm agent '{}'. Allowed tools: {}",
            intent.action,
            spec.name,
            spec.allowed_tools.join(", ")
        ))
    }

    fn missing_key_user_message(agent_name: &str, provider: &str) -> String {
        format!(
            "Agent '{agent_name}' requires a valid {} API key. Please click 'Settings' at the top of the window to configure it.",
            provider_display_name(provider)
        )
    }

    fn missing_key_message_from_error(
        &self,
        agent_name: &str,
        error: &MoatError,
    ) -> Option<String> {
        let provider = missing_key_provider_from_error(error)?;
        Some(Self::missing_key_user_message(agent_name, &provider))
    }

    fn format_choice_prompt(parsed: &AskChoicePayload) -> String {
        let mut lines = vec![parsed.context.trim().to_string()];
        for (index, option) in parsed.options.iter().enumerate() {
            lines.push(format!("{}. {}", index + 1, option.trim()));
        }
        lines.join("\n")
    }

    pub(crate) fn ask_choice_telemetry(parsed: &AskChoicePayload, request_id: &str) -> String {
        json!({
            "type": "ask_choice",
            "request_id": request_id,
            "context": clean_consent_summary(parsed.context.trim()),
            "options": parsed
                .options
                .iter()
                .map(|option| option.trim())
                .collect::<Vec<_>>(),
        })
        .to_string()
    }

    pub(crate) fn parse_ask_choice_payload(
        payload: Option<&Value>,
    ) -> Result<AskChoicePayload, String> {
        let payload = payload.ok_or_else(|| {
            "user:ask_choice requires payload {\"context\":\"...\",\"options\":[...]}"
                .to_string()
        })?;
        let parsed: AskChoicePayload = serde_json::from_value(payload.clone())
            .map_err(|err| format!("user:ask_choice payload is invalid JSON object: {err}"))?;
        if parsed.context.trim().is_empty() {
            return Err("user:ask_choice context cannot be empty".into());
        }
        if parsed.options.is_empty() {
            return Err("user:ask_choice options cannot be empty".into());
        }
        for (index, option) in parsed.options.iter().enumerate() {
            let option = option.trim();
            if option.is_empty() {
                return Err(format!("user:ask_choice option {} is empty", index + 1));
            }
        }
        Ok(parsed)
    }

    async fn search_session_transcripts(
        &self,
        query: &str,
        limit: usize,
    ) -> MoatResult<String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok("session:search requires a non-empty query.".to_string());
        }

        let fts_query = format!("\"{}\"", trimmed.replace('"', " "));
        let db = self.db.lock().await;
        let mut stmt = db
            .prepare(
                "SELECT
                    gs.source,
                    gs.channel,
                    gs.title,
                    snippet(gateway_messages_fts, 0, '[', ']', ' ... ', 18),
                    gm.created_at
                 FROM gateway_messages_fts
                 JOIN gateway_messages gm ON gm.id = gateway_messages_fts.rowid
                 JOIN gateway_sessions gs ON gs.session_id = gm.session_id
                 WHERE gateway_messages_fts MATCH ?1
                 ORDER BY bm25(gateway_messages_fts), gm.created_at DESC
                 LIMIT ?2",
            )
            ?;
        let rows = stmt
            .query_map(rusqlite::params![fts_query, limit.max(1) as i64], |row| {
                let source: String = row.get(0)?;
                let channel: String = row.get(1)?;
                let title: String = row.get(2)?;
                let snippet: String = row.get(3)?;
                let timestamp: i64 = row.get(4)?;
                Ok((source, channel, title, snippet, timestamp))
            })
            ?;

        let mut lines = Vec::new();
        for row in rows {
            let (source, channel, title, snippet, timestamp) =
                row?;
            let channel_label = if channel.trim().is_empty() {
                "default".to_string()
            } else {
                channel
            };
            lines.push(format!(
                "[{}:{} @ {}] {}\n{}",
                source.trim(),
                channel_label.trim(),
                timestamp,
                title.trim(),
                snippet.trim()
            ));
        }

        if lines.is_empty() {
            Ok("No relevant session transcripts found.".to_string())
        } else {
            Ok(lines.join("\n---\n"))
        }
    }

    #[allow(dead_code)]
    async fn log_kernel_crash(&self, query: &str, error: &str) -> MoatResult<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let memory_text = format!(
            "[KERNEL ERROR] query=\"{}\" error=\"{}\"",
            query
                .chars()
                .take(256)
                .collect::<String>()
                .replace('"', "'"),
            error.replace('"', "'")
        );

        let _id = {
            let db = self.db.lock().await;
            memory::store_knowledge(
                &db,
                memory::KnowledgeKind::Fact,
                "kernel",
                Some("Kernel pipeline error"),
                &memory_text,
                Some(timestamp),
            )?
        };

        Ok(())
    }

    async fn remember_skill(&self, payload: Option<&Value>) -> MoatResult<String> {
        let payload = payload.ok_or_else(|| {
            MoatError::Orchestrator(
                "memory:learn requires payload {\"keywords\":\"...\",\"skill_description\":\"...\"}"
                    .into(),
            )
        })?;
        let parsed: LearnPayload = serde_json::from_value(payload.clone()).map_err(|err| {
            MoatError::Orchestrator(format!(
                "memory:learn payload is invalid JSON object: {err}"
            ))
        })?;

        let keywords = parsed.keywords.trim();
        let skill_description = parsed.skill_description.trim();
        if keywords.is_empty() || skill_description.is_empty() {
            return Err(MoatError::Orchestrator(
                "memory:learn requires non-empty keywords and skill_description".into(),
            ));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let skill_block = format!(
            "[LEARNED SKILL]\nKeywords: {}\nSkill: {}",
            keywords, skill_description
        );

        let _id = format!("skill::{}::{}", timestamp, Uuid::new_v4());
        {
            let db = self.db.lock().await;
            memory::store_knowledge(
                &db,
                memory::KnowledgeKind::Fact,
                "memory:learn",
                Some(&format!("Skill: {}", keywords)),
                &skill_block,
                Some(timestamp),
            )?;
        }

        Ok(skill_block)
    }

    pub async fn run(
        &mut self,
        objective: &str,
        bootstrap: &str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
        continuation: Option<ReactContinuation>,
        _commitment_journal: Option<Arc<tokio::sync::Mutex<ChainJournal>>>,
        journal_path: Option<String>,
    ) -> MoatResult<ReactOutcome> {
        let session_id = format!("react-{}", Uuid::new_v4());

        let skills = self.visible_synthesized_skills(objective).await?;
        let instruction_skills = crate::memory::skills::select_instruction_skills(objective)?;

        // Temporary logging for one test run as requested:
        eprintln!("Instruction skill selected:");
        for skill in &instruction_skills {
            let mut score = 0;
            let query_lower = objective.to_lowercase();
            if skill
                .triggers
                .iter()
                .any(|t| query_lower.contains(&t.to_lowercase()))
            {
                score += 10;
            }
            if query_lower.contains(&skill.name.to_lowercase()) {
                score += 5;
            }
            if let Some(ref cat) = skill.category {
                if query_lower.contains(&cat.to_lowercase()) {
                    score += 2;
                }
            }
            eprintln!("{}", skill.name);
            eprintln!("score={}", score);
            eprintln!("\nDelegation candidates:");
            for cand in &skill.delegation {
                eprintln!("{}", cand);
            }
            eprintln!();
        }

        let agent_name = self
            .spec
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("default");
        let budget = crate::agent::context::budget::AgentBudgetProfile::for_agent(agent_name)
            .compute_budget();

        let mut restricted_skills = Vec::new();
        let mut total_skill_tokens = 0;
        for skill in skills {
            let skill_text = format!("{}\n{}", skill.skill_name, skill.description);
            let tokens = crate::agent::context::TokenBudget::count_tokens(&skill_text)
                .unwrap_or(skill_text.len() / 4) as u32;
            if total_skill_tokens + tokens <= budget.skill_tokens {
                total_skill_tokens += tokens;
                restricted_skills.push(skill);
            } else {
                break;
            }
        }

        let mut restricted_instruction_skills = Vec::new();
        for skill in instruction_skills {
            let skill_text = skill.prompt_summary();
            let tokens = crate::agent::context::TokenBudget::count_tokens(&skill_text)
                .unwrap_or(skill_text.len() / 4) as u32;
            if total_skill_tokens + tokens <= budget.skill_tokens {
                total_skill_tokens += tokens;
                restricted_instruction_skills.push(skill);
            } else if restricted_instruction_skills.is_empty() && total_skill_tokens == 0 {
                total_skill_tokens += tokens;
                restricted_instruction_skills.push(skill);
            } else {
                break;
            }
        }

        let cached_system_prompt = build_react_system_prompt(
            self.spec.as_ref(),
            &restricted_skills,
            &restricted_instruction_skills,
        );

        let mut initial_trace = if let (Some(ref path), None) = (&journal_path, &continuation) {
            match ContextRehydrator::rehydrate(path, &session_id) {
                Ok(rehydrated) if !rehydrated.is_empty() => {
                    let mut trace = vec![
                        LlmMessage {
                            role: "system".into(),
                            content: Some(cached_system_prompt.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "{}\n\n[SYSTEM: Session re-hydrated from chain journal. {} previous turns restored.]",
                                bootstrap, rehydrated.len()
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        }
                    ];
                    trace.extend(rehydrated);
                    trace
                }
                _ => {
                    vec![
                        LlmMessage {
                            role: "system".into(),
                            content: Some(cached_system_prompt.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                        LlmMessage {
                            role: "system".into(),
                            content: Some(bootstrap.to_string()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    ]
                }
            }
        } else {
            continuation
                .as_ref()
                .map(|saved| {
                    if saved.context_trace.is_empty() {
                        vec![
                            LlmMessage {
                                role: "system".into(),
                                content: Some(cached_system_prompt.clone()),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                            LlmMessage {
                                role: "system".into(),
                                content: Some(bootstrap.to_string()),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        ]
                    } else {
                        saved.context_trace.clone()
                    }
                })
                .unwrap_or_else(|| {
                    vec![
                        LlmMessage {
                            role: "system".into(),
                            content: Some(cached_system_prompt.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                        LlmMessage {
                            role: "system".into(),
                            content: Some(bootstrap.to_string()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    ]
                })
        };

        if continuation.is_none() {
            let prelude = run_active_memory_prelude(
                objective,
                None,
                Arc::clone(&self.db),
                status_tx.clone(),
                &budget,
            )
            .await;
            if !prelude.context_block.is_empty() {
                initial_trace.push(LlmMessage {
                    role: "system".into(),
                    content: Some(format!(
                        "<active_memory_prelude>\n{}\n</active_memory_prelude>",
                        prelude.context_block
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            initial_trace.push(LlmMessage {
                role: "user".into(),
                content: Some(objective.to_string()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let mut turns = continuation.as_ref().map(|saved| saved.turns).unwrap_or(0);
        let mut searches = continuation
            .as_ref()
            .map(|saved| saved.searches)
            .unwrap_or(0);
        let mut session_searches = continuation
            .as_ref()
            .map(|saved| saved.session_searches)
            .unwrap_or(0);
        let mut total_tokens = continuation
            .as_ref()
            .map(|saved| saved.total_tokens)
            .unwrap_or(0);
        let mut results = Vec::new();
        let mut loop_detector = LoopDetector::from_window(
            continuation
                .as_ref()
                .map(|saved| saved.loop_window.clone())
                .unwrap_or_default(),
        );
        let max_turns = self.planner.config.max_subtasks.max(1);
        let active_session_id = self.session_id.unwrap_or_else(Uuid::new_v4);
        let high_risk_approvals = if let Some(ref budget) = self.high_risk_approvals {
            if budget.session_id == active_session_id {
                Arc::clone(budget)
            } else {
                Arc::new(RiskBudget {
                    session_id: active_session_id,
                    approvals: std::sync::atomic::AtomicUsize::new(
                        continuation
                            .as_ref()
                            .map(|saved| saved.high_risk_approvals)
                            .unwrap_or(0),
                    ),
                })
            }
        } else {
            Arc::new(RiskBudget {
                session_id: active_session_id,
                approvals: std::sync::atomic::AtomicUsize::new(
                    continuation
                        .as_ref()
                        .map(|saved| saved.high_risk_approvals)
                        .unwrap_or(0),
                ),
            })
        };
        let mut consecutive_failures: usize = 0;
        let mut denial_strikes: usize = 0;
        let agent_label = self
            .spec
            .as_ref()
            .map(|agent| agent.name.clone())
            .unwrap_or_else(|| "primary".to_string());

        // Swarm delegation is disabled in Single-Agent mode
        let injected_observation: Option<String> = None;
        let execution_world_state = self.execution_world_state.clone();
        let execution_world_state = if execution_world_state.is_none() {
            Some(Arc::new(tokio::sync::RwLock::new(
                crate::agent::system::ExecutionWorldState {
                    denied_capabilities: Vec::new(),
                    discovered_facts: std::collections::HashMap::new(),
                    completed_subtasks: Vec::new(),
                    live_artifacts: std::collections::HashMap::new(),
                },
            )))
        } else {
            execution_world_state
        };

        let context_trace: SharedContext =
            Arc::new(AsyncRwLock::new(VersionedContext::new(initial_trace)));

        if let (Some(saved), Some(observation)) = (&continuation, injected_observation.as_ref())
        {
            push_context(
                &context_trace,
                LlmMessage {
                    role: "system".into(),
                    content: Some(format!(
                        "Turn {} INTENT: {} | TARGET: {}\nOBSERVATION: {}",
                        saved.turns,
                        saved.pending_tool_call.action,
                        saved.pending_tool_call.target,
                        observation
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                },
            )
            .await;
        }

        if let Some(tx) = &status_tx {
            let banner = if continuation.is_some() {
                "🔄 ReAct Loop Resumed. Generating next intent..."
            } else {
                "🔄 ReAct Loop Initialized. Generating first intent..."
            };
            let _ = tx.send(banner.into()).await;
        }

        let mut cached_tools = self.planner.build_llm_tools();

        if let Some(ref spec) = self.spec {
            if !spec.allowed_tools.is_empty() {
                cached_tools.retain(|t| spec.allowed_tools.contains(&t.function.name));
            }
        }

        loop {
            if self.cancel_flag.load(Ordering::Relaxed) {
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send("__TOOL_STATUS__:⏹️ Cancelled by user".to_string())
                        .await;
                }
                return Ok(ReactOutcome::Completed {
                    results,
                    context_snapshot: snapshot_context(&context_trace).await,
                });
            }
            turns += 1;

            if turns > max_turns {
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!(
                            "CRITICAL: {}-turn limit exceeded. Terminating loop.",
                            max_turns
                        ))
                        .await;
                }
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: "system:limit".into(),
                    target: "react".into(),
                    status: TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!("Max turns ({}) exceeded.", max_turns),
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: 0,
                });
                break;
            }

            const PROACTIVE_THRESHOLD: u32 = 115_200;
            const HARD_TOKEN_LIMIT: u32 = 128_000;

            if total_tokens > HARD_TOKEN_LIMIT {
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send("❌ CRITICAL: 128,000 Token Wall Hit. Terminating loop.".into())
                        .await;
                }
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: "system:limit".into(),
                    target: "react".into(),
                    status: TaskStatus::Failed {
                        exit_code: None,
                        stderr: "Token budget (128k) exhausted.".into(),
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: 0,
                });
                break;
            }

            if total_tokens > PROACTIVE_THRESHOLD {
                prune_context_if_needed(&context_trace, total_tokens as usize).await;
                total_tokens =
                    Self::count_messages_tokens(&snapshot_context(&context_trace).await) as u32;
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!(
                            "📦 Proactive compression: {} tokens used ({}% of budget). Pruning oldest turns.",
                            total_tokens,
                            (total_tokens * 100) / HARD_TOKEN_LIMIT
                        ))
                        .await;
                }
                if total_tokens > PROACTIVE_THRESHOLD {
                    if let Some(tx) = &status_tx {
                        let _ = tx
                            .send("❌ CRITICAL: Token budget exceeded safety margin even after pruning. Terminating loop.".to_string())
                            .await;
                    }
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: "system:limit".into(),
                        target: "react".into(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: format!(
                                "Token limit (128k) safety margin violated: {} tokens.",
                                total_tokens
                            ),
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: 0,
                    });
                    break;
                }
            } else if context_len(&context_trace).await > 8 {
                prune_context_if_needed(&context_trace, total_tokens as usize).await;
                total_tokens =
                    Self::count_messages_tokens(&snapshot_context(&context_trace).await) as u32;
            }

            let messages = snapshot_context(&context_trace).await;
            let llm_tools = Some(cached_tools.clone());

            if let Some(tx) = &status_tx {
                let _ = tx.send("__TOOL_STATUS__:🤔 Thinking...".to_string()).await;
            }

            let (tool_intents, first_raw, tokens) = match self
                .planner
                .generate_intent_tools(messages, llm_tools)
                .await
            {
                Ok(mut resp) => {
                    let mut tool_intents: Vec<ActionIntent> = Vec::new();

                    if let Some(tool_calls) = resp.tool_calls.take() {
                        for t in tool_calls {
                            let action = t.function.name;
                            let mut target = "user".to_string();
                            let mut payload = serde_json::Value::Null;
                            let mut reason = None;
                            if let Ok(args) =
                                serde_json::from_str::<serde_json::Value>(&t.function.arguments)
                            {
                                if let Some(tgt) = args.get("target").and_then(|v| v.as_str()) {
                                    target = tgt.to_string();
                                }
                                if let Some(pld) = args.get("payload") {
                                    payload = pld.clone();
                                }
                                if let Some(r) = args.get("reason").and_then(|v| v.as_str()) {
                                    reason = Some(r.to_string());
                                }
                            }
                            tool_intents.push(ActionIntent {
                                action,
                                target,
                                payload: Some(payload),
                                thought: None,
                                reason,
                            });
                        }
                    }

                    if tool_intents.is_empty() {
                        if let Some(content) = &resp.content {
                            let trimmed = content.trim();
                            if !trimmed.is_empty() {
                                let is_truncated = trimmed.len() < 10
                                    || (trimmed.starts_with('{') && !trimmed.ends_with('}'));
                                if is_truncated {
                                    let observation = format!(
                                        "[SYSTEM: Response appears truncated ({} chars). The model may have hit its max_tokens limit. Try simplifying your request or breaking it into steps.]",
                                        trimmed.len()
                                    );
                                    push_context(
                                        &context_trace,
                                        LlmMessage {
                                            role: "system".into(),
                                            content: Some(format!(
                                                "Turn {} OBSERVATION: {}",
                                                turns, observation
                                            )),
                                            tool_calls: None,
                                            tool_call_id: None,
                                        },
                                    )
                                    .await;
                                    results.push(SubTaskResult {
                                        subtask_id: turns,
                                        action: "system:truncated".into(),
                                        target: "planner".into(),
                                        status: TaskStatus::Failed {
                                            exit_code: None,
                                            stderr: observation,
                                        },
                                        elapsed: Duration::ZERO,
                                        tokens_used: resp.tokens,
                                    });
                                    total_tokens += resp.tokens;
                                    continue;
                                }
                                results.push(SubTaskResult {
                                    subtask_id: turns,
                                    action: "done".into(),
                                    target: "user".into(),
                                    status: TaskStatus::Success {
                                        stdout: trimmed.to_string(),
                                    },
                                    elapsed: Duration::ZERO,
                                    tokens_used: resp.tokens,
                                });
                                break;
                            }
                        }
                        let observation = "[SYSTEM: No action determined. Provide a concrete action or finish with state 'completed'.]";
                        push_context(
                            &context_trace,
                            LlmMessage {
                                role: "system".into(),
                                content: Some(format!(
                                    "Turn {} OBSERVATION: {}",
                                    turns, observation
                                )),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        )
                        .await;
                        continue;
                    }

                    let first = tool_intents
                        .first()
                        .cloned()
                        .expect("tool_intents non-empty");
                    let first_raw_str = serde_json::to_string(&serde_json::json!({
                        "action": first.action,
                        "target": first.target,
                        "payload": first.payload
                    }))
                    .unwrap_or_default();

                    (tool_intents, first_raw_str, resp.tokens)
                }
                Err(err) => {
                    if let Some(message) = self.missing_key_message_from_error(
                        self.spec
                            .as_ref()
                            .map(|agent| agent.name.as_str())
                            .unwrap_or("primary"),
                        &err,
                    ) {
                        if let Some(tx) = &status_tx {
                            let _ = tx.send(format!("⚠️ {message}")).await;
                        }
                        results.push(SubTaskResult {
                            subtask_id: turns,
                            action: "done".into(),
                            target: "user".into(),
                            status: TaskStatus::Success { stdout: message },
                            elapsed: Duration::ZERO,
                            tokens_used: 0,
                        });
                        break;
                    }
                    return Err(err);
                }
            };
            total_tokens += tokens;

            let raw = first_raw;

            let first = tool_intents
                .first()
                .cloned()
                .expect("tool_intents non-empty");
            let intent = first.clone();

            if let Some(tx) = &status_tx {
                let monologue = raw.trim();
                if !monologue.is_empty() && !monologue.starts_with('{') {
                    let _ = tx.send(format!("__TOOL_STATUS__:🤖 {}", monologue)).await;
                }
            }

            if intent.action == "system:parse_error" || intent.action.is_empty() {
                let observation = "[SYSTEM: Your last response could not be parsed. You MUST respond with a valid <action> JSON block. Try again.]";
                push_context(
                    &context_trace,
                    LlmMessage {
                        role: "system".into(),
                        content: Some(format!(
                            "Turn {} INTENT: system:parse_error | TARGET: parser\nOBSERVATION: {}",
                            turns, observation
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                )
                .await;
                if let Some(tx) = &status_tx {
                    let _ = tx.send(observation.to_string()).await;
                }
                continue;
            }

            if let Some(err) = self.enforce_agent_tool_policy(&intent).await {
                let observation = format!(
                    "[SYSTEM: Tool '{}' blocked by agent policy: {}. Use 'done' or an allowed tool.]",
                    intent.action, err
                );
                push_context(
                    &context_trace,
                    LlmMessage {
                        role: "system".into(),
                        content: Some(format!(
                            "Turn {} INTENT: {} | TARGET: {}\nOBSERVATION: FAILED: {}",
                            turns, intent.action, intent.target, observation
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                )
                .await;
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status: TaskStatus::Failed {
                        exit_code: None,
                        stderr: observation,
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: tokens,
                });
                continue;
            }

            if let Some(ref state_lock) = execution_world_state {
                let state = state_lock.read().await;
                if state.is_capability_denied(&intent.action, &intent.target) {
                    let err = format!(
                        "CAPABILITY DENIED (cached): '{}' on '{}' was already denied earlier in this workflow run. \
                         Do NOT retry. Use 'done' to report the limitation or 'user:ask_choice' to request permission.",
                        intent.action, intent.target
                    );
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} INTENT: {} | TARGET: {}\nOBSERVATION: {}",
                                turns, intent.action, intent.target, err
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: err,
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                }
            }

            if intent.action == "done" {
                let display_text = value_to_string(intent.payload.as_ref());
                let is_truncated = display_text.len() < 10
                    || (display_text.starts_with('{') && !display_text.ends_with('}'))
                    || (display_text.starts_with('[') && !display_text.ends_with(']'));
                if is_truncated {
                    let observation = format!(
                        "[SYSTEM: Response appears truncated ({} chars). The model output may have been cut off. Please try your request again with simpler wording.]",
                        display_text.len()
                    );
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: "done".into(),
                        target: "user".into(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: observation.clone(),
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    break;
                }
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send("✅ Task marked complete or yielded by agent.".into())
                        .await;
                }
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status: TaskStatus::Success {
                        stdout: display_text,
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: tokens,
                });
                break;
            }

            if intent.action == "user:ask_choice" {
                if intent.target != "user" {
                    let err = "user:ask_choice target must be \"user\"".to_string();
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} ERROR: {}. Fix the action and continue.",
                                turns, err
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: err,
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                }

                let parsed = match Self::parse_ask_choice_payload(intent.payload.as_ref()) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        push_context(
                            &context_trace,
                            LlmMessage {
                                role: "system".into(),
                                content: Some(format!(
                                    "Turn {} ERROR: {}. Fix the payload and continue.",
                                    turns, err
                                )),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        )
                        .await;
                        results.push(SubTaskResult {
                            subtask_id: turns,
                            action: intent.action.clone(),
                            target: intent.target.clone(),
                            status: TaskStatus::Failed {
                                exit_code: None,
                                stderr: err,
                            },
                            elapsed: Duration::ZERO,
                            tokens_used: tokens,
                        });
                        continue;
                    }
                };

                let choice_request_id = uuid::Uuid::new_v4().to_string();

                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(Self::ask_choice_telemetry(&parsed, &choice_request_id))
                        .await;
                }

                if let Some(bridge) = &self.worker.consent_bridge {
                    if let Some(tx) = &status_tx {
                        let _ = tx.send("⏸️ Waiting for user choice...".to_string()).await;
                    }
                    let selection = bridge.request_user_choice(&choice_request_id).await;
                    let observation = if selection.is_empty() {
                        "[USER CHOICE]: (no selection — user dismissed)".to_string()
                    } else {
                        format!("[USER CHOICE]: \"{}\"", selection)
                    };
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} OBSERVATION: {}",
                                turns, observation
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Success {
                            stdout: observation,
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                } else {
                    let output = Self::format_choice_prompt(&parsed);
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Success { stdout: output },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    break;
                }
            }

            if intent.action == "memory:search" {
                searches += 1;
                let max_search = self.max_memory_searches;
                if searches > max_search {
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} OBSERVATION: memory:search budget ({}) exhausted. You must proceed with available context.",
                                turns, max_search
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    continue;
                }

                let query = value_to_string(intent.payload.as_ref());
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!("🔍 Searching Neural Memory for: '{}'", query))
                        .await;
                }

                let max_chars = std::env::var("MOAT_MEMORY_MAX_CHARS")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(600);

                let found = {
                    let db = self.db.lock().await;
                    let mut stmt = db
                        .prepare(
                            "SELECT title, text FROM knowledge_memory_fts 
                             WHERE knowledge_memory_fts MATCH ?1 
                             ORDER BY rank 
                             LIMIT 3",
                        )
                        ?;
                    let query_cleaned = query.replace('"', " ");
                    let fts_query = format!("\"{}\"", query_cleaned);

                    let mut hits = Vec::new();
                    if let Ok(rows) = stmt.query_map(rusqlite::params![fts_query], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    }) {
                        for (title, text) in rows.flatten() {
                            hits.push((title, text));
                        }
                    }

                    hits.into_iter()
                        .map(|(title, text)| {
                            let clipped: String = text.chars().take(max_chars).collect();
                            let clipped_text = if clipped.len() == text.len() {
                                clipped
                            } else {
                                format!("{clipped}…")
                            };
                            format!("[{}] {}", title, clipped_text)
                        })
                        .collect::<Vec<_>>()
                        .join("\n---\n")
                };
                let found = if found.is_empty() {
                    "No relevant memories found at current similarity threshold.".to_string()
                } else {
                    found
                };

                // Quarantine: wrap untrusted external memory in delimiters
                let quarantined =
                    format!("[EXTERNAL_MEMORY_START]\n{}\n[EXTERNAL_MEMORY_END]", found);

                push_context(
                    &context_trace,
                    LlmMessage {
                        role: "system".into(),
                        content: Some(format!(
                            "Turn {} INTENT: memory:search | QUERY: {}\nOBSERVATION:\n{}",
                            turns, query, quarantined
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                )
                .await;
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status: if found.starts_with("No relevant memories") {
                        TaskStatus::SoftDenial {
                            reason: found.clone(),
                        }
                    } else {
                        TaskStatus::Success {
                            stdout: format!("Searched memory for: {}", query),
                        }
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: tokens,
                });
                continue;
            }

            if intent.action == "session:search" {
                session_searches += 1;
                let max_ss = self.max_session_searches;
                if session_searches > max_ss {
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} OBSERVATION: session:search budget ({}) exhausted. Continue with the transcripts already retrieved.",
                                turns, max_ss
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: format!("session:search budget ({}) exhausted", max_ss),
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                }

                let query = value_to_string(intent.payload.as_ref());
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!("🧵 Searching session transcripts for: '{}'", query))
                        .await;
                }

                let found = self.search_session_transcripts(&query, 5).await?;
                push_context(
                    &context_trace,
                    LlmMessage {
                        role: "system".into(),
                        content: Some(format!(
                            "Turn {} INTENT: session:search | QUERY: {}\nOBSERVATION:\n{}",
                            turns, query, found
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                )
                .await;
                if found.starts_with("No relevant session transcripts") {
                    continue;
                }
                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status: TaskStatus::Success {
                        stdout: format!("Searched session transcripts for: {}", query),
                    },
                    elapsed: Duration::ZERO,
                    tokens_used: tokens,
                });
                continue;
            }

            if intent.action == "memory:learn" {
                if intent.target != "procedural" {
                    let err = "memory:learn target must be \"procedural\"".to_string();
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!(
                                "Turn {} ERROR: {}. Fix the action and continue.",
                                turns, err
                            )),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: err,
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                }
                // Injection guard: scan payload for system instruction delimiters
                let raw_payload = value_to_string(intent.payload.as_ref());
                let injection_markers = [
                    "<|SYSTEM_START|>",
                    "<|SYSTEM_END|>",
                    "<|IM_START|>",
                    "<|IM_END|>",
                    "<|ASSISTANT_START|>",
                    "<|ASSISTANT_END|>",
                    "[EXTERNAL_MEMORY_START]",
                    "[EXTERNAL_MEMORY_END]",
                    "<system>",
                    "</system>",
                ];
                let payload_lower = raw_payload.to_lowercase();
                if let Some(marker) = injection_markers
                    .iter()
                    .find(|m| payload_lower.contains(&m.to_lowercase()))
                {
                    let err = format!("memory:learn rejected: payload contains injection marker '{}'. OperatorAssistant data is sanitized.", marker);
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!("Turn {} DENIED: {}", turns, err)),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: err,
                        },
                        elapsed: Duration::ZERO,
                        tokens_used: tokens,
                    });
                    continue;
                }
                match self.remember_skill(intent.payload.as_ref()).await {
                    Ok(skill_block) => {
                        push_context(
                            &context_trace,
                            LlmMessage {
                                role: "system".into(),
                                content: Some(format!(
                                    "Turn {} INTENT: memory:learn | TARGET: procedural\nOBSERVATION: SUCCESS: {}",
                                    turns, skill_block
                                )),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        )
                        .await;
                        results.push(SubTaskResult {
                            subtask_id: turns,
                            action: intent.action.clone(),
                            target: intent.target.clone(),
                            status: TaskStatus::Success {
                                stdout: format!("Learned procedural skill.\n{}", skill_block),
                            },
                            elapsed: Duration::ZERO,
                            tokens_used: tokens,
                        });
                    }
                    Err(err) => {
                        let err = err.to_string();
                        push_context(
                            &context_trace,
                            LlmMessage {
                                role: "system".into(),
                                content: Some(format!(
                                    "Turn {} ERROR: {}. Fix the payload and continue.",
                                    turns, err
                                )),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        )
                        .await;
                        results.push(SubTaskResult {
                            subtask_id: turns,
                            action: intent.action.clone(),
                            target: intent.target.clone(),
                            status: TaskStatus::Failed {
                                exit_code: None,
                                stderr: err,
                            },
                            elapsed: Duration::ZERO,
                            tokens_used: tokens,
                        });
                    }
                }
                continue;
            }

            let start = std::time::Instant::now();

            let parallel_intents: Vec<&ActionIntent> = if tool_intents.len() > 1 {
                tool_intents
                    .iter()
                    .filter(|i| {
                        !matches!(
                            i.action.as_str(),
                            "done"
                                | "system:parse_error"
                                | "swarm:delegate"
                                | "system:spawn_worker"
                                | "user:ask_choice"
                                | "memory:search"
                                | "session:search"
                                | "memory:learn"
                                | "meta:query_brain"
                        )
                    })
                    .collect()
            } else {
                vec![]
            };

            if parallel_intents.len() > 1 {
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!(
                            "__TOOL_STATUS__:⚡ Running {} tools in parallel",
                            parallel_intents.len()
                        ))
                        .await;
                }

                let mut join_set = tokio::task::JoinSet::new();
                for i in &parallel_intents {
                    let intent = (*i).clone();
                    let worker = Arc::clone(&self.worker);
                    let session_id = session_id.clone();
                    let agent_label = agent_label.clone();
                    let approvals = Arc::clone(&high_risk_approvals);
                    let world = execution_world_state.clone();
                    join_set.spawn(async move {
                        let result = worker
                            .execute_capability_task(
                                &intent,
                                root_node_id,
                                &session_id,
                                &agent_label,
                                approvals,
                                world,
                            )
                            .await;
                        (intent.action.clone(), intent.target.clone(), result)
                    });
                }

                let mut parallel_results: Vec<(String, String, MoatResult<TaskStatus>)> =
                    vec![];
                while let Some(res) = join_set.join_next().await {
                    if let Ok(result) = res {
                        parallel_results.push(result);
                    }
                }

                let mut obs_lines = vec![format!(
                    "Turn {} — Parallel batch ({} tools):",
                    turns,
                    parallel_intents.len()
                )];
                let mut all_success = true;
                let mut any_yielded: Option<(String, String)> = None;
                for (i, (action, target, result)) in parallel_results.iter().enumerate() {
                    match result {
                        Ok(status) => {
                            if matches!(status, TaskStatus::YieldedForConsent { .. }) {
                                any_yielded = Some((action.clone(), target.clone()));
                            }
                            let status_str = match status {
                                TaskStatus::Success { stdout } => {
                                    format!("SUCCESS: {}", stdout)
                                }
                                TaskStatus::SoftDenial { reason } => {
                                    format!("SOFT DENIAL: {}", reason)
                                }
                                TaskStatus::Failed { stderr, .. } => {
                                    format!("FAILED: {}", stderr)
                                }
                                TaskStatus::Blocked { .. } => "BLOCKED".into(),
                                TaskStatus::Cancelled { stage } => {
                                    format!("CANCELLED: {}", stage)
                                }
                                TaskStatus::YieldedForConsent { request_id } => {
                                    format!("YIELDED_FOR_CONSENT: {}", request_id)
                                }
                            };
                            obs_lines.push(format!(
                                "  [{}] {} → {}: {}",
                                i + 1,
                                action,
                                target,
                                status_str
                            ));
                            results.push(SubTaskResult {
                                subtask_id: turns,
                                action: action.clone(),
                                target: target.clone(),
                                status: status.clone(),
                                elapsed: start.elapsed(),
                                tokens_used: tokens,
                            });
                        }
                        Err(e) => {
                            obs_lines.push(format!(
                                "  [{}] {} → {}: ERROR {}",
                                i + 1,
                                action,
                                target,
                                e
                            ));
                            all_success = false;
                        }
                    }
                }

                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!(
                            "__TOOL_STATUS__:{} Parallel batch completed ({})",
                            if all_success { "✅" } else { "❌" },
                            parallel_intents.len()
                        ))
                        .await;
                }

                let observation = obs_lines.join("\n");
                push_context(
                    &context_trace,
                    LlmMessage {
                        role: "system".into(),
                        content: Some(observation),
                        tool_calls: None,
                        tool_call_id: None,
                    },
                )
                .await;

                if let Some((yielded_action, yielded_target)) = any_yielded {
                    let continuation = ReactContinuation {
                        context_trace: snapshot_context(&context_trace).await,
                        turns,
                        searches,
                        session_searches,
                        total_tokens,
                        delegate_count: 0,
                        high_risk_approvals: high_risk_approvals
                            .approvals
                            .load(std::sync::atomic::Ordering::SeqCst),
                        loop_window: loop_detector.snapshot(),
                        pending_tool_call: ActionIntent {
                            reason: None,
                            thought: None,
                            action: yielded_action.clone(),
                            target: yielded_target.clone(),
                            payload: None,
                        },
                        delegation_stack: Vec::new(),
                    };
                    return Ok(ReactOutcome::Yielded(
                        crate::agent::interaction_orchestrator::YieldStatus::AwaitingConsent {
                            request_id: String::new(),
                            tool_name: yielded_action,
                            target: yielded_target,
                            continuation,
                        },
                    ));
                }

                continue;
            }

            if let Some(tx) = &status_tx {
                let target_str = if intent.target.is_empty() {
                    String::new()
                } else {
                    format!(" → {}", intent.target)
                };
                let _ = tx
                    .send(format!(
                        "__TOOL_STATUS__:🛠️ {}{}",
                        intent.action, target_str
                    ))
                    .await;
            }

            let status = self
                .worker
                .execute_capability_task(
                    &intent,
                    root_node_id,
                    &session_id,
                    &agent_label,
                    Arc::clone(&high_risk_approvals),
                    execution_world_state.clone(),
                )
                .await?;

            if let TaskStatus::YieldedForConsent { request_id } = &status {
                if let Some(tx) = &status_tx {
                    let _ = tx
                        .send(format!(
                            "__TOOL_STATUS__:⏸️ Yielded for consent ({})",
                            request_id
                        ))
                        .await;
                }

                let _snapshot = YieldSnapshot::capture(
                    &session_id,
                    DelErrorCode::YieldedForConsent {
                        request_id: request_id.clone(),
                        tool_name: intent.action.clone(),
                        target: intent.target.clone(),
                    },
                    &snapshot_context(&context_trace).await,
                    &results,
                    turns,
                    total_tokens,
                    loop_detector.snapshot(),
                    &agent_label,
                    objective,
                );

                results.push(SubTaskResult {
                    subtask_id: turns,
                    action: intent.action.clone(),
                    target: intent.target.clone(),
                    status: TaskStatus::YieldedForConsent {
                        request_id: request_id.clone(),
                    },
                    elapsed: start.elapsed(),
                    tokens_used: tokens,
                });

                let continuation = ReactContinuation {
                    context_trace: snapshot_context(&context_trace).await,
                    turns,
                    searches,
                    session_searches,
                    total_tokens,
                    delegate_count: 0,
                    high_risk_approvals: high_risk_approvals
                        .approvals
                        .load(std::sync::atomic::Ordering::SeqCst),
                    loop_window: loop_detector.snapshot(),
                    pending_tool_call: intent.clone(),
                    delegation_stack: Vec::new(),
                };

                return Ok(ReactOutcome::Yielded(
                    crate::agent::interaction_orchestrator::YieldStatus::AwaitingConsent {
                        request_id: request_id.clone(),
                        tool_name: intent.action.clone(),
                        target: intent.target.clone(),
                        continuation,
                    },
                ));
            }

            if let TaskStatus::Failed { stderr, .. } = &status {
                if stderr.contains("CAPABILITY DENIED")
                    || stderr.contains("PERMISSION DENIED")
                    || stderr.contains("not allowed for swarm agent")
                {
                    if let Some(ref state_lock) = execution_world_state {
                        let mut state = state_lock.write().await;
                        state.record_denial(&intent.action, &intent.target);
                    }

                    // Circuit breaker: after 2+ denials of same action+target, hard stop
                    let cascade_key = format!("{}:{}", intent.action, intent.target);
                    cascade_denial_increment(&cascade_key);
                    let cascade_count = cascade_denial_count(&cascade_key);
                    if cascade_count >= 2 {
                        let observation = format!(
                            "[SYSTEM: Action '{}' was denied {} time(s). You MUST stop. Use 'done' or a different approach.]",
                            intent.action, cascade_count
                        );
                        push_context(
                            &context_trace,
                            LlmMessage {
                                role: "system".into(),
                                content: Some(observation.clone()),
                                tool_calls: None,
                                tool_call_id: None,
                            },
                        )
                        .await;
                        results.push(SubTaskResult {
                            subtask_id: turns,
                            action: intent.action.clone(),
                            target: intent.target.clone(),
                            status: TaskStatus::Failed {
                                exit_code: None,
                                stderr: observation.clone(),
                            },
                            tokens_used: 0,
                            elapsed: Duration::ZERO,
                        });
                        continue;
                    }
                    // Inject denial as observation — no forced user dialog
                    let observation = format!(
                        "[SYSTEM: Action '{}' denied: {}. Do NOT retry. Use 'done' or a different approach.]",
                        intent.action, stderr
                    );
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(observation.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: observation,
                        },
                        tokens_used: tokens,
                        elapsed: start.elapsed(),
                    });
                    continue;
                }
            }

            let status_str = match &status {
                TaskStatus::Success { stdout } => {
                    format!("SUCCESS: {}", stdout)
                }
                TaskStatus::SoftDenial { reason } => format!(
                    "SOFT DENIAL: {}. Do NOT retry this action with synonyms or rephrased queries. \
                     Accept this result and proceed with 'done' or an alternative approach.",
                    reason
                ),
                TaskStatus::Failed { stderr, .. } => {
                    format!("FAILED: {}", stderr)
                }
                TaskStatus::Blocked { .. } => "BLOCKED".into(),
                TaskStatus::Cancelled { stage } => {
                    format!("CANCELLED at: {}", stage)
                }
                TaskStatus::YieldedForConsent { request_id } => {
                    format!("YIELDED_FOR_CONSENT: {}", request_id)
                }
            };

            if let Some(tx) = &status_tx {
                match &status {
                    TaskStatus::Success { .. } => {
                        let _ = tx
                            .send(format!(
                                "__TOOL_STATUS__:✅ {} completed ({:.0}ms)",
                                intent.action,
                                start.elapsed().as_secs_f64() * 1000.0
                            ))
                            .await;
                    }
                    TaskStatus::Failed { stderr, .. } => {
                        let reason: String = stderr.chars().take(120).collect();
                        let _ = tx
                            .send(format!(
                                "__TOOL_STATUS__:❌ {} failed: {}",
                                intent.action, reason
                            ))
                            .await;
                    }
                    TaskStatus::SoftDenial { reason } => {
                        let _ = tx
                            .send(format!(
                                "__TOOL_STATUS__:⛔ {} denied: {}",
                                intent.action, reason
                            ))
                            .await;
                    }
                    _ => {}
                }
            }

            if let TaskStatus::SoftDenial { .. } = &status {
                if let Some(ref state_lock) = execution_world_state {
                    let mut state = state_lock.write().await;
                    state.record_denial(&intent.action, &intent.target);
                }
            }

            const MAX_OBSERVATION_CHARS: usize = 1500;
            let truncated_status = if status_str.len() > MAX_OBSERVATION_CHARS {
                let clipped: String = status_str.chars().take(MAX_OBSERVATION_CHARS).collect();
                format!(
                    "{}… [truncated {} chars]",
                    clipped,
                    status_str.len() - MAX_OBSERVATION_CHARS
                )
            } else {
                status_str.clone()
            };
            push_context(
                &context_trace,
                LlmMessage {
                    role: "system".into(),
                    content: Some(format!(
                        "Turn {} INTENT: {} | TARGET: {}\nOBSERVATION: {}",
                        turns, intent.action, intent.target, truncated_status
                    )),
                    tool_calls: None,
                    tool_call_id: None,
                },
            )
            .await;

            match loop_detector.analyze(LoopDetector::hash_execution(&intent, &status)) {
                LoopVerdict::Clear => {}
                LoopVerdict::SoftWarn { message } => {
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(format!("Turn {} LOOP WARNING: {}", turns, message)),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                }
                LoopVerdict::HardStop { message } => {
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: "done".into(),
                        target: "user".into(),
                        status: TaskStatus::Success { stdout: message },
                        elapsed: start.elapsed(),
                        tokens_used: tokens,
                    });
                    break;
                }
            }

            if matches!(&status, TaskStatus::Failed { .. }) {
                consecutive_failures += 1;
                if consecutive_failures >= 3 {
                    // Automatically trigger operator escalation for repeated execution failure
                    let db_clone = Arc::clone(&self.db);
                    let session_id_clone = session_id.clone();
                    tokio::spawn(async move {
                        let conn = db_clone.lock().await;
                        // Ensure the thread exists or resolves first, then escalate
                        let _ = crate::business::workflow::resolve_active_thread(
                            &conn,
                            "system_contact",
                        );
                        let _ = crate::business::escalation::escalate_thread(
                            &conn,
                            &session_id_clone,
                            "repeated_execution_failure",
                        );
                    });

                    let stop_msg = "[SYSTEM: 3 consecutive failures. You MUST emit 'done' now. Do NOT retry. Explain to the user what went wrong.]";
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(stop_msg.to_string()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    results.push(SubTaskResult {
                        subtask_id: turns,
                        action: intent.action.clone(),
                        target: intent.target.clone(),
                        status: TaskStatus::Failed {
                            exit_code: None,
                            stderr: "3 consecutive failures — stopping".into(),
                        },
                        elapsed: start.elapsed(),
                        tokens_used: tokens,
                    });
                    break;
                }
            } else if matches!(&status, TaskStatus::Success { .. }) {
                consecutive_failures = 0;
            }

            // ── 3-strike denial circuit breaker ──
            if matches!(&status, TaskStatus::Failed { .. })
                || matches!(&status, TaskStatus::SoftDenial { .. })
                || matches!(&status, TaskStatus::Blocked { .. })
            {
                denial_strikes += 1;
                if denial_strikes >= 3 {
                    let kill_msg = format!(
                        "[SYSTEM OVERRIDE: AGENT TERMINATED]\n\
                         You accumulated {} consecutive execution denials.\n\
                         The Execution Kernel has terminated this task.",
                        denial_strikes
                    );
                    push_context(
                        &context_trace,
                        LlmMessage {
                            role: "system".into(),
                            content: Some(kill_msg.clone()),
                            tool_calls: None,
                            tool_call_id: None,
                        },
                    )
                    .await;
                    return Ok(ReactOutcome::Terminated(kill_msg));
                }
            } else if matches!(&status, TaskStatus::Success { .. }) {
                denial_strikes = 0;
            }

            results.push(SubTaskResult {
                subtask_id: turns,
                action: intent.action.clone(),
                target: intent.target.clone(),
                status,
                elapsed: start.elapsed(),
                tokens_used: tokens,
            });
        }

        Ok(ReactOutcome::Completed {
            results,
            context_snapshot: snapshot_context(&context_trace).await,
        })
    }

    fn count_messages_tokens(messages: &[LlmMessage]) -> usize {
        let mut total = 0;
        for msg in messages {
            if let Some(ref content) = msg.content {
                total += crate::agent::context::TokenBudget::count_tokens(content).unwrap_or(0);
            }
            if let Some(ref calls) = msg.tool_calls {
                for call in calls {
                    total +=
                        crate::agent::context::TokenBudget::count_tokens(&call.function.name)
                            .unwrap_or(0);
                    total += crate::agent::context::TokenBudget::count_tokens(
                        &call.function.arguments,
                    )
                    .unwrap_or(0);
                }
            }
        }
        total
    }
}
