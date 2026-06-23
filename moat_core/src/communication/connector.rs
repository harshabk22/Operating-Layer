use super::types::{ChannelType, InteractionMessage};
use crate::MoatResult;
use async_trait::async_trait;

#[async_trait]
pub trait ChannelConnector: Send + Sync {
    /// Receive a message from an external system / customer client and feed it to the runtime.
    async fn receive_message(&self, message: InteractionMessage) -> MoatResult<()>;

    /// Send a message back to the customer on their channel.
    async fn send_message(&self, message: InteractionMessage) -> MoatResult<()>;

    /// Initialize/open a conversation thread for a customer on a channel.
    async fn open_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        customer_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()>;

    /// Mark a conversation thread as closed.
    async fn close_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()>;

    /// Check if the connector instance is healthy.
    async fn health_check(&self) -> MoatResult<bool>;

    /// Retrieve the current QR code of the connector if it is in pairing mode.
    async fn get_qr_code(&self) -> MoatResult<Option<String>> {
        Ok(None)
    }
}
