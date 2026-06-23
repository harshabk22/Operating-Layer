use crate::error::SalesError;
use crate::evidence::{Evidence, EvidenceType};

pub trait Constraint {
    fn check(&self, evidence: &[Evidence]) -> Result<(), SalesError>;
}

pub struct RequireNda;
impl Constraint for RequireNda {
    fn check(&self, evidence: &[Evidence]) -> Result<(), SalesError> {
        if evidence
            .iter()
            .any(|e| e.evidence_type == EvidenceType::NdaSigned)
        {
            Ok(())
        } else {
            Err(SalesError::ConstraintNotMet(
                "NDA must be signed".to_string(),
            ))
        }
    }
}

pub struct RequireBudget;
impl Constraint for RequireBudget {
    fn check(&self, evidence: &[Evidence]) -> Result<(), SalesError> {
        if evidence
            .iter()
            .any(|e| e.evidence_type == EvidenceType::BudgetConfirmed)
        {
            Ok(())
        } else {
            Err(SalesError::ConstraintNotMet(
                "Budget must be confirmed".to_string(),
            ))
        }
    }
}
