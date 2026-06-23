use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::business::db;
use crate::business::models::{
    AuthorizationRequest, InteractionThread, OperatorEscalation, OpportunityStatus,
    ThreadControlMode, ThreadStatus,
};
use crate::business::runtime::CustomerInteractionRuntime;

pub struct InboxService {
    runtime: Arc<CustomerInteractionRuntime>,
}

impl InboxService {
    pub fn new(runtime: Arc<CustomerInteractionRuntime>) -> Self {
        Self { runtime }
    }

    // --- Assignment Views ---

    /// Get all threads currently assigned to an operator (view read directly from projection table)
    pub async fn get_assignment_view(&self, operator_id: &str) -> Result<Vec<InteractionThread>> {
        let conn = self.runtime.db.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, contact_id, status, opportunity_status, created_at, updated_at, control_mode, assigned_operator, is_paused 
             FROM business_interaction_threads 
             WHERE assigned_operator = ?1 
             ORDER BY updated_at DESC"
        )?;
        let mut rows = stmt.query([operator_id])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let status_str: String = row.get(2)?;
            let status = match status_str.as_str() {
                "active" => ThreadStatus::Active,
                "awaiting_authorization" => ThreadStatus::AwaitingAuthorization,
                "closed" => ThreadStatus::Closed,
                "escalated" => ThreadStatus::Escalated,
                _ => ThreadStatus::Active,
            };

            let opp_status_str: String = row.get(3)?;
            let opportunity_status = match opp_status_str.as_str() {
                "discovered" => OpportunityStatus::Discovered,
                "qualified" => OpportunityStatus::Qualified,
                "disqualified" => OpportunityStatus::Disqualified,
                "engagement_pending" => OpportunityStatus::EngagementPending,
                "converted" => OpportunityStatus::Converted,
                "archived" => OpportunityStatus::Archived,
                _ => OpportunityStatus::Discovered,
            };

            let control_mode_str: String = row.get(6)?;
            let control_mode = match control_mode_str.as_str() {
                "ai" => ThreadControlMode::Ai,
                "human" => ThreadControlMode::Human,
                "hybrid" => ThreadControlMode::Hybrid,
                _ => ThreadControlMode::Ai,
            };

            items.push(InteractionThread {
                id: row.get(0)?,
                contact_id: row.get(1)?,
                status,
                opportunity_status,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                control_mode,
                assigned_operator: row.get(7)?,
                is_paused: {
                    let paused: i32 = row.get(8)?;
                    paused != 0
                },
            });
        }
        Ok(items)
    }

    // --- Approval Views ---

    /// Get all pending authorization requests (view read directly from projection table)
    pub async fn get_approval_view(&self) -> Result<Vec<AuthorizationRequest>> {
        let conn = self.runtime.db.lock().await;
        // db::list_pending_authorization_requests reads from business_authorization_requests
        let list = db::list_pending_authorization_requests(&conn)?;
        Ok(list)
    }

    // --- Escalation Views ---

    /// Get all escalations that require attention (view read directly from projection table)
    pub async fn get_escalation_view(&self) -> Result<Vec<OperatorEscalation>> {
        let conn = self.runtime.db.lock().await;
        // Read directly from business_operator_escalations
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

    // --- Operator Actions ---

    pub async fn claim_escalation(&self, escalation_id: &str, operator_id: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;

        let esc = db::get_escalation(&conn, escalation_id)?
            .ok_or_else(|| anyhow!("Escalation not found"))?;

        let payload = serde_json::json!({
            "operator_id": operator_id,
            "thread_id": esc.thread_id,
            "status": "claimed",
            "action": "claim_escalation"
        });
        crate::business::event_store::record_event(
            &conn,
            "EscalationClaimed",
            escalation_id,
            &payload,
        )?;

        // NOTE: Projections are updated purely via rebuilding from the Event Store.
        Ok(())
    }

    pub async fn resolve_escalation(
        &self,
        escalation_id: &str,
        operator_id: &str,
        resolution_notes: &str,
    ) -> Result<()> {
        let conn = self.runtime.db.lock().await;

        let esc = db::get_escalation(&conn, escalation_id)?
            .ok_or_else(|| anyhow!("Escalation not found"))?;

        let payload = serde_json::json!({
            "operator_id": operator_id,
            "thread_id": esc.thread_id,
            "status": "resolved",
            "action": "resolve_escalation",
            "resolution_notes": resolution_notes
        });
        crate::business::event_store::record_event(
            &conn,
            "EscalationResolved",
            escalation_id,
            &payload,
        )?;

        // NOTE: Projections are updated purely via rebuilding from the Event Store.
        Ok(())
    }

    pub async fn approve_request(&self, request_id: &str, operator_id: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        let payload = serde_json::json!({
            "operator_id": operator_id,
            "status": "approved",
            "action": "approve_request"
        });
        crate::business::event_store::record_event(&conn, "RequestApproved", request_id, &payload)?;

        // NOTE: Projections are updated purely via rebuilding from the Event Store.
        Ok(())
    }

    pub async fn reject_request(&self, request_id: &str, operator_id: &str) -> Result<()> {
        let conn = self.runtime.db.lock().await;
        let payload = serde_json::json!({
            "operator_id": operator_id,
            "status": "rejected",
            "action": "reject_request"
        });
        crate::business::event_store::record_event(&conn, "RequestRejected", request_id, &payload)?;

        // NOTE: Projections are updated purely via rebuilding from the Event Store.
        Ok(())
    }
}
