use super::types::ConnectorEvent;
use crate::MoatError;
use crate::MoatResult;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rusqlite::{params, Connection};

pub fn prepare_tables(conn: &Connection) -> MoatResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS communication_events (
            id TEXT PRIMARY KEY,
            business_id TEXT NOT NULL,
            thread_id TEXT NOT NULL,
            connector_type TEXT NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS communication_connector_sessions (
            business_id TEXT NOT NULL,
            connector_type TEXT NOT NULL,
            file_name TEXT NOT NULL,
            file_content TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (business_id, connector_type, file_name)
        );",
    )
    ?;
    Ok(())
}

pub fn persist_event(
    conn: &Connection,
    event: &ConnectorEvent,
    connector_type: &str,
) -> MoatResult<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let business_id = event.business_id();
    let thread_id = event.thread_id();
    let event_type = event.event_type();
    let payload = serde_json::to_string(event).map_err(MoatError::Serde)?;
    let created_at = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO communication_events (id, business_id, thread_id, connector_type, event_type, payload, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, business_id, thread_id, connector_type, event_type, payload, created_at],
    )?;

    Ok(())
}

pub fn list_events(
    conn: &Connection,
    thread_id: &str,
) -> MoatResult<Vec<(String, String, String, String)>> {
    let mut stmt = conn
        .prepare("SELECT event_type, connector_type, payload, id FROM communication_events WHERE thread_id = ?1 ORDER BY created_at ASC")
        ?;

    let rows = stmt
        .query_map(params![thread_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        ?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

pub fn get_channel_and_contact_for_thread(
    conn: &Connection,
    thread_id: &str,
) -> MoatResult<(super::types::ChannelType, String)> {
    let contact_id: String = conn
        .query_row(
            "SELECT contact_id FROM business_interaction_threads WHERE id = ?1",
            params![thread_id],
            |row| row.get(0),
        )
        .map_err(|e| MoatError::Orchestrator(format!("Thread not found: {e}")))?;

    let mut stmt = conn
        .prepare(
            "SELECT channel, external_id FROM business_identity_mappings WHERE contact_id = ?1",
        )
        ?;
    let mut rows = stmt
        .query(params![contact_id])
        ?;
    if let Some(row) = rows.next()? {
        let channel_str: String = row.get(0)?;
        let customer_id: String = row.get(1)?;
        let channel = match channel_str.as_str() {
            "whatsapp" => super::types::ChannelType::Whatsapp,
            "instagram" => super::types::ChannelType::Instagram,
            "messenger" => super::types::ChannelType::Messenger,
            "web" | "webchat" => super::types::ChannelType::Web,
            "voice" => super::types::ChannelType::Voice,
            _ => super::types::ChannelType::Unknown,
        };
        Ok((channel, customer_id))
    } else {
        Ok((super::types::ChannelType::Unknown, "".to_string()))
    }
}

fn get_encryption_key() -> String {
    std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string())
}

pub fn save_session_file(
    conn: &Connection,
    business_id: &str,
    connector_type: &str,
    file_name: &str,
    content: &[u8],
) -> MoatResult<()> {
    let key = get_encryption_key();
    let b64 = STANDARD.encode(content);
    let encrypted = crate::security::encryption::encrypt_credential(&b64, &key);
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO communication_connector_sessions (business_id, connector_type, file_name, file_content, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(business_id, connector_type, file_name) DO UPDATE SET
             file_content = excluded.file_content,
             updated_at = excluded.updated_at",
        params![business_id, connector_type, file_name, encrypted, now],
    )
    ?;

    Ok(())
}

pub fn load_session_files(
    conn: &Connection,
    business_id: &str,
    connector_type: &str,
) -> MoatResult<Vec<(String, Vec<u8>)>> {
    let mut stmt = conn
        .prepare("SELECT file_name, file_content FROM communication_connector_sessions WHERE business_id = ?1 AND connector_type = ?2")
        ?;

    let mut rows = stmt
        .query(params![business_id, connector_type])
        ?;

    let key = get_encryption_key();
    let mut results = Vec::new();

    while let Some(row) = rows.next()? {
        let file_name: String = row.get(0)?;
        let encrypted: String = row.get(1)?;

        let b64 = crate::security::encryption::decrypt_credential(&encrypted, &key)
            .map_err(|e| MoatError::SecurityViolation(format!("Decryption failed: {}", e)))?;
        let bytes = STANDARD
            .decode(&b64)
            .map_err(|e| MoatError::SecurityViolation(format!("Base64 decode failed: {}", e)))?;

        results.push((file_name, bytes));
    }

    Ok(results)
}

pub fn delete_session_file(
    conn: &Connection,
    business_id: &str,
    connector_type: &str,
    file_name: &str,
) -> MoatResult<()> {
    conn.execute(
        "DELETE FROM communication_connector_sessions WHERE business_id = ?1 AND connector_type = ?2 AND file_name = ?3",
        params![business_id, connector_type, file_name],
    )
    ?;
    Ok(())
}

pub fn clear_session(conn: &Connection, business_id: &str, connector_type: &str) -> MoatResult<()> {
    conn.execute(
        "DELETE FROM communication_connector_sessions WHERE business_id = ?1 AND connector_type = ?2",
        params![business_id, connector_type],
    )
    ?;
    Ok(())
}

pub fn has_baileys_credentials(conn: &Connection, business_id: &str) -> MoatResult<bool> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM communication_connector_sessions WHERE business_id = ?1 AND connector_type = 'whatsapp_baileys' AND file_name = 'creds.json' LIMIT 1")
        ?;
    let mut rows = stmt
        .query(params![business_id])
        ?;
    Ok(rows.next()?.is_some())
}
