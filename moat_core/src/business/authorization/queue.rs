use crate::business::db;
use crate::business::models::{
    AuthorizationRequest, AuthorizationStatus, EngagementStatus, ThreadStatus,
};
use crate::MoatResult;
use rusqlite::Connection;

pub struct AuthorizationQueue;

impl AuthorizationQueue {
    pub fn create_request(
        conn: &Connection,
        engagement_id: &str,
        proposed_by: &str,
    ) -> MoatResult<AuthorizationRequest> {
        let req =
            crate::business::workflow::request_authorization(conn, engagement_id, proposed_by)?;
        db::increment_kpi_metric(conn, "authorization_requests_total")?;

        let event_payload = serde_json::json!({
            "request_id": req.id,
            "engagement_id": req.engagement_id,
            "thread_id": req.thread_id,
            "proposed_by": proposed_by
        });
        let _ = crate::business::event_store::record_event(
            conn,
            "authorization_created",
            &req.id,
            &event_payload,
        );

        Ok(req)
    }

    pub fn approve_request(conn: &Connection, request_id: &str) -> MoatResult<()> {
        let req = db::get_authorization_request(conn, request_id)?.ok_or_else(|| {
            crate::MoatError::Orchestrator(format!(
                "Authorization request {} not found",
                request_id
            ))
        })?;

        db::update_authorization_status(conn, &req.id, AuthorizationStatus::Approved)?;
        db::update_engagement_status(conn, &req.engagement_id, EngagementStatus::Authorized)?;
        crate::business::workflow::update_thread_status(
            conn,
            &req.thread_id,
            ThreadStatus::Active,
        )?;

        db::increment_kpi_metric(conn, "authorization_approved_total")?;
        Ok(())
    }

    pub fn reject_request(conn: &Connection, request_id: &str) -> MoatResult<()> {
        let req = db::get_authorization_request(conn, request_id)?.ok_or_else(|| {
            crate::MoatError::Orchestrator(format!(
                "Authorization request {} not found",
                request_id
            ))
        })?;

        db::update_authorization_status(conn, &req.id, AuthorizationStatus::Rejected)?;
        db::update_engagement_status(conn, &req.engagement_id, EngagementStatus::Rejected)?;
        crate::business::workflow::update_thread_status(
            conn,
            &req.thread_id,
            ThreadStatus::Active,
        )?;

        db::increment_kpi_metric(conn, "authorization_rejected_total")?;
        Ok(())
    }

    pub fn expire_request(conn: &Connection, request_id: &str) -> MoatResult<()> {
        let req = db::get_authorization_request(conn, request_id)?.ok_or_else(|| {
            crate::MoatError::Orchestrator(format!(
                "Authorization request {} not found",
                request_id
            ))
        })?;

        db::update_authorization_status(conn, &req.id, AuthorizationStatus::Expired)?;
        db::update_engagement_status(conn, &req.engagement_id, EngagementStatus::Cancelled)?;
        crate::business::workflow::update_thread_status(
            conn,
            &req.thread_id,
            ThreadStatus::Active,
        )?;

        Ok(())
    }

    pub fn list_pending(conn: &Connection) -> MoatResult<Vec<AuthorizationRequest>> {
        db::list_pending_authorization_requests(conn)
    }
}
