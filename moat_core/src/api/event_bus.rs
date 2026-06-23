use std::sync::OnceLock;
use tokio::sync::broadcast::{channel, Receiver, Sender};

pub static EVENT_BROADCASTER: OnceLock<Sender<String>> = OnceLock::new();

/// Broadcasts an event to all active WebSocket connections.
pub fn broadcast_event(event_type: &str, data: serde_json::Value) {
    if let Some(tx) = EVENT_BROADCASTER.get() {
        let payload = serde_json::json!({
            "type": event_type,
            "data": data
        });
        let _ = tx.send(payload.to_string());
    }
}

/// Initializes the global event broadcaster if not already set, and returns a new subscriber Receiver.
pub fn init_broadcaster() -> Receiver<String> {
    let tx = EVENT_BROADCASTER.get_or_init(|| {
        let (tx, _rx) = channel(100);
        tx
    });
    tx.subscribe()
}
