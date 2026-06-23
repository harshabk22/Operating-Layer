use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Event Primitives ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventMetadata {
    pub timestamp: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub sequence_number: u64,
    pub event: Event,
    pub metadata: EventMetadata,
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append_event(&self, envelope: EventEnvelope) -> Result<(), String>;
    async fn load_event_stream(
        &self,
        aggregate_id: Uuid,
        after_sequence: Option<u64>,
    ) -> Result<Vec<EventEnvelope>, String>;
    async fn load_all_events(&self) -> Result<Vec<EventEnvelope>, String>;
}

// --- Snapshot Primitives ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub aggregate_id: Uuid,
    pub sequence_number: u64,
    pub state: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[async_trait]
pub trait SnapshotStore: Send + Sync {
    async fn save_snapshot(&self, snapshot: Snapshot) -> Result<(), String>;
    async fn load_snapshot(&self, aggregate_id: Uuid) -> Result<Option<Snapshot>, String>;
}


// --- RuntimeCore ---

pub struct RuntimeCore<E: EventStore, S: SnapshotStore> {
    pub event_store: E,
    pub snapshot_store: S,
}

impl<E: EventStore, S: SnapshotStore> RuntimeCore<E, S> {
    pub fn new(event_store: E, snapshot_store: S) -> Self {
        Self {
            event_store,
            snapshot_store,
        }
    }

    pub async fn recover_state(
        &self,
        aggregate_id: Uuid,
    ) -> Result<(Option<Snapshot>, Vec<EventEnvelope>), String> {
        let snapshot = self.snapshot_store.load_snapshot(aggregate_id).await?;
        let after_seq = snapshot.as_ref().map(|s| s.sequence_number);
        let events = self
            .event_store
            .load_event_stream(aggregate_id, after_seq)
            .await?;

        Ok((snapshot, events))
    }
}
