use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents an event received from an external system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngressEvent {
    pub connection_id: String,
    pub payload: Vec<u8>,
}

impl IngressEvent {
    pub fn new(connection_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            connection_id: connection_id.into(),
            payload,
        }
    }
}

/// Represents a request to be sent to an external system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressRequest {
    pub connection_id: String,
    pub payload: Vec<u8>,
}

impl EgressRequest {
    pub fn new(connection_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            connection_id: connection_id.into(),
            payload,
        }
    }
}

/// A generic abstraction for an asynchronous connection.
#[async_trait]
pub trait Connection: Send + Sync {
    /// Returns the unique identifier of the connection.
    fn id(&self) -> &str;

    /// Sends an egress request through this connection.
    async fn send(&self, request: EgressRequest) -> Result<(), String>;
}

/// Routes ingress and egress traffic to/from the appropriate connections.
pub struct ConnectionRouter {
    connections: Arc<RwLock<HashMap<String, Arc<dyn Connection>>>>,
}

impl ConnectionRouter {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds a connection to the router.
    pub async fn add_connection(&self, connection: Arc<dyn Connection>) {
        let mut conns = self.connections.write().await;
        conns.insert(connection.id().to_string(), connection);
    }

    /// Removes a connection from the router by ID.
    pub async fn remove_connection(&self, connection_id: &str) -> Option<Arc<dyn Connection>> {
        let mut conns = self.connections.write().await;
        conns.remove(connection_id)
    }

    /// Routes an ingress event to the core system (placeholder logic for now).
    pub async fn route_ingress(&self, _event: IngressEvent) -> Result<(), String> {
        // Generic abstraction: In a real system, this would push to a queue or channel.
        Ok(())
    }

    /// Routes an egress request to the appropriate connection.
    pub async fn route_egress(&self, request: EgressRequest) -> Result<(), String> {
        let conns = self.connections.read().await;
        if let Some(conn) = conns.get(&request.connection_id) {
            conn.send(request).await
        } else {
            Err(format!("Connection not found: {}", request.connection_id))
        }
    }
}

impl Default for ConnectionRouter {
    fn default() -> Self {
        Self::new()
    }
}
