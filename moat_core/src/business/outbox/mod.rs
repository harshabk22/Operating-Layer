use crate::business::jobs::retry::calculate_retry_delay;
use crate::business::runtime::CustomerInteractionRuntime;
use crate::communication::{ConnectorEvent, InteractionMessage};
use crate::MoatResult;
use rusqlite::{params, Connection};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

pub fn enqueue_outbox(conn: &Connection, msg: &InteractionMessage) -> MoatResult<String> {
    let now = chrono::Utc::now().timestamp();
    let id = Uuid::new_v4().to_string();
    let channel_str = msg.channel.to_string();
    let direction_str = msg.direction.to_string();

    conn.execute(
        "INSERT INTO business_message_outbox (id, business_id, thread_id, customer_id, channel, direction, body, status, retries, max_retries, run_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, 5, ?8, ?8)",
        params![id, msg.business_id, msg.thread_id, msg.customer_id, channel_str, direction_str, msg.body, now],
    )?;

    Ok(id)
}

pub fn start_outbox_worker(
    runtime: Arc<CustomerInteractionRuntime>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_outbox_iteration(&runtime).await {
                eprintln!("[OutboxWorker] Error: {:?}", e);
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
}

struct OutboxRecord {
    id: String,
    business_id: String,
    thread_id: String,
    customer_id: String,
    channel_str: String,
    direction_str: String,
    body: String,
    retries: i32,
    max_retries: i32,
    created_at: i64,
}

fn query_next_outbox(conn: &Connection, now: i64) -> anyhow::Result<Option<OutboxRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, business_id, thread_id, customer_id, channel, direction, body, retries, max_retries, created_at
         FROM business_message_outbox
         WHERE status = 'pending' AND run_at <= ?1
         LIMIT 1"
    )?;
    let mut rows = stmt.query([now])?;
    if let Some(row) = rows.next()? {
        Ok(Some(OutboxRecord {
            id: row.get(0)?,
            business_id: row.get(1)?,
            thread_id: row.get(2)?,
            customer_id: row.get(3)?,
            channel_str: row.get(4)?,
            direction_str: row.get(5)?,
            body: row.get(6)?,
            retries: row.get(7)?,
            max_retries: row.get(8)?,
            created_at: row.get(9)?,
        }))
    } else {
        Ok(None)
    }
}

async fn run_outbox_iteration(runtime: &CustomerInteractionRuntime) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let outbox_record = {
        let conn = runtime.db.lock().await;
        let record = query_next_outbox(&conn, now)?;
        if let Some(ref rec) = record {
            conn.execute(
                "UPDATE business_message_outbox SET status = 'processing' WHERE id = ?1",
                rusqlite::params![rec.id],
            )?;
        }
        record
    };

    if let Some(rec) = outbox_record {
        let channel_type = parse_channel_type(&rec.channel_str);
        let direction_type = parse_message_direction(&rec.direction_str);
        let msg_body = rec.body.clone();
        let outbound_msg = InteractionMessage::new(
            rec.business_id,
            rec.thread_id.clone(),
            rec.customer_id,
            channel_type,
            direction_type,
            rec.body,
        );

        let msg_sent_event = ConnectorEvent::MessageSent(outbound_msg);
        let res = runtime
            .registry
            .dispatch_event(msg_sent_event.clone(), &rec.channel_str)
            .await;

        let conn = runtime.db.lock().await;
        match res {
            Ok(_) => {
                let _now = chrono::Utc::now().timestamp();
                conn.execute(
                    "UPDATE business_message_outbox SET status = 'completed' WHERE id = ?1",
                    rusqlite::params![rec.id],
                )?;
                let latency = chrono::Utc::now().timestamp() - rec.created_at;
                let _ = crate::business::db::insert_analytics_event(
                    &conn,
                    "message_latency",
                    Some(&rec.thread_id),
                    latency as f64,
                );
                let _ = crate::communication::db::persist_event(
                    &conn,
                    &msg_sent_event,
                    &rec.channel_str,
                );

                crate::api::event_bus::broadcast_event(
                    "new_message",
                    serde_json::json!({
                        "thread_id": rec.thread_id,
                        "sender": "business",
                        "content": msg_body,
                        "timestamp": chrono::Utc::now().timestamp()
                    }),
                );
            }
            Err(err) => {
                let err_str = err.to_string();
                eprintln!(
                    "[OutboxWorker] Outbox message {} failed: {}",
                    rec.id, err_str
                );
                let next_retries = rec.retries + 1;
                if next_retries >= rec.max_retries {
                    let _now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "UPDATE business_message_outbox SET status = 'failed', retries = ?1, last_error = ?2 WHERE id = ?3",
                        rusqlite::params![next_retries, err_str, rec.id],
                    )?;
                    let _ = crate::business::db::increment_kpi_metric(
                        &conn,
                        "message_delivery_failures_total",
                    );
                } else {
                    let delay = calculate_retry_delay(next_retries as u32, 2);
                    let next_run = chrono::Utc::now().timestamp() + delay as i64;
                    let _now = chrono::Utc::now().timestamp();
                    conn.execute(
                        "UPDATE business_message_outbox SET status = 'pending', retries = ?1, last_error = ?2, run_at = ?3 WHERE id = ?4",
                        rusqlite::params![next_retries, err_str, next_run, rec.id],
                    )?;
                    let _ = crate::business::db::increment_kpi_metric(&conn, "total_retry_count");
                }
            }
        }
    }
    Ok(())
}

fn parse_channel_type(s: &str) -> crate::communication::types::ChannelType {
    match s.to_lowercase().as_str() {
        "whatsapp" => crate::communication::types::ChannelType::Whatsapp,
        "instagram" => crate::communication::types::ChannelType::Instagram,
        "messenger" => crate::communication::types::ChannelType::Messenger,
        "web" | "webchat" => crate::communication::types::ChannelType::Web,
        "voice" => crate::communication::types::ChannelType::Voice,
        _ => crate::communication::types::ChannelType::Unknown,
    }
}

fn parse_message_direction(s: &str) -> crate::communication::types::MessageDirection {
    if s.to_lowercase() == "inbound" {
        crate::communication::types::MessageDirection::Inbound
    } else {
        crate::communication::types::MessageDirection::Outbound
    }
}
