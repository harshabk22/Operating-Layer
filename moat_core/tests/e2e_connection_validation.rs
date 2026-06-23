use async_trait::async_trait;
use moat_core::v2::connection::{Connection, ConnectionRouter, EgressRequest, IngressEvent};
use moat_core::v2::governance::core::{CircuitBreaker, CircuitState, OperationalControls};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// ==========================================
// MockNotificationConnection
// ==========================================
pub struct MockNotificationConnection {
    id: String,
    pub sent_requests: Arc<Mutex<Vec<EgressRequest>>>,
    pub circuit_breaker: Arc<Mutex<CircuitBreaker>>,
    pub fail_requests: Arc<Mutex<bool>>,
    pub processed_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
    pub controls: Arc<OperationalControls>,
}

impl MockNotificationConnection {
    pub fn new(id: &str, controls: Arc<OperationalControls>) -> Self {
        Self {
            id: id.to_string(),
            sent_requests: Arc::new(Mutex::new(Vec::new())),
            circuit_breaker: Arc::new(Mutex::new(CircuitBreaker::new(3))), // Threshold 3
            fail_requests: Arc::new(Mutex::new(false)),
            processed_ids: Arc::new(Mutex::new(HashSet::new())),
            controls,
        }
    }
}

#[async_trait]
impl Connection for MockNotificationConnection {
    fn id(&self) -> &str {
        &self.id
    }

    async fn send(&self, request: EgressRequest) -> Result<(), String> {
        let mut cb = self.circuit_breaker.lock().unwrap();
        if cb.state == CircuitState::Open {
            return Err("Circuit breaker is open".to_string());
        }

        let fail = *self.fail_requests.lock().unwrap();
        if fail {
            if let Some(incident) = cb.record_failure() {
                self.controls.report_incident(incident.clone());
                return Err(format!("INCIDENT: {}", incident.description));
            }
            return Err("Connection failed".to_string());
        }

        cb.record_success();

        let mut processed = self.processed_ids.lock().unwrap();
        if processed.contains(&request.payload) {
            // Idempotency: return OK if already processed payload
            return Ok(());
        }
        processed.insert(request.payload.clone());

        self.sent_requests.lock().unwrap().push(request);
        Ok(())
    }
}

// ==========================================
// MockSchedulingConnection
// ==========================================
pub struct MockSchedulingConnection {
    id: String,
    pub sent_requests: Arc<Mutex<Vec<EgressRequest>>>,
    pub processed_ids: Arc<Mutex<HashSet<Vec<u8>>>>,
}

impl MockSchedulingConnection {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            sent_requests: Arc::new(Mutex::new(Vec::new())),
            processed_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[async_trait]
impl Connection for MockSchedulingConnection {
    fn id(&self) -> &str {
        &self.id
    }

    async fn send(&self, request: EgressRequest) -> Result<(), String> {
        let mut processed = self.processed_ids.lock().unwrap();
        if processed.contains(&request.payload) {
            return Ok(());
        }
        processed.insert(request.payload.clone());

        self.sent_requests.lock().unwrap().push(request);
        Ok(())
    }
}

// ==========================================
// Tests
// ==========================================

#[tokio::test]
async fn test_ingress_egress_flow() {
    let controls = Arc::new(OperationalControls::new());
    let router = ConnectionRouter::new();

    let notif_conn = Arc::new(MockNotificationConnection::new("notif-1", controls.clone()));
    let sched_conn = Arc::new(MockSchedulingConnection::new("sched-1"));

    router
        .add_connection(notif_conn.clone() as Arc<dyn Connection>)
        .await;
    router
        .add_connection(sched_conn.clone() as Arc<dyn Connection>)
        .await;

    // Test Ingress
    let ingress = IngressEvent::new("notif-1", b"incoming-payload".to_vec());
    assert!(router.route_ingress(ingress).await.is_ok());

    // Test Egress
    let egress1 = EgressRequest::new("notif-1", b"notify-user".to_vec());
    let egress2 = EgressRequest::new("sched-1", b"schedule-job".to_vec());

    assert!(router.route_egress(egress1).await.is_ok());
    assert!(router.route_egress(egress2).await.is_ok());

    assert_eq!(notif_conn.sent_requests.lock().unwrap().len(), 1);
    assert_eq!(sched_conn.sent_requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_idempotency() {
    let controls = Arc::new(OperationalControls::new());
    let router = ConnectionRouter::new();
    let notif_conn = Arc::new(MockNotificationConnection::new("notif-2", controls.clone()));

    router
        .add_connection(notif_conn.clone() as Arc<dyn Connection>)
        .await;

    let egress1 = EgressRequest::new("notif-2", b"idempotent-payload".to_vec());
    let egress2 = EgressRequest::new("notif-2", b"idempotent-payload".to_vec());

    assert!(router.route_egress(egress1).await.is_ok());
    // Send same payload again
    assert!(router.route_egress(egress2).await.is_ok());

    // Should only be recorded once
    assert_eq!(notif_conn.sent_requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_retries() {
    let controls = Arc::new(OperationalControls::new());
    let router = ConnectionRouter::new();
    let notif_conn = Arc::new(MockNotificationConnection::new("notif-3", controls.clone()));

    router
        .add_connection(notif_conn.clone() as Arc<dyn Connection>)
        .await;

    *notif_conn.fail_requests.lock().unwrap() = true;

    let egress = EgressRequest::new("notif-3", b"retry-payload".to_vec());

    let mut attempts = 0;
    let mut success = false;

    for _ in 0..3 {
        attempts += 1;
        if router.route_egress(egress.clone()).await.is_ok() {
            success = true;
            break;
        }
        // fix after 2 failures
        if attempts == 2 {
            *notif_conn.fail_requests.lock().unwrap() = false;
        }
    }

    assert!(success);
    assert_eq!(attempts, 3);
    assert_eq!(notif_conn.sent_requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_circuit_breaker_and_incident_generation() {
    let controls = Arc::new(OperationalControls::new());
    let router = ConnectionRouter::new();
    let notif_conn = Arc::new(MockNotificationConnection::new("notif-4", controls.clone()));

    router
        .add_connection(notif_conn.clone() as Arc<dyn Connection>)
        .await;

    *notif_conn.fail_requests.lock().unwrap() = true;

    for i in 0..4 {
        let payload = format!("payload-{}", i).into_bytes();
        let egress = EgressRequest::new("notif-4", payload);
        let _ = router.route_egress(egress).await;
    }

    let cb = notif_conn.circuit_breaker.lock().unwrap();
    assert_eq!(cb.state, CircuitState::Open);

    let incidents = controls.incidents.lock().unwrap();
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].description, "Circuit Breaker Tripped");

    // Severity 1 causes a pause
    assert!(controls.is_paused());
}
