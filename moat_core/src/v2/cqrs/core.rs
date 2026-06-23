use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::v2::runtime::core::{EventEnvelope, EventStore};

#[async_trait]
pub trait ReadModel: Send + Sync {
    fn name(&self) -> &str;
    async fn handle_event(&self, envelope: &EventEnvelope) -> Result<(), String>;
}

#[async_trait]
pub trait ProjectionStore: Send + Sync {
    async fn get_last_processed_sequence(
        &self,
        projection_name: &str,
    ) -> Result<Option<u64>, String>;
    async fn update_last_processed_sequence(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), String>;
    async fn reset_last_processed_sequence(&self, projection_name: &str) -> Result<(), String>;
}

pub struct InMemoryProjectionStore {
    sequences: Arc<RwLock<HashMap<String, u64>>>,
}

impl InMemoryProjectionStore {
    pub fn new() -> Self {
        Self {
            sequences: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryProjectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProjectionStore for InMemoryProjectionStore {
    async fn get_last_processed_sequence(
        &self,
        projection_name: &str,
    ) -> Result<Option<u64>, String> {
        let lock = self.sequences.read().await;
        Ok(lock.get(projection_name).cloned())
    }

    async fn update_last_processed_sequence(
        &self,
        projection_name: &str,
        sequence: u64,
    ) -> Result<(), String> {
        let mut lock = self.sequences.write().await;
        lock.insert(projection_name.to_string(), sequence);
        Ok(())
    }

    async fn reset_last_processed_sequence(&self, projection_name: &str) -> Result<(), String> {
        let mut lock = self.sequences.write().await;
        lock.remove(projection_name);
        Ok(())
    }
}

pub struct ProjectionRegistry {
    read_models: HashMap<String, Arc<dyn ReadModel>>,
}

impl ProjectionRegistry {
    pub fn new() -> Self {
        Self {
            read_models: HashMap::new(),
        }
    }

    pub fn register(&mut self, read_model: Arc<dyn ReadModel>) {
        self.read_models
            .insert(read_model.name().to_string(), read_model);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn ReadModel>> {
        self.read_models.get(name).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn ReadModel>> {
        self.read_models.values().cloned().collect()
    }
}

impl Default for ProjectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ProjectionWorker<S: ProjectionStore> {
    registry: Arc<ProjectionRegistry>,
    store: Arc<S>,
}

impl<S: ProjectionStore> ProjectionWorker<S> {
    pub fn new(registry: Arc<ProjectionRegistry>, store: Arc<S>) -> Self {
        Self { registry, store }
    }

    pub async fn process_event(&self, envelope: &EventEnvelope) -> Result<(), String> {
        for read_model in self.registry.all() {
            let name = read_model.name();
            // In a real system, you'd check last processed sequence and maybe deduplicate.
            read_model.handle_event(envelope).await?;
            self.store
                .update_last_processed_sequence(name, envelope.sequence_number)
                .await?;
        }
        Ok(())
    }
}

pub struct ProjectionRebuilder<E: EventStore, S: ProjectionStore> {
    event_store: Arc<E>,
    store: Arc<S>,
    registry: Arc<ProjectionRegistry>,
}

impl<E: EventStore, S: ProjectionStore> ProjectionRebuilder<E, S> {
    pub fn new(event_store: Arc<E>, store: Arc<S>, registry: Arc<ProjectionRegistry>) -> Self {
        Self {
            event_store,
            store,
            registry,
        }
    }

    pub async fn rebuild(&self, aggregate_id: Uuid) -> Result<(), String> {
        let events = self
            .event_store
            .load_event_stream(aggregate_id, None)
            .await?;
        for envelope in events {
            for read_model in self.registry.all() {
                read_model.handle_event(&envelope).await?;
                self.store
                    .update_last_processed_sequence(read_model.name(), envelope.sequence_number)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn rebuild_all(&self) -> Result<(), String> {
        let events = self.event_store.load_all_events().await?;
        for envelope in events {
            for read_model in self.registry.all() {
                read_model.handle_event(&envelope).await?;
                self.store
                    .update_last_processed_sequence(read_model.name(), envelope.sequence_number)
                    .await?;
            }
        }
        Ok(())
    }
}
