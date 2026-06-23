use super::db;
use super::models::{
    AuthorizationRequest, AuthorizationStatus, EngagementStatus, InteractionThread, MessageRecord,
    Opportunity, OpportunityStatus, PipelineStage, ScheduledEngagement, ThreadControlMode,
    ThreadStatus,
};
use crate::communication::db::persist_event;
use crate::communication::types::ConnectorEvent;
use crate::MoatError;
use crate::MoatResult;
use rusqlite::Connection;
use uuid::Uuid;

pub fn resolve_active_thread(conn: &Connection, contact_id: &str) -> MoatResult<InteractionThread> {
    if let Some(thread) = db::get_active_thread_by_contact(conn, contact_id)? {
        return Ok(thread);
    }

    // Create a new thread
    let now = chrono::Utc::now().timestamp();
    let thread_id = Uuid::new_v4().to_string();
    let thread = InteractionThread {
        id: thread_id.clone(),
        contact_id: contact_id.to_string(),
        status: ThreadStatus::Active,
        opportunity_status: OpportunityStatus::Discovered,
        created_at: now,
        updated_at: now,
        control_mode: ThreadControlMode::Ai,
        assigned_operator: None,
        is_paused: false,
    };

    db::insert_thread(conn, &thread)?;

    // Create a corresponding opportunity
    let opp = Opportunity {
        id: Uuid::new_v4().to_string(),
        contact_id: contact_id.to_string(),
        thread_id,
        status: OpportunityStatus::Discovered,
        value: None,
        notes: None,
        created_at: now,
        updated_at: now,
        pipeline_stage: PipelineStage::New,
    };

    db::insert_opportunity(conn, &opp)?;

    Ok(thread)
}

pub fn update_thread_status(
    conn: &Connection,
    thread_id: &str,
    status: ThreadStatus,
) -> MoatResult<()> {
    let thread = db::get_thread(conn, thread_id)?
        .ok_or_else(|| MoatError::Orchestrator(format!("Thread ID {} not found", thread_id)))?;

    if !thread.status.can_transition_to(status) {
        return Err(MoatError::Orchestrator(format!(
            "Invalid thread status transition from {:?} to {:?}",
            thread.status, status
        )));
    }

    db::update_thread_status(conn, thread_id, status)?;

    if status == ThreadStatus::Closed {
        if let Ok((channel, _)) =
            crate::communication::db::get_channel_and_contact_for_thread(conn, thread_id)
        {
            let close_event = ConnectorEvent::ThreadClosed {
                business_id: "default_business".to_string(),
                thread_id: thread_id.to_string(),
                channel,
            };
            let _ = persist_event(conn, &close_event, &channel.to_string());
        }
    }

    Ok(())
}

pub fn update_opportunity_status(
    conn: &Connection,
    thread_id: &str,
    status: OpportunityStatus,
) -> MoatResult<()> {
    // Check thread exists
    let thread = db::get_thread(conn, thread_id)?.ok_or_else(|| {
        MoatError::Orchestrator(format!("Thread ID {} does not exist", thread_id))
    })?;

    if !thread.opportunity_status.can_transition_to(status) {
        return Err(MoatError::Orchestrator(format!(
            "Invalid opportunity status transition from {:?} to {:?}",
            thread.opportunity_status, status
        )));
    }

    db::update_thread_opportunity_status(conn, thread_id, status)?;
    db::update_opportunity_status(conn, thread_id, status)?;
    Ok(())
}

pub fn log_message(
    conn: &Connection,
    thread_id: &str,
    sender: &str,
    content: &str,
) -> MoatResult<MessageRecord> {
    let now = chrono::Utc::now().timestamp();
    let msg = MessageRecord {
        id: Uuid::new_v4().to_string(),
        thread_id: thread_id.to_string(),
        sender: sender.to_string(),
        content: content.to_string(),
        timestamp: now,
    };

    db::insert_message(conn, &msg)?;
    Ok(msg)
}

// Engagement workflows
pub fn propose_engagement(
    conn: &Connection,
    contact_id: &str,
    thread_id: &str,
    title: &str,
    start_time: i64,
    end_time: i64,
) -> MoatResult<ScheduledEngagement> {
    // Create proposed engagement.
    // RULE: The system MUST NOT create finalized engagements (authorized/completed) automatically.
    // They must always start as proposed.
    let now = chrono::Utc::now().timestamp();
    let eng = ScheduledEngagement {
        id: Uuid::new_v4().to_string(),
        contact_id: contact_id.to_string(),
        thread_id: thread_id.to_string(),
        title: title.to_string(),
        status: EngagementStatus::Proposed,
        start_time,
        end_time,
        created_at: now,
        updated_at: now,
    };

    db::insert_engagement(conn, &eng)?;

    let event_payload = serde_json::json!({
        "engagement_id": eng.id,
        "contact_id": eng.contact_id,
        "thread_id": eng.thread_id,
        "title": eng.title,
        "start_time": eng.start_time,
        "end_time": eng.end_time
    });
    let _ = crate::business::event_store::record_event(
        conn,
        "booking_created",
        &eng.id,
        &event_payload,
    );

    Ok(eng)
}

pub fn request_authorization(
    conn: &Connection,
    engagement_id: &str,
    proposed_by: &str,
) -> MoatResult<AuthorizationRequest> {
    let eng = db::get_engagement(conn, engagement_id)?.ok_or_else(|| {
        MoatError::Orchestrator(format!("Engagement ID {} not found", engagement_id))
    })?;

    if eng.status != EngagementStatus::Proposed {
        return Err(MoatError::Orchestrator(format!(
            "Cannot request authorization for engagement in status {:?}",
            eng.status
        )));
    }

    // 1. Transition engagement status
    db::update_engagement_status(conn, engagement_id, EngagementStatus::PendingAuthorization)?;

    // 2. Transition thread status
    update_thread_status(conn, &eng.thread_id, ThreadStatus::AwaitingAuthorization)?;

    // 3. Create AuthorizationRequest
    let now = chrono::Utc::now().timestamp();
    let req = AuthorizationRequest {
        id: Uuid::new_v4().to_string(),
        thread_id: eng.thread_id.clone(),
        engagement_id: engagement_id.to_string(),
        proposed_by: proposed_by.to_string(),
        status: AuthorizationStatus::Pending,
        created_at: now,
    };
    db::insert_authorization_request(conn, &req)?;

    // Record AuthorizationRequested event to communication_events
    if let Ok((channel, _)) =
        crate::communication::db::get_channel_and_contact_for_thread(conn, &eng.thread_id)
    {
        let auth_event = ConnectorEvent::AuthorizationRequested {
            business_id: "default_business".to_string(),
            thread_id: eng.thread_id.clone(),
            request_id: req.id.clone(),
            action: "engagement:request_authorization".to_string(),
            payload: serde_json::to_string(&req).unwrap_or_default(),
        };
        let _ = persist_event(conn, &auth_event, &channel.to_string());
    }

    crate::api::event_bus::broadcast_event(
        "new_authorization",
        serde_json::json!({
            "auth_id": req.id,
            "thread_id": eng.thread_id
        }),
    );

    Ok(req)
}

pub fn authorize_engagement(conn: &Connection, engagement_id: &str) -> MoatResult<()> {
    // 1. Find pending authorization request
    let req =
        db::get_pending_authorization_by_engagement(conn, engagement_id)?.ok_or_else(|| {
            MoatError::Orchestrator(format!(
                "No pending authorization request found for engagement {}",
                engagement_id
            ))
        })?;

    // 2. Approve authorization
    db::update_authorization_status(conn, &req.id, AuthorizationStatus::Approved)?;

    // 3. Update engagement status
    db::update_engagement_status(conn, engagement_id, EngagementStatus::Authorized)?;

    // 4. Reset thread status back to Active
    update_thread_status(conn, &req.thread_id, ThreadStatus::Active)?;

    Ok(())
}

pub fn reject_engagement(conn: &Connection, engagement_id: &str) -> MoatResult<()> {
    let req =
        db::get_pending_authorization_by_engagement(conn, engagement_id)?.ok_or_else(|| {
            MoatError::Orchestrator(format!(
                "No pending authorization request found for engagement {}",
                engagement_id
            ))
        })?;

    // 1. Reject authorization
    db::update_authorization_status(conn, &req.id, AuthorizationStatus::Rejected)?;

    // 2. Update engagement status
    db::update_engagement_status(conn, engagement_id, EngagementStatus::Rejected)?;

    // 3. Reset thread status back to Active
    update_thread_status(conn, &req.thread_id, ThreadStatus::Active)?;

    Ok(())
}

pub fn expire_engagement(conn: &Connection, engagement_id: &str) -> MoatResult<()> {
    let req =
        db::get_pending_authorization_by_engagement(conn, engagement_id)?.ok_or_else(|| {
            MoatError::Orchestrator(format!(
                "No pending authorization request found for engagement {}",
                engagement_id
            ))
        })?;

    // 1. Expire authorization
    db::update_authorization_status(conn, &req.id, AuthorizationStatus::Expired)?;

    // 2. Update engagement status
    db::update_engagement_status(conn, engagement_id, EngagementStatus::Cancelled)?;

    // 3. Reset thread status back to Active
    update_thread_status(conn, &req.thread_id, ThreadStatus::Active)?;

    Ok(())
}
