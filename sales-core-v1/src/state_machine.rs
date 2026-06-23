use crate::actors::Role;
use crate::assets::{Money, Opportunity};
use crate::commands::{Command, CommandEnvelope};
use crate::constraints::{Constraint, RequireNda};
use crate::error::SalesError;
use crate::events::{Event, EventData};
use crate::evidence::Evidence;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpportunityState {
    New,
    Qualified,
    ProposalActive,
    PendingApproval,
    Approved,
    ClosedWon,
    ClosedLost,
}

#[derive(Debug, Clone)]
pub struct OpportunityAggregate {
    pub id: Uuid,
    pub state: OpportunityState,
    pub version: u64,
    pub opportunity_details: Option<Opportunity>,
    pub evidence: Vec<Evidence>,
    pub current_quote_value: Option<Money>,
}

impl Default for OpportunityAggregate {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            state: OpportunityState::New,
            version: 0,
            opportunity_details: None,
            evidence: Vec::new(),
            current_quote_value: None,
        }
    }
}

impl OpportunityAggregate {
    pub fn apply(&mut self, event: &Event) {
        self.version = event.version;
        self.id = event.aggregate_id;

        match &event.data {
            EventData::OpportunityCreated { customer_id, name } => {
                self.state = OpportunityState::New;
                self.opportunity_details = Some(Opportunity {
                    id: self.id,
                    customer_id: *customer_id,
                    value: None,
                    name: name.clone(),
                });
            }
            EventData::OpportunityQualified { budget, evidence } => {
                self.state = OpportunityState::Qualified;
                if let Some(details) = &mut self.opportunity_details {
                    details.value = Some(budget.clone());
                }
                self.evidence.extend(evidence.clone());
            }
            EventData::SolutionProposed { quote_id: _, value } => {
                self.state = OpportunityState::ProposalActive;
                self.current_quote_value = Some(value.clone());
            }
            EventData::ApprovalRequested { approval_id: _ } => {
                self.state = OpportunityState::PendingApproval;
            }
            EventData::QuoteApproved {
                approval_id: _,
                manager_id: _,
            } => {
                self.state = OpportunityState::Approved;
            }
            EventData::OpportunityClosedWon { contract_evidence } => {
                self.state = OpportunityState::ClosedWon;
                self.evidence.push(contract_evidence.clone());
            }
            EventData::OpportunityClosedLost { reason: _ } => {
                self.state = OpportunityState::ClosedLost;
            }
        }
    }

    pub fn apply_all(&mut self, events: &[Event]) {
        for event in events {
            self.apply(event);
        }
    }

    pub fn handle(&self, envelope: CommandEnvelope) -> Result<Vec<Event>, SalesError> {
        let actor = envelope.actor;
        let command = envelope.command;
        let next_version = self.version + 1;
        let timestamp = chrono::Utc::now();

        match command {
            Command::CreateOpportunity {
                opportunity_id,
                customer_id,
                name,
            } => {
                if self.version > 0 {
                    return Err(SalesError::InvalidStateTransition(
                        "Any".to_string(),
                        "New".to_string(),
                    ));
                }
                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: opportunity_id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::OpportunityCreated { customer_id, name },
                }])
            }
            Command::QualifyOpportunity {
                opportunity_id: _,
                budget,
                evidence,
            } => {
                if self.state != OpportunityState::New {
                    return Err(SalesError::InvalidStateTransition(
                        format!("{:?}", self.state),
                        "Qualified".to_string(),
                    ));
                }
                // Constraints check
                let mut combined_evidence = self.evidence.clone();
                combined_evidence.extend(evidence.clone());
                let nda_constraint = RequireNda;
                nda_constraint.check(&combined_evidence)?;

                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: self.id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::OpportunityQualified { budget, evidence },
                }])
            }
            Command::ProposeSolution {
                opportunity_id: _,
                quote_id,
                value,
            } => {
                if self.state != OpportunityState::Qualified
                    && self.state != OpportunityState::Approved
                {
                    return Err(SalesError::InvalidStateTransition(
                        format!("{:?}", self.state),
                        "ProposalActive".to_string(),
                    ));
                }
                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: self.id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::SolutionProposed { quote_id, value },
                }])
            }
            Command::RequestApproval {
                opportunity_id: _,
                approval_id,
            } => {
                if self.state != OpportunityState::ProposalActive {
                    return Err(SalesError::InvalidStateTransition(
                        format!("{:?}", self.state),
                        "PendingApproval".to_string(),
                    ));
                }
                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: self.id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::ApprovalRequested { approval_id },
                }])
            }
            Command::ApproveQuote {
                opportunity_id: _,
                approval_id,
                manager_id,
            } => {
                if self.state != OpportunityState::PendingApproval {
                    return Err(SalesError::InvalidStateTransition(
                        format!("{:?}", self.state),
                        "Approved".to_string(),
                    ));
                }
                if actor.role != Role::SalesManager {
                    return Err(SalesError::Unauthorized(
                        "Only SalesManager can approve".to_string(),
                    ));
                }
                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: self.id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::QuoteApproved {
                        approval_id,
                        manager_id,
                    },
                }])
            }
            Command::CloseWon {
                opportunity_id: _,
                contract_evidence,
            } => {
                if self.state != OpportunityState::ProposalActive
                    && self.state != OpportunityState::Approved
                {
                    return Err(SalesError::InvalidStateTransition(
                        format!("{:?}", self.state),
                        "ClosedWon".to_string(),
                    ));
                }
                Ok(vec![Event {
                    event_id: Uuid::new_v4(),
                    aggregate_id: self.id,
                    timestamp,
                    version: next_version,
                    actor_id: actor.id,
                    data: EventData::OpportunityClosedWon { contract_evidence },
                }])
            }
            Command::CloseLost {
                opportunity_id: _,
                reason,
            } => Ok(vec![Event {
                event_id: Uuid::new_v4(),
                aggregate_id: self.id,
                timestamp,
                version: next_version,
                actor_id: actor.id,
                data: EventData::OpportunityClosedLost { reason },
            }]),
        }
    }
}
