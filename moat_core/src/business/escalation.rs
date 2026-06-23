use super::db;
use super::models::{OperatorEscalation, ThreadStatus};
use crate::communication::db::persist_event;
use crate::communication::types::ConnectorEvent;
use crate::MoatError;
use crate::MoatResult;
use rusqlite::Connection;
use uuid::Uuid;

pub fn escalate_thread(
    conn: &Connection,
    thread_id: &str,
    reason: &str,
) -> MoatResult<OperatorEscalation> {
    // Verify thread exists
    let thread = db::get_thread(conn, thread_id)?
        .ok_or_else(|| MoatError::Orchestrator(format!("Thread ID {} not found", thread_id)))?;

    if thread.status == ThreadStatus::Closed {
        return Err(MoatError::Orchestrator(
            "Cannot escalate a closed thread".to_string(),
        ));
    }

    let now = chrono::Utc::now().timestamp();
    let esc = OperatorEscalation {
        id: Uuid::new_v4().to_string(),
        thread_id: thread_id.to_string(),
        reason: reason.to_string(),
        status: "pending".to_string(),
        created_at: now,
        updated_at: now,
    };

    db::insert_escalation(conn, &esc)?;

    let event_payload = serde_json::json!({
        "escalation_id": esc.id,
        "thread_id": esc.thread_id,
        "reason": esc.reason
    });
    let _ = crate::business::event_store::record_event(
        conn,
        "escalation_created",
        &esc.id,
        &event_payload,
    );

    super::workflow::update_thread_status(conn, thread_id, ThreadStatus::Escalated)?;

    // Record EscalationRequested event to communication_events
    if let Ok((channel, _)) =
        crate::communication::db::get_channel_and_contact_for_thread(conn, thread_id)
    {
        let esc_event = ConnectorEvent::EscalationRequested {
            business_id: "default_business".to_string(),
            thread_id: thread_id.to_string(),
            reason: reason.to_string(),
        };
        let _ = persist_event(conn, &esc_event, &channel.to_string());
    }

    crate::api::event_bus::broadcast_event(
        "new_escalation",
        serde_json::json!({
            "escalation_id": esc.id,
            "thread_id": thread_id,
            "reason": reason
        }),
    );

    Ok(esc)
}
