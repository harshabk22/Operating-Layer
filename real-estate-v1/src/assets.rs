use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub id: Uuid,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub id: Uuid,
    pub property_id: Uuid,
    pub unit_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyAvailability {
    pub id: Uuid,
    pub unit_id: Uuid,
    pub is_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDocument {
    pub id: Uuid,
    pub property_id: Uuid,
    pub document_url: String,
}
