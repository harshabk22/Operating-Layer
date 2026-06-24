use crate::business::db;
use crate::business::jobs::retry::calculate_retry_delay;
use crate::business::runtime::CustomerInteractionRuntime;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

pub fn start_job_worker(runtime: Arc<CustomerInteractionRuntime>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_job_iteration(&runtime).await {
                eprintln!("[JobWorker] Error in iteration: {:?}", e);
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
}

struct JobRecord {
    id: String,
    job_type: String,
    payload_str: String,
    retries: i32,
    max_retries: i32,
}

fn query_next_job(conn: &rusqlite::Connection, now: i64) -> anyhow::Result<Option<JobRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, job_type, payload, retries, max_retries 
         FROM business_background_jobs 
         WHERE status = 'pending' AND run_at <= ?1 
         LIMIT 1",
    )?;
    let mut rows = stmt.query([now])?;
    if let Some(row) = rows.next()? {
        Ok(Some(JobRecord {
            id: row.get(0)?,
            job_type: row.get(1)?,
            payload_str: row.get(2)?,
            retries: row.get(3)?,
            max_retries: row.get(4)?,
        }))
    } else {
        Ok(None)
    }
}

async fn run_job_iteration(runtime: &CustomerInteractionRuntime) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let job_record = {
        let conn = runtime.db.lock().await;
        let record = query_next_job(&conn, now)?;
        if let Some(ref rec) = record {
            conn.execute(
                "UPDATE business_background_jobs SET status = 'processing' WHERE id = ?1",
                rusqlite::params![rec.id],
            )?;
        }
        record
    };

    if let Some(rec) = job_record {
        let payload: Value = serde_json::from_str(&rec.payload_str).unwrap_or(Value::Null);
        let res = execute_job_type(runtime, &rec.job_type, payload).await;

        let conn = runtime.db.lock().await;
        match res {
            Ok(_) => {
                let _now = chrono::Utc::now().timestamp();
                conn.execute(
                    "UPDATE business_background_jobs SET status = 'completed' WHERE id = ?1",
                    rusqlite::params![rec.id],
                )?;
            }
            Err(err) => {
                let err_str = err.to_string();
                eprintln!(
                    "[JobWorker] Job {} ({}) failed: {}",
                    rec.id, rec.job_type, err_str
                );
                let next_retries = rec.retries + 1;
                if next_retries >= rec.max_retries {
                    let _now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "UPDATE business_background_jobs SET status = 'failed', retries = ?1, last_error = ?2 WHERE id = ?3",
                        rusqlite::params![next_retries, err_str, rec.id],
                    )?;

                    let metric_key = match rec.job_type.as_str() {
                        "calendar_sync" => "calendar_failures_total",
                        "sheets_upsert" => "sheets_failures_total",
                        "notification" => "notification_failures_total",
                        _ => "job_failures_total",
                    };
                    let _ = db::increment_kpi_metric(&conn, metric_key);
                } else {
                    let delay = calculate_retry_delay(next_retries as u32, 2);
                    let next_run = chrono::Utc::now().timestamp() + delay as i64;
                    let _now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "UPDATE business_background_jobs SET status = 'pending', retries = ?1, last_error = ?2, run_at = ?3 WHERE id = ?4",
                        rusqlite::params![next_retries, err_str, next_run, rec.id],
                    )?;
                    let _ = db::increment_kpi_metric(&conn, "total_retry_count");
                }
            }
        }
    }
    Ok(())
}

async fn execute_job_type(
    runtime: &CustomerInteractionRuntime,
    job_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    match job_type {
        "calendar_sync" => {
            let request_id = payload
                .get("request_id")
                .and_then(|r| r.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing request_id in payload"))?;

            execute_calendar_sync_job(runtime, request_id).await?;
        }
        "sheets_upsert" => {
            let table = payload
                .get("table")
                .and_then(|t| t.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing table in payload"))?;
            let row_val = payload
                .get("row")
                .and_then(|r| r.as_array())
                .ok_or_else(|| anyhow::anyhow!("Missing row in payload"))?;
            let row: Vec<String> = row_val
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect();

            if let Some(key) = row.first() {
                if let Ok(Some((index, _))) = runtime.spreadsheet.lookup_row(table, key) {
                    runtime.spreadsheet.update_row(table, index, row)?;
                } else {
                    runtime.spreadsheet.append_row(table, row)?;
                }
            } else {
                runtime.spreadsheet.append_row(table, row)?;
            }

            let conn = runtime.db.lock().await;
            db::increment_kpi_metric(&conn, "sheets_upserts_total")?;
        }
        "notification" => {
            let notification_type = payload
                .get("type")
                .and_then(|t| t.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing type in payload"))?;

            match notification_type {
                "new_opportunity" => {
                    let opportunity_id = payload
                        .get("opportunity_id")
                        .and_then(|o| o.as_str())
                        .unwrap_or_default();
                    let thread_id = payload
                        .get("thread_id")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    runtime
                        .notifier
                        .notify_new_opportunity(opportunity_id, thread_id)?;
                }
                "pipeline_stage_change" => {
                    let opportunity_id = payload
                        .get("opportunity_id")
                        .and_then(|o| o.as_str())
                        .unwrap_or_default();
                    let from_stage_str = payload
                        .get("from_stage")
                        .and_then(|f| f.as_str())
                        .unwrap_or_default();
                    let to_stage_str = payload
                        .get("to_stage")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();

                    runtime.notifier.notify_pipeline_stage_change(
                        opportunity_id,
                        from_stage_str,
                        to_stage_str,
                    )?;
                }
                "new_escalation" => {
                    let escalation_id = payload
                        .get("escalation_id")
                        .and_then(|e| e.as_str())
                        .unwrap_or_default();
                    let thread_id = payload
                        .get("thread_id")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    let reason = payload
                        .get("reason")
                        .and_then(|r| r.as_str())
                        .unwrap_or_default();
                    runtime
                        .notifier
                        .notify_new_escalation(escalation_id, thread_id, reason)?;
                }
                "new_authorization" => {
                    let request_id = payload
                        .get("request_id")
                        .and_then(|r| r.as_str())
                        .unwrap_or_default();
                    let thread_id = payload
                        .get("thread_id")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    runtime
                        .notifier
                        .notify_new_authorization(request_id, thread_id)?;
                }
                _ => {}
            }
        }
        "analytics" => {
            let metric = payload
                .get("metric")
                .and_then(|m| m.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing metric in payload"))?;
            let thread_id = payload.get("thread_id").and_then(|t| t.as_str());
            let value = payload
                .get("value")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing value in payload"))?;

            let conn = runtime.db.lock().await;
            db::insert_analytics_event(&conn, metric, thread_id, value)?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown job type: {}", job_type));
        }
    }
    Ok(())
}

pub async fn execute_calendar_sync_job(
    runtime: &CustomerInteractionRuntime,
    request_id: &str,
) -> anyhow::Result<()> {
    let conn = runtime.db.lock().await;
    let req = db::get_authorization_request(&conn, request_id)?
        .ok_or_else(|| anyhow::anyhow!("Authorization request {} not found", request_id))?;
    if req.status != crate::business::models::AuthorizationStatus::Pending {
        println!("[Job] Authorization request {} is not pending (status: {:?}). Skipping.", request_id, req.status);
        return Ok(());
    }
    let engagement = db::get_engagement(&conn, &req.engagement_id)?
        .ok_or_else(|| anyhow::anyhow!("Engagement {} not found", req.engagement_id))?;
    drop(conn);

    let calendar_event = runtime
        .calendar
        .create_event(
            &engagement.title,
            engagement.start_time,
            engagement.end_time,
        )
        .map_err(|e| anyhow::anyhow!("Calendar Provider Error: {}", e))?;

    let conn = runtime.db.lock().await;
    db::increment_kpi_metric(&conn, "calendar_events_created_total")?;

    let sheets_payload = serde_json::json!({
        "table": "Bookings",
        "row": vec![
            engagement.id.clone(),
            engagement.contact_id.clone(),
            engagement.title.clone(),
            engagement.start_time.to_string(),
            engagement.end_time.to_string(),
            calendar_event.id.clone(),
        ]
    });
    crate::business::jobs::queue::enqueue_job(&conn, "sheets_upsert", sheets_payload, 5)?;

    db::update_engagement_status(
        &conn,
        &engagement.id,
        crate::business::models::EngagementStatus::Authorized,
    )?;
    db::update_authorization_status(
        &conn,
        &req.id,
        crate::business::models::AuthorizationStatus::Approved,
    )?;
    crate::business::workflow::update_thread_status(
        &conn,
        &req.thread_id,
        crate::business::models::ThreadStatus::Active,
    )?;

    db::increment_kpi_metric(&conn, "authorization_approved_total")?;
    db::increment_kpi_metric(&conn, "bookings_created_total")?;

    let formatted_time = chrono::DateTime::from_timestamp(engagement.start_time, 0)
        .map(|dt| dt.to_rfc2822())
        .unwrap_or_else(|| engagement.start_time.to_string());
    let confirmation_msg = format!(
        "Confirmed: Your appointment '{}' has been scheduled for {} (Calendar ID: {}).",
        engagement.title, formatted_time, calendar_event.id
    );

    crate::business::workflow::log_message(&conn, &req.thread_id, "business", &confirmation_msg)?;

    if let Ok((channel, contact_id)) =
        crate::communication::db::get_channel_and_contact_for_thread(&conn, &req.thread_id)
    {
        let outbound_msg = crate::communication::InteractionMessage::new(
            "default_business".to_string(),
            req.thread_id.clone(),
            contact_id,
            channel,
            crate::communication::types::MessageDirection::Outbound,
            confirmation_msg,
        );
        crate::business::outbox::enqueue_outbox(&conn, &outbound_msg)?;
    }

    let elapsed = chrono::Utc::now().timestamp() - req.created_at;
    let _ = db::insert_analytics_event(
        &conn,
        "authorization_time",
        Some(&req.thread_id),
        elapsed as f64,
    );

    let booking_latency = chrono::Utc::now().timestamp() - engagement.created_at;
    let _ = db::insert_analytics_event(
        &conn,
        "booking_latency",
        Some(&req.thread_id),
        booking_latency as f64,
    );

    let event_payload = serde_json::json!({
        "engagement_id": engagement.id,
        "calendar_event_id": calendar_event.id
    });
    let _ = crate::business::event_store::record_event(
        &conn,
        "booking_completed",
        &engagement.id,
        &event_payload,
    );

    if let Ok(Some(opp)) = db::get_opportunity_by_thread(&conn, &req.thread_id) {
        let _ = db::transition_opportunity_stage(
            &conn,
            runtime.notifier.as_ref(),
            &opp.id,
            crate::business::models::PipelineStage::Booked,
            "system",
            Some("Booking approved"),
        );
        let _ = db::transition_opportunity_stage(
            &conn,
            runtime.notifier.as_ref(),
            &opp.id,
            crate::business::models::PipelineStage::Won,
            "system",
            Some("Booking finalized"),
        );
        let _ = db::insert_analytics_event(&conn, "opportunity_won", None, 1.0);
    }

    crate::business::reports::generate_reports(&conn)?;

    Ok(())
}
