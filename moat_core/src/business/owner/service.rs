use super::NotificationProvider;
use crate::business::db;
use crate::business::models::{
    AuthorizationRequest, OperatorEscalation, OwnerAuditRecord, OwnerMetricsDashboard,
    ScheduledEngagement, ThreadStatus,
};
use crate::business::runtime::CustomerInteractionRuntime;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use uuid::Uuid;

pub struct OwnerService {
    runtime: Arc<CustomerInteractionRuntime>,
    notifier: Arc<dyn NotificationProvider>,
}

impl OwnerService {
    pub fn new(
        runtime: Arc<CustomerInteractionRuntime>,
        notifier: Arc<dyn NotificationProvider>,
    ) -> Self {
        Self { runtime, notifier }
    }

    pub async fn approve_authorization(
        &self,
        auth_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<String> {
        // Load details for notification prior to approval in case status changes
        let (engagement_id, thread_id) = {
            let conn = self.runtime.db.lock().await;
            let req = db::get_authorization_request(&conn, auth_id)?
                .ok_or_else(|| anyhow!("Authorization request {} not found", auth_id))?;
            (req.engagement_id.clone(), req.thread_id.clone())
        };

        // Call runtime approval (executes booking internally)
        let confirmation_msg = self.runtime.approve_and_execute_booking(auth_id).await?;

        // Write Audit log
        {
            let conn = self.runtime.db.lock().await;
            let audit = OwnerAuditRecord {
                id: Uuid::new_v4().to_string(),
                action: "approve_authorization".to_string(),
                target_id: auth_id.to_string(),
                performed_by: performed_by.to_string(),
                performed_at: chrono::Utc::now().timestamp(),
                reason: Some(reason.to_string()),
            };
            db::insert_owner_audit_record(&conn, &audit)?;
        }

        // Notify
        let _ = self
            .notifier
            .notify_booking_completed(&engagement_id, &thread_id);

        Ok(confirmation_msg)
    }

    pub async fn reject_authorization(
        &self,
        auth_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<()> {
        // Call runtime rejection
        self.runtime.reject_authorization(auth_id).await?;

        // Write Audit log
        {
            let conn = self.runtime.db.lock().await;
            let audit = OwnerAuditRecord {
                id: Uuid::new_v4().to_string(),
                action: "reject_authorization".to_string(),
                target_id: auth_id.to_string(),
                performed_by: performed_by.to_string(),
                performed_at: chrono::Utc::now().timestamp(),
                reason: Some(reason.to_string()),
            };
            db::insert_owner_audit_record(&conn, &audit)?;
        }

        Ok(())
    }

    pub async fn list_pending_requests(&self) -> Result<Vec<AuthorizationRequest>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_pending_authorization_requests(&conn)?;
        Ok(list)
    }

    pub async fn list_escalations(&self) -> Result<Vec<OperatorEscalation>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_escalations(&conn)?;
        Ok(list)
    }

    pub async fn list_recent_bookings(&self) -> Result<Vec<ScheduledEngagement>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_recent_bookings(&conn)?;
        Ok(list)
    }

    pub async fn resolve_escalation(
        &self,
        escalation_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<()> {
        let (thread_id, current_status) = {
            let conn = self.runtime.db.lock().await;
            let esc = db::get_escalation(&conn, escalation_id)?
                .ok_or_else(|| anyhow!("Escalation {} not found", escalation_id))?;
            (esc.thread_id.clone(), esc.status.clone())
        };

        if current_status == "resolved" || current_status == "closed" {
            return Err(anyhow!(
                "Escalation {} is already in resolved or closed state",
                escalation_id
            ));
        }

        {
            let conn = self.runtime.db.lock().await;
            db::update_escalation_status(&conn, escalation_id, "resolved")?;
            db::update_thread_status(&conn, &thread_id, ThreadStatus::Active)?;

            let event_payload = serde_json::json!({
                "escalation_id": escalation_id,
                "status": "resolved"
            });
            let _ = crate::business::event_store::record_event(
                &conn,
                "escalation_resolved",
                escalation_id,
                &event_payload,
            );

            let audit = OwnerAuditRecord {
                id: Uuid::new_v4().to_string(),
                action: "resolve_escalation".to_string(),
                target_id: escalation_id.to_string(),
                performed_by: performed_by.to_string(),
                performed_at: chrono::Utc::now().timestamp(),
                reason: Some(reason.to_string()),
            };
            db::insert_owner_audit_record(&conn, &audit)?;
        }

        Ok(())
    }

    pub async fn close_escalation(
        &self,
        escalation_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<()> {
        let thread_id = {
            let conn = self.runtime.db.lock().await;
            let esc = db::get_escalation(&conn, escalation_id)?
                .ok_or_else(|| anyhow!("Escalation {} not found", escalation_id))?;
            esc.thread_id.clone()
        };

        {
            let conn = self.runtime.db.lock().await;
            db::update_escalation_status(&conn, escalation_id, "closed")?;
            db::update_thread_status(&conn, &thread_id, ThreadStatus::Closed)?;

            let event_payload = serde_json::json!({
                "escalation_id": escalation_id,
                "status": "closed"
            });
            let _ = crate::business::event_store::record_event(
                &conn,
                "escalation_resolved",
                escalation_id,
                &event_payload,
            );

            let audit = OwnerAuditRecord {
                id: Uuid::new_v4().to_string(),
                action: "close_escalation".to_string(),
                target_id: escalation_id.to_string(),
                performed_by: performed_by.to_string(),
                performed_at: chrono::Utc::now().timestamp(),
                reason: Some(reason.to_string()),
            };
            db::insert_owner_audit_record(&conn, &audit)?;
        }

        Ok(())
    }

    pub async fn reopen_escalation(
        &self,
        escalation_id: &str,
        performed_by: &str,
        reason: &str,
    ) -> Result<()> {
        let thread_id = {
            let conn = self.runtime.db.lock().await;
            let esc = db::get_escalation(&conn, escalation_id)?
                .ok_or_else(|| anyhow!("Escalation {} not found", escalation_id))?;
            esc.thread_id.clone()
        };

        {
            let conn = self.runtime.db.lock().await;
            db::update_escalation_status(&conn, escalation_id, "reopened")?;
            db::update_thread_status(&conn, &thread_id, ThreadStatus::Escalated)?;

            let event_payload = serde_json::json!({
                "escalation_id": escalation_id,
                "status": "reopened"
            });
            let _ = crate::business::event_store::record_event(
                &conn,
                "escalation_reopened",
                escalation_id,
                &event_payload,
            );

            let audit = OwnerAuditRecord {
                id: Uuid::new_v4().to_string(),
                action: "reopen_escalation".to_string(),
                target_id: escalation_id.to_string(),
                performed_by: performed_by.to_string(),
                performed_at: chrono::Utc::now().timestamp(),
                reason: Some(reason.to_string()),
            };
            db::insert_owner_audit_record(&conn, &audit)?;
        }

        Ok(())
    }

    pub async fn get_metrics(&self) -> Result<OwnerMetricsDashboard> {
        let conn = self.runtime.db.lock().await;

        // 1. Pending Authorizations
        let pending_auths: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_authorization_requests WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )?;

        // 2. Pending Escalations (status = 'pending' or 'reopened')
        let pending_escs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_operator_escalations WHERE status IN ('pending', 'reopened')",
            [],
            |r| r.get(0)
        )?;

        // 3. Bookings Today
        let now = chrono::Utc::now();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let bookings_today: i64 = conn.query_row(
            "SELECT COUNT(*) FROM business_scheduled_engagements WHERE created_at >= ?1",
            [today_start],
            |r| r.get(0),
        )?;

        // 4. Bookings Approved (historical count from kpi metrics)
        let bookings_approved =
            db::get_kpi_metric(&conn, "authorization_approved_total").unwrap_or(0);

        // 5. Bookings Rejected (historical count from kpi metrics)
        let bookings_rejected =
            db::get_kpi_metric(&conn, "authorization_rejected_total").unwrap_or(0);

        Ok(OwnerMetricsDashboard {
            pending_authorizations: pending_auths as u64,
            pending_escalations: pending_escs as u64,
            bookings_today: bookings_today as u64,
            bookings_approved,
            bookings_rejected,
        })
    }

    pub async fn list_audit_records(&self) -> Result<Vec<OwnerAuditRecord>> {
        let conn = self.runtime.db.lock().await;
        let list = db::list_owner_audit_records(&conn)?;
        Ok(list)
    }
}
