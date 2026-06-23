use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SalesError {
    #[error("Invalid state transition from {0} to {1}")]
    InvalidStateTransition(String, String),
    #[error("Constraint not met: {0}")]
    ConstraintNotMet(String),
    #[error("Missing evidence: {0}")]
    MissingEvidence(String),
    #[error("Unauthorized action by actor: {0}")]
    Unauthorized(String),
}
