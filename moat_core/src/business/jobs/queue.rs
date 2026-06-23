use crate::MoatResult;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn enqueue_job(
    conn: &Connection,
    job_type: &str,
    payload: serde_json::Value,
    max_retries: i32,
) -> MoatResult<String> {
    let now = chrono::Utc::now().timestamp();
    let id = Uuid::new_v4().to_string();
    let payload_str = payload.to_string();

    conn.execute(
        "INSERT INTO business_background_jobs (id, job_type, payload, status, retries, max_retries, run_at, created_at) VALUES (?1, ?2, ?3, 'pending', 0, ?4, ?5, ?5)",
        params![id, job_type, payload_str, max_retries, now],
    )?;

    Ok(id)
}
