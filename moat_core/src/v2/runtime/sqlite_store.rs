use super::core::{EventEnvelope, EventStore, Snapshot, SnapshotStore};
use async_trait::async_trait;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct SqliteEventStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteEventStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl EventStore for SqliteEventStore {
    async fn append_event(&self, envelope: EventEnvelope) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let payload_str = serde_json::to_string(&envelope.event.payload).unwrap();
        let meta_str = serde_json::to_string(&envelope.metadata).unwrap();

        // Ensure table exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS v2_events (
                event_id TEXT PRIMARY KEY,
                aggregate_id TEXT NOT NULL,
                sequence_number INTEGER NOT NULL,
                payload TEXT NOT NULL,
                metadata TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        ).map_err(|e| e.to_string())?;

        let timestamp = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO v2_events (event_id, aggregate_id, sequence_number, payload, metadata, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                envelope.event_id.to_string(),
                envelope.aggregate_id.to_string(),
                envelope.sequence_number as i64,
                payload_str,
                meta_str,
                timestamp
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn load_event_stream(
        &self,
        aggregate_id: Uuid,
        after_sequence: Option<u64>,
    ) -> Result<Vec<EventEnvelope>, String> {
        let conn = self.conn.lock().unwrap();
        
        let seq = after_sequence.unwrap_or(0);
        let mut stmt = conn.prepare(
            "SELECT event_id, aggregate_id, sequence_number, payload, metadata
             FROM v2_events 
             WHERE aggregate_id = ?1 AND sequence_number > ?2
             ORDER BY sequence_number ASC"
        ).map_err(|e| e.to_string())?;

        let agg_id_str = aggregate_id.to_string();
        let rows = stmt.query_map(params![agg_id_str, seq as i64], |row| {
            let event_id: String = row.get(0)?;
            let agg_id: String = row.get(1)?;
            let seq_num: i64 = row.get(2)?;
            let payload_str: String = row.get(3)?;
            let meta_str: String = row.get(4)?;

            let envelope = EventEnvelope {
                event_id: Uuid::parse_str(&event_id).unwrap(),
                aggregate_id: Uuid::parse_str(&agg_id).unwrap(),
                sequence_number: seq_num as u64,
                event: super::core::Event {
                    payload: serde_json::from_str(&payload_str).unwrap(),
                },
                metadata: serde_json::from_str(&meta_str).unwrap(),
            };
            Ok(envelope)
        }).map_err(|e| e.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| e.to_string())?);
        }

        Ok(events)
    }

    async fn load_all_events(&self) -> Result<Vec<EventEnvelope>, String> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT event_id, aggregate_id, sequence_number, payload, metadata
             FROM v2_events 
             ORDER BY timestamp ASC"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |row| {
            let event_id: String = row.get(0)?;
            let agg_id: String = row.get(1)?;
            let seq_num: i64 = row.get(2)?;
            let payload_str: String = row.get(3)?;
            let meta_str: String = row.get(4)?;

            let envelope = EventEnvelope {
                event_id: Uuid::parse_str(&event_id).unwrap(),
                aggregate_id: Uuid::parse_str(&agg_id).unwrap(),
                sequence_number: seq_num as u64,
                event: super::core::Event {
                    payload: serde_json::from_str(&payload_str).unwrap(),
                },
                metadata: serde_json::from_str(&meta_str).unwrap(),
            };
            Ok(envelope)
        }).map_err(|e| e.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| e.to_string())?);
        }

        Ok(events)
    }
}

pub struct SqliteSnapshotStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSnapshotStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl SnapshotStore for SqliteSnapshotStore {
    async fn save_snapshot(&self, snapshot: Snapshot) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let state_str = serde_json::to_string(&snapshot.state).unwrap();
        let timestamp = snapshot.timestamp.timestamp();

        conn.execute(
            "CREATE TABLE IF NOT EXISTS v2_snapshots (
                aggregate_id TEXT PRIMARY KEY,
                sequence_number INTEGER NOT NULL,
                state TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        ).map_err(|e| e.to_string())?;

        conn.execute(
            "INSERT OR REPLACE INTO v2_snapshots (aggregate_id, sequence_number, state, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot.aggregate_id.to_string(),
                snapshot.sequence_number as i64,
                state_str,
                timestamp
            ],
        ).map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn load_snapshot(&self, aggregate_id: Uuid) -> Result<Option<Snapshot>, String> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT sequence_number, state, timestamp
             FROM v2_snapshots 
             WHERE aggregate_id = ?1"
        ).map_err(|e| e.to_string())?;

        let agg_id_str = aggregate_id.to_string();
        let mut rows = stmt.query(params![agg_id_str]).map_err(|e| e.to_string())?;

        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let seq_num: i64 = row.get(0).map_err(|e| e.to_string())?;
            let state_str: String = row.get(1).map_err(|e| e.to_string())?;
            let timestamp: i64 = row.get(2).map_err(|e| e.to_string())?;

            Ok(Some(Snapshot {
                aggregate_id,
                sequence_number: seq_num as u64,
                state: serde_json::from_str(&state_str).unwrap(),
                timestamp: chrono::DateTime::from_timestamp(timestamp, 0).unwrap(),
            }))
        } else {
            Ok(None)
        }
    }
}
