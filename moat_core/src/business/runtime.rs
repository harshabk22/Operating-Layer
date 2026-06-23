use super::models::{OpportunityStatus, PipelineStage, ThreadControlMode};
use super::{identity, knowledge, workflow};
use crate::agent::interaction_orchestrator::Orchestrator;
use crate::agent::interaction_orchestrator::TaskStatus;
use crate::communication::types::MessageDirection;
use crate::communication::{ConnectorEvent, ConnectorRegistry, InteractionMessage};
use crate::MoatResult;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;
pub struct CustomerInteractionRuntime {
    pub orchestrator: Arc<Orchestrator>,
    pub db: Arc<Mutex<Connection>>,
    pub registry: Arc<ConnectorRegistry>,
    pub calendar: Arc<dyn crate::business::calendar::CalendarProvider>,
    pub spreadsheet: Arc<dyn crate::business::spreadsheet::SpreadsheetProvider>,
    pub notifier: Arc<dyn crate::business::owner::NotificationProvider>,
    pub lock_manager: Arc<crate::business::locking::LockManager>,
}

impl CustomerInteractionRuntime {
    #[cfg(test)]
    pub fn new(
        orchestrator: Arc<Orchestrator>,
        db: Arc<Mutex<Connection>>,
        registry: Arc<ConnectorRegistry>,
    ) -> Self {
        Self {
            orchestrator,
            db,
            registry,
            calendar: Arc::new(crate::business::calendar::MockCalendarProvider::new())
                as Arc<dyn crate::business::calendar::CalendarProvider>,
            spreadsheet: Arc::new(crate::business::spreadsheet::MockSpreadsheetProvider::new())
                as Arc<dyn crate::business::spreadsheet::SpreadsheetProvider>,
            notifier: Arc::new(crate::business::owner::MockNotificationProvider::new())
                as Arc<dyn crate::business::owner::NotificationProvider>,
            lock_manager: Arc::new(crate::business::locking::LockManager::new()),
        }
    }


    pub fn with_providers(
        orchestrator: Arc<Orchestrator>,
        db: Arc<Mutex<Connection>>,
        registry: Arc<ConnectorRegistry>,
        calendar: Arc<dyn crate::business::calendar::CalendarProvider>,
        spreadsheet: Arc<dyn crate::business::spreadsheet::SpreadsheetProvider>,
        notifier: Arc<dyn crate::business::owner::NotificationProvider>,
    ) -> Self {
        Self {
            orchestrator,
            db,
            registry,
            calendar,
            spreadsheet,
            notifier,
            lock_manager: Arc::new(crate::business::locking::LockManager::new()),
        }
    }
    // Handles an incoming message on a channel from a contact
    pub async fn handle_interaction(
        &self,
        channel: &str,
        external_id: &str,
        message_content: &str,
    ) -> MoatResult<String> {
        let channel_type = match channel {
            "whatsapp" => crate::communication::types::ChannelType::Whatsapp,
            "instagram" => crate::communication::types::ChannelType::Instagram,
            "messenger" => crate::communication::types::ChannelType::Messenger,
            "web" | "webchat" => crate::communication::types::ChannelType::Web,
            "voice" => crate::communication::types::ChannelType::Voice,
            _ => crate::communication::types::ChannelType::Unknown,
        };

        let thread_id = {
            let conn = self.db.lock().await;
            if let Ok(contact) = identity::resolve_contact(&conn, channel, external_id) {
                if let Ok(Some(thread)) =
                    super::db::get_active_thread_by_contact(&conn, &contact.id)
                {
                    thread.id
                } else {
                    uuid::Uuid::new_v4().to_string()
                }
            } else {
                uuid::Uuid::new_v4().to_string()
            }
        };

        let lock_key = format!("thread:{}", thread_id);
        let _lock_guard = match self.lock_manager.try_lock(&lock_key) {
            Some(g) => g,
            None => {
                return Err(crate::MoatError::Orchestrator(
                    "Concurrent request in flight for this thread".to_string(),
                ));
            }
        };

        let msg = InteractionMessage {
            id: uuid::Uuid::new_v4().to_string(),
            business_id: "default_business".to_string(),
            thread_id,
            customer_id: external_id.to_string(),
            channel: channel_type,
            direction: MessageDirection::Inbound,
            body: message_content.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let reply = self.handle_message(msg).await?;
        Ok(reply.body)
    }

    // Connect runtime flow:
    // MessageReceived -> Identity Resolution -> Conversation Retrieval -> Knowledge Query -> Opportunity Qualification -> Authorization Logic -> Escalation Logic -> Response Generation -> MessageSent
    pub async fn handle_message(
        &self,
        message: InteractionMessage,
    ) -> MoatResult<InteractionMessage> {
        let conn = self.db.lock().await;

        // 1. Detect Inbound Intent
        let intent = self.detect_intent(&message.body);

        // 2. Identity Resolution
        let contact =
            identity::resolve_contact(&conn, &message.channel.to_string(), &message.customer_id)?;

        // 3. Conversation Retrieval (load active thread)
        let is_new_thread = super::db::get_active_thread_by_contact(&conn, &contact.id)?.is_none();
        let thread = workflow::resolve_active_thread(&conn, &contact.id)?;

        // Populate resolved thread ID on the incoming message
        let mut message = message;
        message.thread_id = thread.id.clone();

        // 4. Dispatch MessageReceived event
        let msg_received_event = ConnectorEvent::MessageReceived(message.clone());
        crate::communication::db::persist_event(
            &conn,
            &msg_received_event,
            &message.channel.to_string(),
        )?;
        self.registry
            .dispatch_event(msg_received_event, &message.channel.to_string())
            .await?;

        // Dispatch ThreadOpened if it's new
        if is_new_thread {
            let thread_opened_event = ConnectorEvent::ThreadOpened {
                business_id: message.business_id.clone(),
                thread_id: thread.id.clone(),
                customer_id: contact.id.clone(),
                channel: message.channel,
            };
            crate::communication::db::persist_event(
                &conn,
                &thread_opened_event,
                &message.channel.to_string(),
            )?;
            self.registry
                .dispatch_event(thread_opened_event, &message.channel.to_string())
                .await?;
        }

        // 5. Track Thread Objective
        use super::models::{ThreadObjective, ThreadObjectiveRecord};
        let mut objective_record = match super::db::get_thread_objective(&conn, &thread.id)? {
            Some(rec) => rec,
            None => {
                let default_obj = match intent.as_str() {
                    "booking" => ThreadObjective::ScheduleConsultation,
                    "support" => ThreadObjective::SupportCustomer,
                    "refund" => ThreadObjective::RecoverOpportunity,
                    _ => ThreadObjective::QualifyLead,
                };
                let rec = ThreadObjectiveRecord {
                    thread_id: thread.id.clone(),
                    objective: default_obj,
                    status: "active".to_string(),
                    created_at: chrono::Utc::now().timestamp(),
                    updated_at: chrono::Utc::now().timestamp(),
                };
                super::db::set_thread_objective(&conn, &rec)?;
                rec
            }
        };

        // Evolve objective based on message intent/policy
        if intent == "booking" && objective_record.objective == ThreadObjective::QualifyLead {
            objective_record.objective = ThreadObjective::ScheduleConsultation;
            objective_record.updated_at = chrono::Utc::now().timestamp();
            super::db::set_thread_objective(&conn, &objective_record)?;
        }

        // 6. Log incoming message
        workflow::log_message(&conn, &thread.id, "contact", &message.body)?;

        crate::api::event_bus::broadcast_event(
            "new_message",
            serde_json::json!({
                "thread_id": thread.id,
                "sender": "contact",
                "content": message.body,
                "timestamp": chrono::Utc::now().timestamp()
            }),
        );

        // 7. Resolve & Score Opportunity
        let opp = match super::db::get_opportunity_by_thread(&conn, &thread.id)? {
            Some(o) => o,
            None => {
                let o = super::models::Opportunity {
                    id: uuid::Uuid::new_v4().to_string(),
                    contact_id: contact.id.clone(),
                    thread_id: thread.id.clone(),
                    status: OpportunityStatus::Discovered,
                    value: Some(100.0),
                    notes: None,
                    created_at: chrono::Utc::now().timestamp(),
                    updated_at: chrono::Utc::now().timestamp(),
                    pipeline_stage: PipelineStage::New,
                };
                super::db::insert_opportunity(&conn, &o)?;
                let _ = self.notifier.notify_new_opportunity(&o.id, &thread.id);
                o
            }
        };

        let opp_score = super::policy::OpportunityEvaluator::evaluate(&opp.id, &message.body);
        super::db::insert_or_update_opportunity_score(&conn, &opp_score)?;

        let mut opp = opp;
        opp.status = match opp_score.rating {
            super::models::OpportunityRating::Critical => OpportunityStatus::Qualified,
            super::models::OpportunityRating::Hot => OpportunityStatus::EngagementPending,
            super::models::OpportunityRating::Warm => OpportunityStatus::Qualified,
            super::models::OpportunityRating::Cold => OpportunityStatus::Discovered,
        };

        if intent == "close" || message.body.contains("no thanks") {
            opp.status = OpportunityStatus::Disqualified;
            objective_record.status = "failed".to_string();
            super::db::set_thread_objective(&conn, &objective_record)?;
        }
        opp.updated_at = chrono::Utc::now().timestamp();
        super::db::update_opportunity_status(&conn, &thread.id, opp.status)?;

        let target_stage = match opp.status {
            OpportunityStatus::Qualified => PipelineStage::Qualified,
            OpportunityStatus::EngagementPending => PipelineStage::ProposalSent,
            OpportunityStatus::Disqualified => PipelineStage::Lost,
            _ => PipelineStage::New,
        };
        super::db::transition_opportunity_stage(
            &conn,
            self.notifier.as_ref(),
            &opp.id,
            target_stage,
            "system",
            Some("Auto-transition based on opportunity scoring"),
        )?;

        // Check if paused or human mode
        if thread.is_paused || thread.control_mode == ThreadControlMode::Human {
            let outbound_msg = InteractionMessage::new(
                message.business_id.clone(),
                thread.id.clone(),
                contact.id.clone(),
                message.channel,
                MessageDirection::Outbound,
                "Muted: AI responses are paused or thread is in human takeover mode.".to_string(),
            );
            super::reports::generate_reports(&conn)?;
            return Ok(outbound_msg);
        }

        // 8. Query organizational knowledge store for context
        let docs = knowledge::query_knowledge_store(&conn, &message.body, 3)?;
        let knowledge_context = if docs.is_empty() {
            "No matching documents found in the knowledge store.".to_string()
        } else {
            docs.iter()
                .map(|doc| format!("Document: {}\nContent: {}", doc.title, doc.content))
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        // 9. Evaluate Business Policy & Engagement Decision
        use super::models::EngagementDecision;
        let decision = super::policy::BusinessPolicy::evaluate(
            &intent,
            &opp_score,
            &objective_record.objective,
        );

        let mut response_text = String::new();
        let mut should_run_llm = false;

        match decision {
            EngagementDecision::Escalate => {
                // Trigger Escalation in DB
                let esc = super::escalation::escalate_thread(&conn, &thread.id, &intent)?;

                // Add to Operator Queue
                let queue_item = super::models::OperatorQueueItem {
                    thread_id: thread.id.clone(),
                    customer_id: contact.id.clone(),
                    reason: format!(
                        "Policy Escalation (Intent: {}, Rating: {:?})",
                        intent, opp_score.rating
                    ),
                    priority: match opp_score.rating {
                        super::models::OpportunityRating::Critical => "critical".to_string(),
                        super::models::OpportunityRating::Hot => "high".to_string(),
                        super::models::OpportunityRating::Warm => "medium".to_string(),
                        _ => "low".to_string(),
                    },
                    created_at: chrono::Utc::now().timestamp(),
                    customer_summary: Some(format!(
                        "Name: {}, Phone: {:?}",
                        contact.name.clone().unwrap_or_default(),
                        contact.phone
                    )),
                    thread_summary: Some(format!(
                        "Opportunity Status: {:?}",
                        thread.opportunity_status
                    )),
                };
                super::db::add_to_operator_queue(&conn, &queue_item)?;

                // Notify
                let _ = self
                    .notifier
                    .notify_new_escalation(&esc.id, &thread.id, &intent); // Dispatch Escalation Event
                let escalation_event = ConnectorEvent::EscalationRequested {
                    business_id: message.business_id.clone(),
                    thread_id: thread.id.clone(),
                    reason: intent.clone(),
                };
                crate::communication::db::persist_event(
                    &conn,
                    &escalation_event,
                    &message.channel.to_string(),
                )?;
                self.registry
                    .dispatch_event(escalation_event, &message.channel.to_string())
                    .await?;

                response_text =
                    "Your request has been escalated to an operator. We will be in touch shortly."
                        .to_string();
            }
            EngagementDecision::RequestAuthorization => {
                // Propose booking engagement
                let engagement = super::workflow::propose_engagement(
                    &conn,
                    &contact.id,
                    &thread.id,
                    "Scheduled Consultation Slot",
                    chrono::Utc::now().timestamp() + 86400,
                    chrono::Utc::now().timestamp() + 86400 + 3600,
                )?; // Request operator approval
                let auth_req = crate::business::authorization::AuthorizationQueue::create_request(
                    &conn,
                    &engagement.id,
                    "system",
                )?;

                let _ = crate::business::db::transition_opportunity_stage(
                    &conn,
                    self.notifier.as_ref(),
                    &opp.id,
                    PipelineStage::AwaitingApproval,
                    "system",
                    Some("Proposed booking proposal awaiting operator approval"),
                );

                // Notify
                let _ = self
                    .notifier
                    .notify_new_authorization(&auth_req.id, &thread.id);
                // Dispatch AuthorizationRequested
                let auth_event = ConnectorEvent::AuthorizationRequested {
                    business_id: message.business_id.clone(),
                    thread_id: thread.id.clone(),
                    request_id: auth_req.id.clone(),
                    action: "engagement:request_authorization".to_string(),
                    payload: serde_json::to_string(&auth_req).unwrap_or_default(),
                };
                crate::communication::db::persist_event(
                    &conn,
                    &auth_event,
                    &message.channel.to_string(),
                )?;
                self.registry
                    .dispatch_event(auth_event, &message.channel.to_string())
                    .await?;

                response_text = "A booking proposal has been submitted and is pending operator approval. We will notify you once confirmed.".to_string();
            }
            EngagementDecision::ProposeEngagement => {
                // Propose consulting engagement (auto-accepted or proposed)
                super::workflow::propose_engagement(
                    &conn,
                    &contact.id,
                    &thread.id,
                    "Proposed Consultation",
                    chrono::Utc::now().timestamp() + 86400,
                    chrono::Utc::now().timestamp() + 86400 + 3600,
                )?;

                response_text = "I have proposed a consultation slot for you. Please let us know if this works.".to_string();
            }
            EngagementDecision::CloseThread => {
                super::workflow::update_thread_status(
                    &conn,
                    &thread.id,
                    super::models::ThreadStatus::Closed,
                )?;

                // Dispatch ThreadClosed
                let close_event = ConnectorEvent::ThreadClosed {
                    business_id: message.business_id.clone(),
                    thread_id: thread.id.clone(),
                    channel: message.channel,
                };
                crate::communication::db::persist_event(
                    &conn,
                    &close_event,
                    &message.channel.to_string(),
                )?;
                self.registry
                    .dispatch_event(close_event, &message.channel.to_string())
                    .await?;

                response_text =
                    "Thank you. This conversation is now closed. Have a great day!".to_string();
            }
            EngagementDecision::AnswerOnly => {
                should_run_llm = true;
            }
        }

        // We release the DB lock here to run the receptionist / orchestrator loop if needed
        drop(conn);

        if should_run_llm {
            // Run Receptionist Loop
            let prompt_input = format!(
                "Interaction context for Contact ID: {}\nThread ID: {}\nChannel: {}\n\nIncoming message: {}\n\nKnowledge Store context:\n{}",
                contact.id, thread.id, message.channel, message.body, knowledge_context
            );

            let (results, _) = self
                .orchestrator
                .run_react(
                    &prompt_input,
                    "",
                    crate::memory::meta_brain::SYSTEM_ROOT_ID,
                    None,
                )
                .await?;

            response_text =
                "Thank you for your message. An operator will be with you shortly.".to_string();
            for res in &results {
                if res.action == "done" {
                    if let TaskStatus::Success { stdout } = &res.status {
                        response_text = stdout.clone();
                    }
                }
            }
        }

        // Re-acquire lock to log response, generate reports, and dispatch
        let conn = self.db.lock().await;

        if thread.control_mode == ThreadControlMode::Hybrid {
            // Save as suggestion
            let suggestion = super::models::ResponseSuggestion {
                thread_id: thread.id.clone(),
                suggested_reply: response_text.clone(),
                confidence: 0.9,
                reasoning_summary: "AI receptionist response generated in hybrid control mode"
                    .to_string(),
            };
            super::db::insert_response_suggestion(&conn, &suggestion)?;

            let outbound_msg = InteractionMessage::new(
                message.business_id.clone(),
                thread.id.clone(),
                contact.id.clone(),
                message.channel,
                MessageDirection::Outbound,
                format!("Suggestion generated: {}", response_text),
            );

            // Regenerate reports dynamically
            super::reports::generate_reports(&conn)?;

            return Ok(outbound_msg);
        }

        // Log outgoing message in DB
        workflow::log_message(&conn, &thread.id, "business", &response_text)?;

        // 10. Route outgoing message via Outbox queue
        let outbound_msg = InteractionMessage::new(
            message.business_id.clone(),
            thread.id.clone(),
            contact.id.clone(),
            message.channel,
            MessageDirection::Outbound,
            response_text,
        );

        crate::business::outbox::enqueue_outbox(&conn, &outbound_msg)?;

        // Log response time in analytics
        let elapsed = outbound_msg.timestamp - message.timestamp;
        let _ = super::db::insert_analytics_event(
            &conn,
            "response_time",
            Some(&thread.id),
            elapsed as f64,
        );

        // 11. Regenerate reports dynamically
        super::reports::generate_reports(&conn)?;

        Ok(outbound_msg)
    }

    pub async fn approve_and_execute_booking(&self, request_id: &str) -> MoatResult<String> {
        let lock_key = format!("auth:{}", request_id);
        let _lock_guard = match self.lock_manager.try_lock(&lock_key) {
            Some(g) => g,
            None => {
                return Err(crate::MoatError::Orchestrator(
                    "Concurrent booking approval in flight".to_string(),
                ));
            }
        };

        let conn = self.db.lock().await;

        let req = crate::business::db::get_authorization_request(&conn, request_id)?.ok_or_else(
            || {
                crate::MoatError::Orchestrator(format!(
                    "Authorization request {} not found",
                    request_id
                ))
            },
        )?;

        if req.status != crate::business::models::AuthorizationStatus::Pending {
            return Err(crate::MoatError::Orchestrator(format!(
                "Cannot execute booking: request {} is not pending",
                request_id
            )));
        }

        conn.execute(
            "UPDATE business_authorization_requests SET status = 'approved' WHERE id = ?1",
            [request_id],
        )?;

        crate::business::db::update_thread_status(
            &conn,
            &req.thread_id,
            crate::business::models::ThreadStatus::Active,
        )?;
        crate::business::db::update_engagement_status(
            &conn,
            &req.engagement_id,
            crate::business::models::EngagementStatus::Authorized,
        )?;

        let event_payload = serde_json::json!({
            "request_id": request_id,
            "engagement_id": req.engagement_id,
            "thread_id": req.thread_id
        });
        crate::business::event_store::record_event(
            &conn,
            "authorization_approved",
            request_id,
            &event_payload,
        )?;

        let job_payload = serde_json::json!({
            "request_id": request_id
        });
        crate::business::jobs::queue::enqueue_job(&conn, "calendar_sync", job_payload, 5)?;

        let confirmation_msg =
            "Confirmed: booking request has been approved and queued for scheduling.".to_string();
        Ok(confirmation_msg)
    }

    pub async fn reject_authorization(&self, request_id: &str) -> MoatResult<()> {
        let lock_key = format!("auth:{}", request_id);
        let _lock_guard = match self.lock_manager.try_lock(&lock_key) {
            Some(g) => g,
            None => {
                return Err(crate::MoatError::Orchestrator(
                    "Concurrent booking decision in flight".to_string(),
                ));
            }
        };

        let conn = self.db.lock().await;
        crate::business::authorization::AuthorizationQueue::reject_request(&conn, request_id)?;

        let req = crate::business::db::get_authorization_request(&conn, request_id)?.ok_or_else(
            || {
                crate::MoatError::Orchestrator(format!(
                    "Authorization request {} not found",
                    request_id
                ))
            },
        )?;

        let event_payload = serde_json::json!({
            "request_id": request_id,
            "engagement_id": req.engagement_id,
            "thread_id": req.thread_id
        });
        crate::business::event_store::record_event(
            &conn,
            "authorization_rejected",
            request_id,
            &event_payload,
        )?;

        let elapsed = chrono::Utc::now().timestamp() - req.created_at;
        let _ = crate::business::db::insert_analytics_event(
            &conn,
            "authorization_time",
            Some(&req.thread_id),
            elapsed as f64,
        );

        if let Ok(Some(opp)) = crate::business::db::get_opportunity_by_thread(&conn, &req.thread_id)
        {
            let _ = crate::business::db::transition_opportunity_stage(
                &conn,
                self.notifier.as_ref(),
                &opp.id,
                PipelineStage::Lost,
                "system",
                Some("Booking rejected by operator"),
            );
        }

        let reject_msg = "Your booking request has been rejected by the operator.".to_string();
        crate::business::workflow::log_message(&conn, &req.thread_id, "business", &reject_msg)?;

        if let Ok((channel, contact_id)) =
            crate::communication::db::get_channel_and_contact_for_thread(&conn, &req.thread_id)
        {
            let outbound_msg = crate::communication::InteractionMessage::new(
                "default_business".to_string(),
                req.thread_id.clone(),
                contact_id,
                channel,
                crate::communication::types::MessageDirection::Outbound,
                reject_msg,
            );
            crate::business::outbox::enqueue_outbox(&conn, &outbound_msg)?;
        }

        Ok(())
    }

    fn detect_intent(&self, content: &str) -> String {
        let lower = content.to_lowercase();
        if lower.contains("refund") {
            "refund".to_string()
        } else if lower.contains("pricing")
            || lower.contains("price")
            || lower.contains("cost")
            || lower.contains("how much")
        {
            "pricing".to_string()
        } else if lower.contains("book")
            || lower.contains("schedule")
            || lower.contains("meeting")
            || lower.contains("appointment")
        {
            "booking".to_string()
        } else if lower.contains("support")
            || lower.contains("help")
            || lower.contains("broken")
            || lower.contains("issue")
        {
            "support".to_string()
        } else if lower.contains("close") || lower.contains("goodbye") || lower.contains("bye") {
            "close".to_string()
        } else if lower.contains("buy")
            || lower.contains("purchase")
            || lower.contains("interested")
        {
            "purchase".to_string()
        } else {
            "general".to_string()
        }
    }

    pub fn start_background_workers(self: &Arc<Self>) -> Vec<tokio::task::JoinHandle<()>> {
        let runtime_clone = self.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::business::recovery::run_recovery(&runtime_clone).await {
                eprintln!("[Recovery] Error on startup: {:?}", e);
            }
        });

        vec![
            crate::business::jobs::worker::start_job_worker(self.clone()),
            crate::business::outbox::start_outbox_worker(self.clone()),
        ]
    }

    pub fn start_webhook(self: Arc<Self>, addr: &str) -> tokio::task::JoinHandle<()> {
        self.start_background_workers();
        let verify_token = std::env::var("WHATSAPP_VERIFY_TOKEN")
            .unwrap_or_else(|_| "default_verify_token".to_string());
        let client_secret = std::env::var("WHATSAPP_CLIENT_SECRET")
            .unwrap_or_else(|_| "default_client_secret".to_string());
        let server = Arc::new(crate::communication::whatsapp::WhatsAppWebhookServer::new(
            self.clone(),
            self.db.clone(),
            verify_token,
            client_secret,
        ));
        server.start(addr.to_string())
    }
}
