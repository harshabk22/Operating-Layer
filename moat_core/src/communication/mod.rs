pub mod connector;
pub mod db;
pub mod registry;
pub mod types;
pub mod whatsapp;
pub mod whatsapp_baileys;

pub use connector::ChannelConnector;
pub use registry::ConnectorRegistry;
pub use types::{ChannelType, ConnectorEvent, InteractionMessage, MessageDirection};

#[cfg(test)]
pub mod tests;

#[cfg(test)]
pub mod mock_connector;
