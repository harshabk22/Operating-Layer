use crate::business::outbox::enqueue_outbox;
use crate::business::runtime::CustomerInteractionRuntime;
use crate::communication::types::MessageDirection;
use crate::communication::InteractionMessage;
use crate::MoatResult;
use rusqlite::Connection;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TriggerContext {
    pub contact_id: String,
    pub thread_id: String,
    pub channel: String,
    pub message_body: String,
}

pub struct WorkflowEngine;

impl WorkflowEngine {
    pub fn run_workflows(
        runtime: &CustomerInteractionRuntime,
        conn: &Connection,
        trigger_type: &str,
        context: &TriggerContext,
    ) -> MoatResult<(Option<String>, Vec<crate::communication::ConnectorEvent>)> {
        let workflows = super::db::list_workflows_by_trigger(conn, trigger_type)?;
        let mut final_response: Option<String> = None;
        let mut events_to_dispatch = Vec::new();

        for wf in workflows {
            let conditions = super::db::list_workflow_conditions(conn, &wf.id)?;
            let mut all_match = true;

            let intent = runtime.detect_intent(&context.message_body);

            for cond in conditions {
                let val_to_check = match cond.field.as_str() {
                    "message_body" => &context.message_body,
                    "channel" => &context.channel,
                    "contact_id" => &context.contact_id,
                    "intent" => &intent,
                    _ => "",
                };
                let matched = match cond.operator.as_str() {
                    "equals" => val_to_check == cond.value,
                    "contains" => val_to_check.contains(&cond.value),
                    "not_contains" => !val_to_check.contains(&cond.value),
                    "starts_with" => val_to_check.starts_with(&cond.value),
                    "ends_with" => val_to_check.ends_with(&cond.value),
                    _ => false,
                };
                if !matched {
                    all_match = false;
                    break;
                }
            }

            if !all_match {
                continue;
            }

            let actions = super::db::list_workflow_actions(conn, &wf.id)?;
            for action in actions {
                match action.action_type.as_str() {
                    "send_message" => {
                        let payload_json: Value =
                            serde_json::from_str(&action.payload).unwrap_or_default();
                        let template = payload_json
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&action.payload);
                        let body = template
                            .replace("{contact_id}", &context.contact_id)
                            .replace("{thread_id}", &context.thread_id);

                        final_response = Some(body.clone());

                        let outbound_msg = InteractionMessage::new(
                            "default_business".to_string(),
                            context.thread_id.clone(),
                            context.contact_id.clone(),
                            match context.channel.as_str() {
                                "whatsapp" => crate::communication::types::ChannelType::Whatsapp,
                                "instagram" => crate::communication::types::ChannelType::Instagram,
                                "messenger" => crate::communication::types::ChannelType::Messenger,
                                "web" | "webchat" => crate::communication::types::ChannelType::Web,
                                "voice" => crate::communication::types::ChannelType::Voice,
                                _ => crate::communication::types::ChannelType::Unknown,
                            },
                            MessageDirection::Outbound,
                            body,
                        );
                        let _ = super::workflow::log_message(
                            conn,
                            &context.thread_id,
                            "business",
                            &outbound_msg.body,
                        )?;
                        let _ = enqueue_outbox(conn, &outbound_msg)?;
                    }
                    "create_booking" => {
                        let payload_json: Value =
                            serde_json::from_str(&action.payload).unwrap_or_default();
                        let title = payload_json
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Scheduled Consultation Slot");
                        let start_time = payload_json
                            .get("start_time")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_else(|| chrono::Utc::now().timestamp() + 86400);
                        let end_time = payload_json
                            .get("end_time")
                            .and_then(|v| v.as_i64())
                            .unwrap_or_else(|| start_time + 3600);

                        super::workflow::propose_engagement(
                            conn,
                            &context.contact_id,
                            &context.thread_id,
                            title,
                            start_time,
                            end_time,
                        )?;
                    }
                    "request_authorization" => {
                        let engagement_id = {
                            let mut stmt = conn.prepare("SELECT id FROM business_scheduled_engagements WHERE thread_id = ?1 AND status = 'Proposed' ORDER BY created_at DESC LIMIT 1")?;
                            let mut rows = stmt.query(rusqlite::params![context.thread_id])?;
                            if let Some(row) = rows.next()? {
                                row.get::<_, String>(0)?
                            } else {
                                let engagement = super::workflow::propose_engagement(
                                    conn,
                                    &context.contact_id,
                                    &context.thread_id,
                                    "Scheduled Consultation Slot",
                                    chrono::Utc::now().timestamp() + 86400,
                                    chrono::Utc::now().timestamp() + 86400 + 3600,
                                )?;
                                engagement.id
                            }
                        };

                        let auth_req =
                            crate::business::authorization::AuthorizationQueue::create_request(
                                conn,
                                &engagement_id,
                                "system",
                            )?;

                        if let Some(opp) = crate::business::db::get_opportunity_by_thread(
                            conn,
                            &context.thread_id,
                        )? {
                            let _ = crate::business::db::transition_opportunity_stage(
                                conn,
                                None,
                                &opp.id,
                                crate::business::models::PipelineStage::AwaitingApproval,
                                "system",
                                Some("Proposed booking proposal awaiting operator approval"),
                            );
                        }

                        let _ = runtime.capability_router.execute(
                            &runtime.business_id,
                            "SendNotification",
                            &serde_json::json!({
                                "message": format!("New authorization request: {}", auth_req.id),
                                "thread_id": context.thread_id
                            }),
                        );

                        let auth_event =
                            crate::communication::ConnectorEvent::AuthorizationRequested {
                                business_id: "default_business".to_string(),
                                thread_id: context.thread_id.clone(),
                                request_id: auth_req.id.clone(),
                                action: "engagement:request_authorization".to_string(),
                                payload: serde_json::to_string(&auth_req).unwrap_or_default(),
                            };
                        crate::communication::db::persist_event(
                            conn,
                            &auth_event,
                            &context.channel,
                        )?;
                        events_to_dispatch.push(auth_event);
                    }
                    "escalate" => {
                        let intent = runtime.detect_intent(&context.message_body);
                        let esc =
                            super::escalation::escalate_thread(conn, &context.thread_id, &intent)?;

                        let _ = super::db::insert_audit_event(
                            conn,
                            &super::models::AuditEvent {
                                id: uuid::Uuid::new_v4().to_string(),
                                business_id: runtime.business_id.clone(),
                                event_type: "escalation_created".to_string(),
                                actor: "workflow".to_string(),
                                payload: format!("esc_id={} intent={}", esc.id, intent),
                                timestamp: chrono::Utc::now().timestamp(),
                            },
                        );

                        let queue_item = super::models::OperatorQueueItem {
                            thread_id: context.thread_id.clone(),
                            customer_id: context.contact_id.clone(),
                            reason: format!("Policy Escalation (Intent: {})", intent),
                            priority: "high".to_string(),
                            created_at: chrono::Utc::now().timestamp(),
                            customer_summary: None,
                            thread_summary: None,
                        };
                        super::db::add_to_operator_queue(conn, &queue_item)?;

                        let _ = runtime.capability_router.execute(&runtime.business_id, "SendNotification", &serde_json::json!({
                            "message": format!("Escalation: thread={}, reason={}", context.thread_id, intent),
                            "thread_id": context.thread_id
                        }));

                        let escalation_event =
                            crate::communication::ConnectorEvent::EscalationRequested {
                                business_id: "default_business".to_string(),
                                thread_id: context.thread_id.clone(),
                                reason: intent.clone(),
                            };
                        crate::communication::db::persist_event(
                            conn,
                            &escalation_event,
                            &context.channel,
                        )?;
                        events_to_dispatch.push(escalation_event);
                    }
                    "update_spreadsheet" => {
                        let payload_json: Value =
                            serde_json::from_str(&action.payload).unwrap_or_default();
                        let sheet_name = payload_json
                            .get("sheet_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Logs");
                        let row_data: Vec<String> = payload_json
                            .get("row")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|item| item.as_str().unwrap_or("").to_string())
                                    .collect()
                            })
                            .unwrap_or_else(|| {
                                vec![
                                    context.contact_id.clone(),
                                    context.thread_id.clone(),
                                    context.message_body.clone(),
                                    chrono::Utc::now().to_rfc3339(),
                                ]
                            });
                        let _ = runtime.capability_router.execute(
                            &runtime.business_id,
                            "AppendRow",
                            &serde_json::json!({
                                "sheet": sheet_name,
                                "values": row_data
                            }),
                        );
                    }
                    "notify_owner" => {
                        let payload_json: Value =
                            serde_json::from_str(&action.payload).unwrap_or_default();
                        let message = payload_json
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("System notification");
                        println!(
                            "[Owner Notification] Thread ID: {}, Message: {}",
                            context.thread_id, message
                        );
                        let event_payload = serde_json::json!({
                            "thread_id": context.thread_id,
                            "message": message
                        });
                        crate::business::event_store::record_event(
                            conn,
                            "owner_notified",
                            &context.thread_id,
                            &event_payload,
                        )?;
                    }
                    _ => {}
                }
            }
        }

        Ok((final_response, events_to_dispatch))
    }
}
