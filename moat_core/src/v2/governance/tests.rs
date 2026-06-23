use super::core::*;

#[test]
fn test_budget_consumption() {
    let policy = BudgetPolicy {
        api_quota: 10,
        transition_quota: 5,
        rate_limit: 100,
    };
    let mut budget = Budget::new(policy);

    assert!(budget.consume(QuotaType::Api, 5).is_ok());
    assert!(budget.consume(QuotaType::Api, 6).is_err());

    assert!(budget.consume(QuotaType::Transition, 5).is_ok());
    assert!(budget.consume(QuotaType::Transition, 1).is_err());

    assert!(budget.consume(QuotaType::RateLimit, 50).is_ok());
    assert!(budget.consume(QuotaType::RateLimit, 60).is_err());
}

#[test]
fn test_circuit_tripping() {
    let mut breaker = CircuitBreaker::new(3);
    assert_eq!(breaker.state, CircuitState::Closed);

    assert!(breaker.record_failure().is_none());
    assert!(breaker.record_failure().is_none());

    let incident = breaker.record_failure();
    assert!(incident.is_some());
    assert_eq!(breaker.state, CircuitState::Open);

    assert_eq!(incident.unwrap().description, "Circuit Breaker Tripped");
}

#[test]
fn test_circuit_success_reset() {
    let mut breaker = CircuitBreaker::new(3);
    breaker.record_failure();
    breaker.record_success();
    assert_eq!(breaker.failure_count, 0);

    breaker.state = CircuitState::HalfOpen;
    breaker.record_success();
    assert_eq!(breaker.state, CircuitState::Closed);
    assert_eq!(breaker.failure_count, 0);

    breaker.record_failure();
    breaker.reset();
    assert_eq!(breaker.state, CircuitState::Closed);
    assert_eq!(breaker.failure_count, 0);
}

#[test]
fn test_incident_driven_pause_resume() {
    let controls = OperationalControls::new();
    assert!(!controls.is_paused());

    controls.report_incident(Incident {
        severity: 1,
        description: "Critical failure".to_string(),
    });

    assert!(controls.is_paused());

    controls.resume();
    assert!(!controls.is_paused());
}

#[test]
fn test_operational_controls_no_pause_on_low_severity() {
    let controls = OperationalControls::new();
    controls.report_incident(Incident {
        severity: 0,
        description: "Minor issue".to_string(),
    });
    assert!(!controls.is_paused());
}
