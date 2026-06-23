pub mod actions;
pub mod actors;
pub mod approvals;
pub mod assets;
pub mod commands;
pub mod constraints;
pub mod error;
pub mod events;
pub mod evidence;
pub mod read_models;
pub mod state_machine;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::{Actor, Role};
    use crate::assets::Money;
    use crate::commands::{Command, CommandEnvelope};
    use crate::evidence::{Evidence, EvidenceType};
    use crate::state_machine::{OpportunityAggregate, OpportunityState};
    use uuid::Uuid;

    #[test]
    fn test_opportunity_lifecycle() {
        let rep = Actor {
            id: Uuid::new_v4(),
            role: Role::SalesRep,
            name: "Alice".to_string(),
        };

        let opportunity_id = Uuid::new_v4();
        let customer_id = Uuid::new_v4();

        let mut aggregate = OpportunityAggregate::default();

        let create_cmd = CommandEnvelope {
            command: Command::CreateOpportunity {
                opportunity_id,
                customer_id,
                name: "Big Deal".to_string(),
            },
            actor: rep.clone(),
        };

        let events = aggregate.handle(create_cmd).expect("Should create");
        aggregate.apply_all(&events);
        assert_eq!(aggregate.state, OpportunityState::New);

        let nda_evidence = Evidence {
            id: Uuid::new_v4(),
            evidence_type: EvidenceType::NdaSigned,
            document_reference: "doc-123".to_string(),
            verified_at: chrono::Utc::now(),
            verified_by: rep.id,
        };

        let qualify_cmd = CommandEnvelope {
            command: Command::QualifyOpportunity {
                opportunity_id,
                budget: Money {
                    amount: 100000,
                    currency: "USD".to_string(),
                },
                evidence: vec![nda_evidence],
            },
            actor: rep.clone(),
        };

        let events = aggregate.handle(qualify_cmd).expect("Should qualify");
        aggregate.apply_all(&events);
        assert_eq!(aggregate.state, OpportunityState::Qualified);
    }
}
