use crate::assets::Money;
use crate::evidence::Evidence;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventData {
    OpportunityCreated {
        customer_id: Uuid,
        name: String,
    },
    OpportunityQualified {
        budget: Money,
        evidence: Vec<Evidence>,
    },
    SolutionProposed {
        quote_id: Uuid,
        value: Money,
    },
    ApprovalRequested {
        approval_id: Uuid,
    },
    QuoteApproved {
        approval_id: Uuid,
        manager_id: Uuid,
    },
    OpportunityClosedWon {
        contract_evidence: Evidence,
    },
    OpportunityClosedLost {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub version: u64,
    pub actor_id: Uuid,
    pub data: EventData,
}
