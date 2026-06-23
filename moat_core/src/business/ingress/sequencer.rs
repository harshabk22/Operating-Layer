use crate::business::db;
use crate::business::models::{
    AuditEvent, DeadLetterEntry, IngressEvent, InteractionBatch, SystemEvent,
};
use crate::business::runtime::CustomerInteractionRuntime;
use crate::MoatResult;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use uuid::Uuid;

const DEBOUNCE_MS: u64 = 2500;
const SEQUENCER_ID: &str = "ingress-sequencer";

pub struct IngressSequencer {
    runtime: Arc<CustomerInteractionRuntime>,
    tx: mpsc::UnboundedSender<String>,
    timers: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl IngressSequencer {
    pub fn new(
        runtime: Arc<CustomerInteractionRuntime>,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sequencer = Arc::new(Self {
            runtime,
            tx,
            timers: Mutex::new(HashMap::new()),
        });
        (sequencer, rx)
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<String> {
        self.tx.clone()
    }

    pub async fn ingest_event(
        runtime: &Arc<CustomerInteractionRuntime>,
        business_id: &str,
        event: SystemEvent,
        thread_id: Option<&str>,
        channel: Option<&str>,
        external_id: Option<&str>,
        payload: &str,
    ) -> MoatResult<()> {
        let now = chrono::Utc::now().timestamp();
        let ingress = IngressEvent {
            id: Uuid::new_v4().to_string(),
            business_id: business_id.to_string(),
            event_type: event.as_str().to_string(),
            thread_id: thread_id.map(|s| s.to_string()),
            channel: channel.map(|s| s.to_string()),
            external_id: external_id.map(|s| s.to_string()),
            payload: payload.to_string(),
            created_at: now,
        };

        let conn = runtime.db.lock().await;
        db::insert_ingress_event(&conn, &ingress)?;
        db::insert_audit_event(
            &conn,
            &AuditEvent {
                id: Uuid::new_v4().to_string(),
                business_id: business_id.to_string(),
                event_type: event.as_str().to_string(),
                actor: "system".to_string(),
                payload: payload.to_string(),
                timestamp: now,
            },
        )?;
        Ok(())
    }

    pub async fn run_sequencer_loop(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<String>) {
        let runtime = self.runtime.clone();
        let sequencer = self.clone();

        while let Some(thread_id) = rx.recv().await {
            let seq = sequencer.clone();
            let rt = runtime.clone();
            let thread_id_clone = thread_id.clone();

            // Cancel existing timer for this thread
            {
                let mut timers = seq.timers.lock().await;
                if let Some(handle) = timers.remove(&thread_id) {
                    handle.abort();
                }
            }

            let handle = tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(DEBOUNCE_MS)).await;

                // Process batch
                let result = {
                    let conn = rt.db.lock().await;
                    process_batch(&conn, &thread_id_clone)
                };

                match result {
                    Ok(()) => {
                        // Mark batch done
                        if let Ok(conn) = rt.db.try_lock() {
                            let batch = InteractionBatch {
                                id: Uuid::new_v4().to_string(),
                                thread_id: thread_id_clone.clone(),
                                business_id: String::new(),
                                batch_size: 1,
                                status: "processed".to_string(),
                                created_at: chrono::Utc::now().timestamp(),
                            };
                            let _ = db::insert_interaction_batch(&conn, &batch);
                        }
                    }
                    Err(e) => {
                        // Dead letter
                        if let Ok(conn) = rt.db.try_lock() {
                            let dl = DeadLetterEntry {
                                id: Uuid::new_v4().to_string(),
                                payload: thread_id_clone.clone(),
                                error: e.to_string(),
                                attempt_count: 0,
                                created_at: chrono::Utc::now().timestamp(),
                            };
                            let _ = db::insert_dead_letter(&conn, &dl);
                        }
                    }
                }

                // Clean up timer
                let mut timers = seq.timers.lock().await;
                timers.remove(&thread_id_clone);
            });

            let mut timers = sequencer.timers.lock().await;
            timers.insert(thread_id, handle);
        }
    }
}

fn process_batch(conn: &Connection, thread_id: &str) -> MoatResult<()> {
    // Acquire thread lock
    let locked = db::acquire_thread_lock(conn, thread_id, SEQUENCER_ID)?;
    if !locked {
        return Ok(()); // Already locked, skip
    }

    // Load pending ingress events for this thread
    let events = db::list_pending_ingress_events(conn, 100)?;

    let mut ids_to_delete = Vec::new();
    for event in &events {
        if event.thread_id.as_deref() == Some(thread_id) {
            ids_to_delete.push(event.id.clone());
        }
    }

    if !ids_to_delete.is_empty() {
        db::delete_ingress_events(conn, &ids_to_delete)?;
    }

    // Release lock
    db::release_thread_lock(conn, thread_id, SEQUENCER_ID)?;

    Ok(())
}
