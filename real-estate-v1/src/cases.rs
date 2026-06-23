use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CaseStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyInquiry {
    pub id: Uuid,
    pub property_id: Uuid,
    pub buyer_id: Uuid,
    pub status: CaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyVisit {
    pub id: Uuid,
    pub property_id: Uuid,
    pub agent_id: Uuid,
    pub buyer_id: Uuid,
    pub status: CaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyOffer {
    pub id: Uuid,
    pub property_id: Uuid,
    pub buyer_id: Uuid,
    pub amount: u64,
    pub status: CaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyNegotiation {
    pub id: Uuid,
    pub offer_id: Uuid,
    pub status: CaseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDeal {
    pub id: Uuid,
    pub property_id: Uuid,
    pub buyer_id: Uuid,
    pub seller_id: Uuid,
    pub final_price: u64,
    pub status: CaseStatus,
}
