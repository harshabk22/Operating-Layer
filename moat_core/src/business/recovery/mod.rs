use crate::business::runtime::CustomerInteractionRuntime;

pub async fn run_recovery(runtime: &CustomerInteractionRuntime) -> crate::MoatResult<()> {
    let conn = runtime.db.lock().await;

    crate::business::event_store::record_event(&conn,
        "StateMutated",
        "aggregate_id",
        &serde_json::json!({ "type": "LegacyMutationReplaced" }),
    )?;

    crate::business::event_store::record_event(&conn,
        "StateMutated",
        "aggregate_id",
        &serde_json::json!({ "type": "LegacyMutationReplaced" }),
    )?;

    let mut stmt = conn.prepare(
        "SELECT a.id 
         FROM business_authorization_requests a
         JOIN business_scheduled_engagements e ON a.engagement_id = e.id
         WHERE a.status = 'approved' AND e.status = 'pending'",
    )?;
    let mut rows = stmt.query([])?;
    let mut stuck_request_ids = Vec::new();
    while let Some(row) = rows.next()? {
        let req_id: String = row.get(0)?;
        stuck_request_ids.push(req_id);
    }
    drop(rows);
    drop(stmt);

    for req_id in stuck_request_ids {
        let job_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM business_background_jobs WHERE job_type = 'calendar_sync' AND payload LIKE ?1 AND status IN ('pending', 'running'))",
            [format!("%\"request_id\":\"{}\"%", req_id)],
            |r| r.get(0),
        ).unwrap_or(false);

        if !job_exists {
            println!(
                "[Recovery] Re-queuing stuck calendar sync job for authorization request {}",
                req_id
            );
            let payload = serde_json::json!({ "request_id": req_id });
            crate::business::jobs::queue::enqueue_job(&conn, "calendar_sync", payload, 5)?;
        }
    }

    Ok(())
}
