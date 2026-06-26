use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaileysInboundMessage {
    pub message_id: String,
    pub chat_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub body: String,
    pub timestamp: i64,
}
