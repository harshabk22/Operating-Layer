use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInventoryView {
    pub property_id: Uuid,
    pub available_units: u32,
    pub total_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveVisitsView {
    pub visit_id: Uuid,
    pub property_id: Uuid,
    pub agent_id: Uuid,
    pub scheduled_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferPipelineView {
    pub offer_id: Uuid,
    pub property_id: Uuid,
    pub amount: u64,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DealPipelineView {
    pub deal_id: Uuid,
    pub property_id: Uuid,
    pub final_price: u64,
    pub state: String,
}
