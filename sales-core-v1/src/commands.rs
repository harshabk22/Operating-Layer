use crate::actors::Actor;
use crate::assets::Money;
use crate::evidence::Evidence;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Command {
    CreateOpportunity {
        opportunity_id: Uuid,
        customer_id: Uuid,
        name: String,
    },
    QualifyOpportunity {
        opportunity_id: Uuid,
        budget: Money,
        evidence: Vec<Evidence>,
    },
    ProposeSolution {
        opportunity_id: Uuid,
        quote_id: Uuid,
        value: Money,
    },
    RequestApproval {
        opportunity_id: Uuid,
        approval_id: Uuid,
    },
    ApproveQuote {
        opportunity_id: Uuid,
        approval_id: Uuid,
        manager_id: Uuid,
    },
    CloseWon {
        opportunity_id: Uuid,
        contract_evidence: Evidence,
    },
    CloseLost {
        opportunity_id: Uuid,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct CommandEnvelope {
    pub command: Command,
    pub actor: Actor,
}
