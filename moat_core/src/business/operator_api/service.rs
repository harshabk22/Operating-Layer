use anyhow::{anyhow, Result};
use std::sync::Arc;
use uuid::Uuid;

use crate::business::db;
use crate::business::models::{
    AuthorizationRequest, BusinessAnalytics, InteractionThread, OperatorEscalation,
    OperatorQueueItem, Opportunity, OwnerAuditRecord, PipelineStage, ScheduledEngagement,
    ThreadControlMode,
};
use crate::business::owner::NotificationProvider;
use crate::business::runtime::CustomerInteractionRuntime;

pub struct OperatorService {
    runtime: Arc<CustomerInteractionRuntime>,
    notifier: Arc<dyn NotificationProvider>,
}

impl OperatorService {
    pub fn new(
        runtime: Arc<CustomerInteractionRuntime>,
        notifier: Arc<dyn NotificationProvider>,
    ) -> Self {
        Self { runtime, notifier }
    }

    pub async fn list_pending_authorizations(&self) -> Result<Vec<AuthorizationRequest>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_pending_authorization_requests(&conn)?;
        Ok(list)
    }

    pub async fn list_all_authorizations(&self) -> Result<Vec<crate::business::models::AuthorizationDetailResponse>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_all_authorization_details(&conn)?;
        Ok(list)
    }

    pub async fn list_pending_escalations(&self) -> Result<Vec<OperatorEscalation>> {
        let conn = self.runtime.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, reason, status, created_at, updated_at 
             FROM business_operator_escalations 
             WHERE status IN ('pending', 'reopened') 
             ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(OperatorEscalation {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                reason: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            });
        }
        Ok(items)
    }

    pub async fn list_active_threads(&self) -> Result<Vec<InteractionThread>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_active_threads(&conn)?;
        Ok(list)
    }

    pub async fn list_bookings(&self) -> Result<Vec<ScheduledEngagement>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_recent_bookings(&conn)?;
        Ok(list)
    }

    pub async fn list_opportunities(&self) -> Result<Vec<Opportunity>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_opportunities(&conn)?;
        Ok(list)
    }

    pub async fn pause_thread(&self, thread_id: &str, performed_by: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_paused_status(&conn, thread_id, true)?;

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "pause_thread".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("AI responses paused".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn resume_thread(&self, thread_id: &str, performed_by: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_paused_status(&conn, thread_id, false)?;

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "resume_thread".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("AI responses resumed".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn force_escalation(
        &self,
        thread_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;

        let thread = db::get_thread(&conn, thread_id)?
            .ok_or_else(|| anyhow!("Thread {} not found", thread_id))?;

        let contact = db::get_contact(&conn, &thread.contact_id)?
            .ok_or_else(|| anyhow!("Contact {} not found", thread.contact_id))?;

        // 1. Update thread status to Escalated
        db::update_thread_status(
            &conn,
            thread_id,
            crate::business::models::ThreadStatus::Escalated,
        )?;

        // 2. Insert escalation record
        let esc = OperatorEscalation {
            id: Uuid::new_v4().to_string(),
            thread_id: thread_id.to_string(),
            reason: reason.to_string(),
            status: "pending".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
        db::insert_escalation(&conn, &esc)?;

        // 3. Add to Operator Queue
        let queue_item = OperatorQueueItem {
            thread_id: thread_id.to_string(),
            customer_id: thread.contact_id.clone(),
            reason: format!("Forced Escalation: {}", reason),
            priority: "high".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            customer_summary: Some(format!(
                "Name: {}, Phone: {:?}",
                contact.name.clone().unwrap_or_default(),
                contact.phone
            )),
            thread_summary: Some("Forced by operator".to_string()),
        };
        db::add_to_operator_queue(&conn, &queue_item)?;

        // 4. Log Audit
        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "force_escalation".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some(reason.to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;

        // 5. Notify
        let _ = self
            .notifier
            .notify_new_escalation(&esc.id, thread_id, reason);

        Ok(())
    }

    pub async fn assign_operator(
        &self,
        thread_id: &str,
        operator_id: &str,
        performed_by: &str,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_operator(&conn, thread_id, Some(operator_id))?;

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "assign_operator".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some(format!("Assigned to operator: {}", operator_id)),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn close_thread(&self, thread_id: &str, performed_by: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_status(
            &conn,
            thread_id,
            crate::business::models::ThreadStatus::Closed,
        )?;

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "close_thread".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("Thread closed".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn reopen_thread(&self, thread_id: &str, performed_by: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_status(
            &conn,
            thread_id,
            crate::business::models::ThreadStatus::Active,
        )?;

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "reopen_thread".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("Thread reopened".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn set_thread_control_mode(
        &self,
        thread_id: &str,
        mode: ThreadControlMode,
        performed_by: &str,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::update_thread_control_mode(&conn, thread_id, mode)?;

        // Log Takeover Event for Analytics if HUMAN or HYBRID
        if mode == ThreadControlMode::Human || mode == ThreadControlMode::Hybrid {
            db::insert_analytics_event(&conn, "takeover", Some(thread_id), 1.0)?;
        }

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "set_control_mode".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some(format!("Mode changed to {:?}", mode)),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn transition_pipeline_stage(
        &self,
        opportunity_id: &str,
        to_stage: PipelineStage,
        performed_by: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        db::transition_opportunity_stage(
            &conn,
            self.notifier.as_ref(),
            opportunity_id,
            to_stage,
            performed_by,
            reason.as_deref(),
        )?;

        // If Won stage, record analytics event
        if to_stage == PipelineStage::Won {
            db::insert_analytics_event(&conn, "opportunity_won", None, 1.0)?;
        }

        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "pipeline_stage_change".to_string(),
            target_id: opportunity_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some(format!("Opportunity transitioned to stage {:?}", to_stage)),
        };
        db::insert_owner_audit_record(&conn, &audit)?;
        Ok(())
    }

    pub async fn send_suggestion(&self, thread_id: &str, performed_by: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;

        let suggestion = db::get_response_suggestion(&conn, thread_id)?
            .ok_or_else(|| anyhow!("No suggestion found for thread {}", thread_id))?;

        // 1. Log outbound message in business_message_records
        crate::business::workflow::log_message(
            &conn,
            thread_id,
            "business",
            &suggestion.suggested_reply,
        )?;

        // 2. Queue in Outbox
        if let Ok((channel, contact_id)) =
            crate::communication::db::get_channel_and_contact_for_thread(&conn, thread_id)
        {
            let outbound_msg = crate::communication::InteractionMessage::new(
                "default_business".to_string(),
                thread_id.to_string(),
                contact_id,
                channel,
                crate::communication::types::MessageDirection::Outbound,
                suggestion.suggested_reply.clone(),
            );
            crate::business::outbox::enqueue_outbox(&conn, &outbound_msg)?;
        }

        // 3. Delete suggestion
        db::delete_response_suggestion(&conn, thread_id)?;

        // 4. Audit log
        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "send_suggestion".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("Suggested response approved and sent".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;

        Ok(())
    }

    pub async fn send_operator_message(
        &self,
        thread_id: &str,
        message_body: &str,
        performed_by: &str,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;

        // 1. Log outbound message
        crate::business::workflow::log_message(&conn, thread_id, "business", message_body)?;

        // 2. Queue in Outbox
        if let Ok((channel, contact_id)) =
            crate::communication::db::get_channel_and_contact_for_thread(&conn, thread_id)
        {
            let outbound_msg = crate::communication::InteractionMessage::new(
                "default_business".to_string(),
                thread_id.to_string(),
                contact_id,
                channel,
                crate::communication::types::MessageDirection::Outbound,
                message_body.to_string(),
            );
            crate::business::outbox::enqueue_outbox(&conn, &outbound_msg)?;
        }

        // 3. Delete suggestion if exists
        db::delete_response_suggestion(&conn, thread_id)?;

        // 4. Audit log
        let audit = OwnerAuditRecord {
            id: Uuid::new_v4().to_string(),
            action: "send_operator_message".to_string(),
            target_id: thread_id.to_string(),
            performed_by: performed_by.to_string(),
            performed_at: chrono::Utc::now().timestamp(),
            reason: Some("Manual operator response sent".to_string()),
        };
        db::insert_owner_audit_record(&conn, &audit)?;

        Ok(())
    }

    pub async fn get_analytics(&self) -> Result<BusinessAnalytics> {
        let conn = self.runtime.db.lock().await;

        let avg_response_time = db::get_avg_analytics_metric(&conn, "response_time", 0, 0)?;
        let avg_auth_time = db::get_avg_analytics_metric(&conn, "authorization_time", 0, 0)?;

        // Escalation rate
        let total_threads: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_interaction_threads",
            [],
            |r| r.get(0),
        )?;
        let escalated_threads: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_interaction_threads WHERE status = 'escalated'",
            [],
            |r| r.get(0),
        )?;
        let escalation_rate = if total_threads > 0 {
            escalated_threads as f64 / total_threads as f64
        } else {
            0.0
        };

        // Booking conversion rate (Completed / Total)
        let total_bookings: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_scheduled_engagements",
            [],
            |r| r.get(0),
        )?;
        let completed_bookings: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_scheduled_engagements WHERE status = 'completed'",
            [],
            |r| r.get(0),
        )?;
        let booking_conversion_rate = if total_bookings > 0 {
            completed_bookings as f64 / total_bookings as f64
        } else {
            0.0
        };

        // Opportunity conversion rate (Won / Total)
        let total_opportunities: i64 =
            conn.query_row("SELECT COUNT(*) FROM business_opportunities", [], |r| {
                r.get(0)
            })?;
        let won_opportunities: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_opportunities WHERE pipeline_stage = 'won'",
            [],
            |r| r.get(0),
        )?;
        let opportunity_conversion_rate = if total_opportunities > 0 {
            won_opportunities as f64 / total_opportunities as f64
        } else {
            0.0
        };

        // Human takeover rate (threads takeover rate = threads with HUMAN or HYBRID mode / total threads)
        let takeover_threads: i64 = conn.query_row("SELECT COUNT(*) FROM business_interaction_threads WHERE control_mode IN ('human', 'hybrid')", [], |r| r.get(0))?;
        let human_takeover_rate = if total_threads > 0 {
            takeover_threads as f64 / total_threads as f64
        } else {
            0.0
        };

        Ok(BusinessAnalytics {
            average_response_time: avg_response_time,
            average_authorization_time: avg_auth_time,
            escalation_rate,
            booking_conversion_rate,
            opportunity_conversion_rate,
            human_takeover_rate,
        })
    }

    pub async fn get_reliability_metrics(
        &self,
    ) -> Result<crate::business::metrics::ReliabilityMetrics> {
        let conn = self.runtime.db.lock().await;
        let metrics = crate::business::metrics::get_reliability_metrics(&conn)?;
        Ok(metrics)
    }

    // Search methods
    pub async fn search_threads(&self, query: &str) -> Result<Vec<InteractionThread>> {
        let conn = self.runtime.db.lock().await;
        let list = db::search_threads_db(&conn, query)?;
        Ok(list)
    }

    pub async fn search_customers(
        &self,
        query: &str,
    ) -> Result<Vec<crate::business::models::ExternalContact>> {
        let conn = self.runtime.db.lock().await;
        let list = db::search_customers_db(&conn, query)?;
        Ok(list)
    }

    pub async fn search_bookings(&self, query: &str) -> Result<Vec<ScheduledEngagement>> {
        let conn = self.runtime.db.lock().await;
        let list = db::search_bookings_db(&conn, query)?;
        Ok(list)
    }

    pub async fn search_authorizations(&self, query: &str) -> Result<Vec<AuthorizationRequest>> {
        let conn = self.runtime.db.lock().await;
        let list = db::search_authorizations_db(&conn, query)?;
        Ok(list)
    }

    pub async fn search_audit_events(&self, query: &str) -> Result<Vec<OwnerAuditRecord>> {
        let conn = self.runtime.db.lock().await;
        let list = db::search_audit_events_db(&conn, query)?;
        Ok(list)
    }
}
