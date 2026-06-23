use crate::business::db;
use crate::business::llm::{LlmMessage, LlmRole};
use crate::business::owner::NotificationProvider;
use crate::business::runtime::CustomerInteractionRuntime;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

pub struct OperationalSupervisor {
    runtime: Arc<CustomerInteractionRuntime>,
    notifier: Arc<dyn NotificationProvider>,
}

impl OperationalSupervisor {
    pub fn new(
        runtime: Arc<CustomerInteractionRuntime>,
        notifier: Arc<dyn NotificationProvider>,
    ) -> Self {
        Self { runtime, notifier }
    }

    pub fn start_worker(&self) -> tokio::task::JoinHandle<()> {
        let runtime = self.runtime.clone();
        let notifier = self.notifier.clone();

        tokio::spawn(async move {
            loop {
                if let Err(e) = Self::run_iteration(&runtime, &notifier).await {
                    eprintln!("[OperationalSupervisor] Error in iteration: {:?}", e);
                }
                sleep(Duration::from_millis(5000)).await;
            }
        })
    }

    async fn run_iteration(
        runtime: &Arc<CustomerInteractionRuntime>,
        notifier: &Arc<dyn NotificationProvider>,
    ) -> anyhow::Result<()> {
        let tasks = {
            let conn = runtime.db.lock().await;
            db::get_pending_operational_supervisor_tasks(&conn, 5)?
        };

        for task in tasks {
            // Mark as running
            {
                let conn = runtime.db.lock().await;
                let _ = db::update_operational_supervisor_task_status(&conn, &task.id, "running");
            }

            // LLM Classification and Analysis
            let system_prompt = "You are the Operational Supervisor for a business automation platform.
Your job is to read technical errors, categorize them, determine the severity, and provide a clear, non-technical explanation and recommended action for the business owner.
Do NOT attempt to execute fixes. You are a READ ONLY monitoring agent.
Output your response as valid JSON matching this schema:
{
    \"severity\": \"Critical\" | \"Warning\" | \"Info\",
    \"category\": \"System\" | \"Provider\" | \"AI\" | \"Logic\",
    \"summary\": \"Clear 1-sentence explanation of what failed\",
    \"recommended_action\": \"Clear 1-sentence instruction for the owner\"
}";
            let messages = vec![LlmMessage {
                role: LlmRole::User,
                content: format!("Please analyze this technical issue:\nTitle: {}\nComponent: {}\nRoot Cause: {}", task.title, task.affected_component, task.root_cause),
            }];

            let mut final_severity = task.severity.clone();
            let mut final_category = task.category.clone();
            let mut final_summary = task.summary.clone();
            let mut final_recommendation = task.recommended_action.clone();

            match runtime.llm.generate(system_prompt, &messages).await {
                Ok(response) => {
                    let text = response.content;
                    let json_str = if let Some(start) = text.find('{') {
                        if let Some(end) = text.rfind('}') {
                            &text[start..=end]
                        } else {
                            &text
                        }
                    } else {
                        &text
                    };

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(s) = parsed.get("severity").and_then(|v| v.as_str()) {
                            final_severity = s.to_string();
                        }
                        if let Some(c) = parsed.get("category").and_then(|v| v.as_str()) {
                            final_category = c.to_string();
                        }
                        if let Some(s) = parsed.get("summary").and_then(|v| v.as_str()) {
                            final_summary = s.to_string();
                        }
                        if let Some(r) = parsed.get("recommended_action").and_then(|v| v.as_str()) {
                            final_recommendation = r.to_string();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[OperationalSupervisor] LLM analysis failed: {:?}", e);
                }
            }

            // Update Task with LLM insights
            {
                let conn = runtime.db.lock().await;
                let _ = crate::business::event_store::record_event(
        conn,
        "StateMutated",
        "aggregate_id",
        &serde_json::json!({ "type": "LegacyMutationReplaced" })
    )?;
            }

            // Notify Owner
            let notification_msg = format!(
                "[{}] {} - {}\nRecommendation: {}",
                final_severity.to_uppercase(),
                task.title,
                final_summary,
                final_recommendation
            );

            let _ = notifier.notify_new_escalation(&task.id, "system", &notification_msg);
        }

        Ok(())
    }
}
