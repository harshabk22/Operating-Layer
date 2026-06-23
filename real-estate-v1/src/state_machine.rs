use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InquiryState {
    New,
    Responded,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VisitState {
    Scheduled,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OfferState {
    Draft,
    Submitted,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DealState {
    Initiated,
    UnderContract,
    Closed,
    FallenThrough,
}
