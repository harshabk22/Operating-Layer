use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaType {
    Api,
    Transition,
    RateLimit,
}

#[derive(Debug, Clone)]
pub struct BudgetPolicy {
    pub api_quota: u32,
    pub transition_quota: u32,
    pub rate_limit: u32,
}

#[derive(Debug, Clone)]
pub struct Budget {
    pub policy: BudgetPolicy,
    pub api_consumed: u32,
    pub transition_consumed: u32,
    pub rate_consumed: u32,
}

impl Budget {
    pub fn new(policy: BudgetPolicy) -> Self {
        Self {
            policy,
            api_consumed: 0,
            transition_consumed: 0,
            rate_consumed: 0,
        }
    }

    pub fn consume(&mut self, qtype: QuotaType, amount: u32) -> Result<BudgetConsumption, String> {
        match qtype {
            QuotaType::Api => {
                if self.api_consumed + amount > self.policy.api_quota {
                    return Err("API quota exceeded".to_string());
                }
                self.api_consumed += amount;
            }
            QuotaType::Transition => {
                if self.transition_consumed + amount > self.policy.transition_quota {
                    return Err("Transition quota exceeded".to_string());
                }
                self.transition_consumed += amount;
            }
            QuotaType::RateLimit => {
                if self.rate_consumed + amount > self.policy.rate_limit {
                    return Err("Rate limit exceeded".to_string());
                }
                self.rate_consumed += amount;
            }
        }
        Ok(BudgetConsumption {
            quota_type: qtype,
            amount,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BudgetConsumption {
    pub quota_type: QuotaType,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub state: CircuitState,
    pub failure_threshold: u32,
    pub failure_count: u32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_threshold,
            failure_count: 0,
        }
    }

    pub fn record_failure(&mut self) -> Option<Incident> {
        self.failure_count += 1;
        if self.failure_count >= self.failure_threshold && self.state == CircuitState::Closed {
            self.state = CircuitState::Open;
            return Some(Incident {
                severity: 1,
                description: "Circuit Breaker Tripped".to_string(),
            });
        }
        None
    }

    pub fn record_success(&mut self) {
        if self.state == CircuitState::HalfOpen {
            self.state = CircuitState::Closed;
            self.failure_count = 0;
        } else if self.state == CircuitState::Closed {
            self.failure_count = 0;
        }
    }

    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
    }
}

#[derive(Debug, Clone)]
pub struct Incident {
    pub severity: u8,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionState {
    Running,
    Paused,
}

pub struct OperationalControls {
    pub state: Mutex<ExecutionState>,
    pub incidents: Mutex<Vec<Incident>>,
}

impl Default for OperationalControls {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationalControls {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ExecutionState::Running),
            incidents: Mutex::new(Vec::new()),
        }
    }

    pub fn pause(&self) {
        let mut state = self.state.lock().unwrap();
        *state = ExecutionState::Paused;
    }

    pub fn resume(&self) {
        let mut state = self.state.lock().unwrap();
        *state = ExecutionState::Running;
    }

    pub fn report_incident(&self, incident: Incident) {
        let mut incidents = self.incidents.lock().unwrap();
        if incident.severity >= 1 {
            self.pause();
        }
        incidents.push(incident);
    }

    pub fn is_paused(&self) -> bool {
        *self.state.lock().unwrap() == ExecutionState::Paused
    }
}
