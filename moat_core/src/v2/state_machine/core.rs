use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum State {
    Draft,
    UnderReview,
    Approved,
    Rejected,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Transition {
    pub from: State,
    pub to: State,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub id: String,
    pub is_valid: bool,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiver {
    pub constraint_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceInvalidated {
    pub evidence_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub id: String,
    pub state: State,
    pub evidence: HashMap<String, Evidence>,
    pub waivers: Vec<Waiver>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TransitionTo(State),
    ProvideEvidence(Evidence),
    GrantWaiver(Waiver),
    InvalidateEvidence(EvidenceInvalidated),
}

pub trait Constraint {
    fn id(&self) -> &str;
    fn evaluate(&self, case: &Case) -> Result<(), String>;
}

pub struct RequiredEvidenceConstraint {
    pub id: String,
    pub evidence_id: String,
}

impl Constraint for RequiredEvidenceConstraint {
    fn id(&self) -> &str {
        &self.id
    }

    fn evaluate(&self, case: &Case) -> Result<(), String> {
        // Waiver Processing
        if case.waivers.iter().any(|w| w.constraint_id == self.id) {
            return Ok(());
        }

        // Evidence Validation
        if let Some(ev) = case.evidence.get(&self.evidence_id) {
            if ev.is_valid {
                return Ok(());
            } else {
                return Err(format!("Evidence {} is invalid", self.evidence_id));
            }
        }

        Err(format!("Missing evidence {}", self.evidence_id))
    }
}

pub struct StateMachine {
    pub allowed_transitions: Vec<Transition>,
    pub constraints: HashMap<Transition, Vec<Box<dyn Constraint>>>,
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            allowed_transitions: Vec::new(),
            constraints: HashMap::new(),
        }
    }

    pub fn register_transition(&mut self, from: State, to: State) {
        self.allowed_transitions.push(Transition { from, to });
    }

    pub fn add_constraint(&mut self, transition: Transition, constraint: Box<dyn Constraint>) {
        self.constraints
            .entry(transition)
            .or_default()
            .push(constraint);
    }

    pub fn process(&self, case: &Case, event: Event) -> Result<Case, String> {
        let mut new_case = case.clone();

        match event {
            Event::TransitionTo(target_state) => {
                let transition = Transition {
                    from: new_case.state.clone(),
                    to: target_state.clone(),
                };

                // Transition Validation
                if !self.allowed_transitions.contains(&transition) {
                    return Err(format!(
                        "Transition from {:?} to {:?} is not allowed",
                        transition.from, transition.to
                    ));
                }

                // Constraint Evaluation
                if let Some(constraints) = self.constraints.get(&transition) {
                    for constraint in constraints {
                        constraint.evaluate(&new_case)?;
                    }
                }

                new_case.state = target_state;
            }
            Event::ProvideEvidence(evidence) => {
                // Evidence Validation
                if !evidence.is_valid {
                    return Err("Cannot provide invalid evidence directly".to_string());
                }
                new_case.evidence.insert(evidence.id.clone(), evidence);
            }
            Event::GrantWaiver(waiver) => {
                new_case.waivers.push(waiver);
            }
            Event::InvalidateEvidence(invalidated) => {
                // Evidence Invalidated Flow
                if let Some(ev) = new_case.evidence.get_mut(&invalidated.evidence_id) {
                    ev.is_valid = false;
                } else {
                    return Err("Cannot invalidate non-existent evidence".to_string());
                }

                if new_case.state == State::Approved {
                    new_case.state = State::Suspended;
                }
            }
        }

        Ok(new_case)
    }
}
