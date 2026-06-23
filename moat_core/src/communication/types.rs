use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Whatsapp,
    Instagram,
    Messenger,
    Web,
    Voice,
    Unknown,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ChannelType::Whatsapp => "whatsapp",
            ChannelType::Instagram => "instagram",
            ChannelType::Messenger => "messenger",
            ChannelType::Web => "web",
            ChannelType::Voice => "voice",
            ChannelType::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

impl std::fmt::Display for MessageDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            MessageDirection::Inbound => "inbound",
            MessageDirection::Outbound => "outbound",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractionMessage {
    pub id: String,
    pub business_id: String,
    pub thread_id: String,
    pub customer_id: String,
    pub channel: ChannelType,
    pub direction: MessageDirection,
    pub body: String,
    pub timestamp: i64,
}

impl InteractionMessage {
    pub fn new(
        business_id: String,
        thread_id: String,
        customer_id: String,
        channel: ChannelType,
        direction: MessageDirection,
        body: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            business_id,
            thread_id,
            customer_id,
            channel,
            direction,
            body,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ConnectorEvent {
    MessageReceived(InteractionMessage),
    MessageSent(InteractionMessage),
    ThreadOpened {
        business_id: String,
        thread_id: String,
        customer_id: String,
        channel: ChannelType,
    },
    ThreadClosed {
        business_id: String,
        thread_id: String,
        channel: ChannelType,
    },
    EscalationRequested {
        business_id: String,
        thread_id: String,
        reason: String,
    },
    AuthorizationRequested {
        business_id: String,
        thread_id: String,
        request_id: String,
        action: String,
        payload: String,
    },
}

impl ConnectorEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            ConnectorEvent::MessageReceived(_) => "message_received",
            ConnectorEvent::MessageSent(_) => "message_sent",
            ConnectorEvent::ThreadOpened { .. } => "thread_opened",
            ConnectorEvent::ThreadClosed { .. } => "thread_closed",
            ConnectorEvent::EscalationRequested { .. } => "escalation_requested",
            ConnectorEvent::AuthorizationRequested { .. } => "authorization_requested",
        }
    }

    pub fn business_id(&self) -> &str {
        match self {
            ConnectorEvent::MessageReceived(msg) => &msg.business_id,
            ConnectorEvent::MessageSent(msg) => &msg.business_id,
            ConnectorEvent::ThreadOpened { business_id, .. } => business_id,
            ConnectorEvent::ThreadClosed { business_id, .. } => business_id,
            ConnectorEvent::EscalationRequested { business_id, .. } => business_id,
            ConnectorEvent::AuthorizationRequested { business_id, .. } => business_id,
        }
    }

    pub fn thread_id(&self) -> &str {
        match self {
            ConnectorEvent::MessageReceived(msg) => &msg.thread_id,
            ConnectorEvent::MessageSent(msg) => &msg.thread_id,
            ConnectorEvent::ThreadOpened { thread_id, .. } => thread_id,
            ConnectorEvent::ThreadClosed { thread_id, .. } => thread_id,
            ConnectorEvent::EscalationRequested { thread_id, .. } => thread_id,
            ConnectorEvent::AuthorizationRequested { thread_id, .. } => thread_id,
        }
    }
}
