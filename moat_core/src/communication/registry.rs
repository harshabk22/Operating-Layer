use super::connector::ChannelConnector;
use super::types::ConnectorEvent;
use crate::MoatResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ConnectorRegistry {
    connectors: RwLock<HashMap<String, Arc<dyn ChannelConnector>>>,
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self {
            connectors: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, id: &str, connector: Arc<dyn ChannelConnector>) {
        let mut map = self.connectors.write().await;
        map.insert(id.to_string(), connector);
    }

    pub async fn remove(&self, id: &str) -> Option<Arc<dyn ChannelConnector>> {
        let mut map = self.connectors.write().await;
        map.remove(id)
    }

    pub async fn lookup(&self, id: &str) -> Option<Arc<dyn ChannelConnector>> {
        let map = self.connectors.read().await;
        map.get(id).cloned()
    }

    pub async fn dispatch_event(
        &self,
        event: ConnectorEvent,
        connector_type: &str,
    ) -> MoatResult<()> {
        // Dispatch to the registered connector if any
        if let Some(conn) = self.lookup(connector_type).await {
            match &event {
                ConnectorEvent::MessageReceived(msg) => {
                    // Usually this triggers reception from the client, so we invoke receive_message
                    conn.receive_message(msg.clone()).await?;
                }
                ConnectorEvent::MessageSent(msg) => {
                    // Send outbound message back to client
                    conn.send_message(msg.clone()).await?;
                }
                ConnectorEvent::ThreadOpened {
                    business_id,
                    thread_id,
                    customer_id,
                    channel,
                } => {
                    conn.open_thread(business_id, thread_id, customer_id, *channel)
                        .await?;
                }
                ConnectorEvent::ThreadClosed {
                    business_id,
                    thread_id,
                    channel,
                } => {
                    conn.close_thread(business_id, thread_id, *channel).await?;
                }
                _ => {} // Other events are workflow notifications (Escalation / Authorization)
            }
        }

        Ok(())
    }
}
