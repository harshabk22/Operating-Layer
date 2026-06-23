use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceType {
    NdaSigned,
    BudgetConfirmed,
    TechnicalValidation,
    ContractSigned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub id: Uuid,
    pub evidence_type: EvidenceType,
    pub document_reference: String,
    pub verified_at: DateTime<Utc>,
    pub verified_by: Uuid,
}
