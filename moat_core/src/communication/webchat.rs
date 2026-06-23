use crate::communication::connector::ChannelConnector;
use crate::communication::types::{ChannelType, InteractionMessage};
use crate::MoatError;
use crate::MoatResult;
use async_trait::async_trait;

pub struct WebChatConnector;

impl Default for WebChatConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl WebChatConnector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ChannelConnector for WebChatConnector {
    async fn receive_message(&self, _message: InteractionMessage) -> MoatResult<()> {
        Err(MoatError::Orchestrator(
            "Website Chat connector not yet implemented".to_string(),
        ))
    }

    async fn send_message(&self, _message: InteractionMessage) -> MoatResult<()> {
        Err(MoatError::Orchestrator(
            "Website Chat connector not yet implemented".to_string(),
        ))
    }

    async fn open_thread(
        &self,
        _business_id: &str,
        _thread_id: &str,
        _customer_id: &str,
        _channel: ChannelType,
    ) -> MoatResult<()> {
        Err(MoatError::Orchestrator(
            "Website Chat connector not yet implemented".to_string(),
        ))
    }

    async fn close_thread(
        &self,
        _business_id: &str,
        _thread_id: &str,
        _channel: ChannelType,
    ) -> MoatResult<()> {
        Err(MoatError::Orchestrator(
            "Website Chat connector not yet implemented".to_string(),
        ))
    }

    async fn health_check(&self) -> MoatResult<bool> {
        Ok(false)
    }
}
