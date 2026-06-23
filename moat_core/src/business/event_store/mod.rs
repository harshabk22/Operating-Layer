use crate::MoatResult;
use rusqlite::{params, Connection};
use uuid::Uuid;

pub fn record_event(
    conn: &Connection,
    event_type: &str,
    aggregate_id: &str,
    payload: &serde_json::Value,
) -> MoatResult<()> {
    let now = chrono::Utc::now().timestamp();
    let id = Uuid::new_v4().to_string();
    let payload_str = payload.to_string();

    conn.execute(
        "INSERT INTO business_event_store (id, event_type, aggregate_id, payload, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, event_type, aggregate_id, payload_str, now],
    )?;

    Ok(())
}
