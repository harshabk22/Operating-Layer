use super::connector::ChannelConnector;
use super::types::{ChannelType, InteractionMessage};
use crate::MoatResult;
use async_trait::async_trait;
use std::sync::Mutex;

pub struct MockConnector {
    pub received_messages: Mutex<Vec<InteractionMessage>>,
    pub sent_messages: Mutex<Vec<InteractionMessage>>,
    pub open_threads: Mutex<Vec<(String, String, String, ChannelType)>>, // (business_id, thread_id, customer_id, channel)
    pub closed_threads: Mutex<Vec<(String, String, ChannelType)>>, // (business_id, thread_id, channel)
    pub is_healthy: Mutex<bool>,
}

impl Default for MockConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl MockConnector {
    pub fn new() -> Self {
        Self {
            received_messages: Mutex::new(Vec::new()),
            sent_messages: Mutex::new(Vec::new()),
            open_threads: Mutex::new(Vec::new()),
            closed_threads: Mutex::new(Vec::new()),
            is_healthy: Mutex::new(true),
        }
    }

    pub fn set_healthy(&self, healthy: bool) {
        let mut h = self.is_healthy.lock().unwrap();
        *h = healthy;
    }

    pub fn get_received_messages(&self) -> Vec<InteractionMessage> {
        self.received_messages.lock().unwrap().clone()
    }

    pub fn get_sent_messages(&self) -> Vec<InteractionMessage> {
        self.sent_messages.lock().unwrap().clone()
    }
}

#[async_trait]
impl ChannelConnector for MockConnector {
    async fn receive_message(&self, message: InteractionMessage) -> MoatResult<()> {
        let mut list = self.received_messages.lock().unwrap();
        list.push(message);
        Ok(())
    }

    async fn send_message(&self, message: InteractionMessage) -> MoatResult<()> {
        let mut list = self.sent_messages.lock().unwrap();
        list.push(message);
        Ok(())
    }

    async fn open_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        customer_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()> {
        let mut list = self.open_threads.lock().unwrap();
        list.push((
            business_id.to_string(),
            thread_id.to_string(),
            customer_id.to_string(),
            channel,
        ));
        Ok(())
    }

    async fn close_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()> {
        let mut list = self.closed_threads.lock().unwrap();
        list.push((business_id.to_string(), thread_id.to_string(), channel));
        Ok(())
    }

    async fn health_check(&self) -> MoatResult<bool> {
        Ok(*self.is_healthy.lock().unwrap())
    }
}
