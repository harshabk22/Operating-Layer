#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_assignments,
    unused_must_use
)]
use super::connector::ChannelConnector;
use super::db as comm_db;
use super::mock_connector::MockConnector;
use super::registry::ConnectorRegistry;
use super::types::{ChannelType, ConnectorEvent, InteractionMessage, MessageDirection};
use crate::agent::injector::InjectorConfig;
use crate::agent::interaction_orchestrator::Orchestrator;
use crate::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
use crate::agent::worker::WorkerAgent;
use crate::business::db as biz_db;
use crate::business::escalation;
use crate::business::models::{
    AuthorizationStatus, EngagementStatus, OpportunityStatus, ThreadStatus,
};
use crate::business::runtime::CustomerInteractionRuntime;
use crate::business::workflow;
use crate::integration::external_api::ExternalApiConfig;
use crate::memory::meta_brain::MetaBrain;
use crate::planner::WorkerConfig;
use crate::planner::{PlannerAgent, PlannerConfig};
use crate::MoatResult;
use rusqlite::Connection;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

struct StubProvider {
    response: Arc<std::sync::Mutex<Option<(String, u32)>>>,
}

impl StubProvider {
    fn new(xml: &str) -> Self {
        Self {
            response: Arc::new(std::sync::Mutex::new(Some((xml.to_string(), 42)))),
        }
    }
}

impl LlmProvider for StubProvider {
    fn chat_complete<'a>(
        &'a self,
        _model: &'a str,
        _messages: Vec<LlmMessage>,
        _tools: Option<Vec<crate::agent::interaction_orchestrator::LlmTool>>,
        _max_tokens: usize,
        _temperature: f32,
        _response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        Box::pin(async move {
            let (text, tokens) = self
                .response
                .lock()
                .expect("lock stub response")
                .take()
                .expect("stub response");
            Ok(LlmResponse {
                content: Some(text),
                tool_calls: None,
                tokens,
                structured: None,
            })
        })
    }
}

struct TestProviderFactory {
    provider: Arc<dyn LlmProvider>,
}

impl crate::integration::external_api::ProviderFactory for TestProviderFactory {
    fn provider_for(&self, _api_provider: &str) -> MoatResult<Arc<dyn LlmProvider>> {
        Ok(Arc::clone(&self.provider))
    }

    fn routed_provider(
        &self,
        _subsystem: &str,
        requested_model: &str,
    ) -> MoatResult<(Arc<dyn LlmProvider>, String)> {
        Ok((Arc::clone(&self.provider), requested_model.to_string()))
    }

    fn invalidate_cache(&self) {}

    fn has_stored_key(&self, _api_provider: &str) -> MoatResult<bool> {
        Ok(false)
    }

    fn set_provider_key(&self, _api_provider: &str, _api_key: &str) -> MoatResult<()> {
        Ok(())
    }

    fn remove_provider_key(&self, _api_provider: &str) -> MoatResult<()> {
        Ok(())
    }

    fn set_default_provider(&self, _provider: &str) {}

    fn get_default_provider(&self) -> String {
        "google".to_string()
    }

    fn provider_health<'a>(
        &'a self,
        _api_provider: &'a str,
    ) -> BoxFuture<'a, MoatResult<crate::config::ProviderHealth>> {
        Box::pin(async { Ok(crate::config::ProviderHealth::Missing) })
    }

    fn provider_variant_health<'a>(
        &'a self,
        _api_provider: &'a str,
        _variant: Option<&'a str>,
    ) -> BoxFuture<'a, MoatResult<crate::config::ProviderHealth>> {
        Box::pin(async { Ok(crate::config::ProviderHealth::Missing) })
    }
}

#[tokio::test]
async fn test_mock_connector_receives_message() {
    let mock = MockConnector::new();
    let msg = InteractionMessage::new(
        "biz1".to_string(),
        "thread1".to_string(),
        "cust1".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Inbound,
        "Hello".to_string(),
    );
    let _res: Result<(), crate::MoatError> = mock.receive_message(msg.clone()).await;
    _res.unwrap();
    let received = mock.get_received_messages();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0], msg);
}

#[tokio::test]
async fn test_mock_connector_sends_message() {
    let mock = MockConnector::new();
    let msg = InteractionMessage::new(
        "biz1".to_string(),
        "thread1".to_string(),
        "cust1".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Outbound,
        "Hi".to_string(),
    );
    mock.send_message(msg.clone()).await.unwrap();
    let sent = mock.get_sent_messages();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0], msg);
}

#[tokio::test]
async fn test_thread_open_close() {
    let mock = MockConnector::new();
    mock.open_thread("biz1", "thread1", "cust1", ChannelType::Whatsapp)
        .await
        .unwrap();
    mock.close_thread("biz1", "thread1", ChannelType::Whatsapp)
        .await
        .unwrap();

    let open = mock.open_threads.lock().unwrap();
    let closed = mock.closed_threads.lock().unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(closed.len(), 1);
    assert_eq!(
        open[0],
        (
            "biz1".to_string(),
            "thread1".to_string(),
            "cust1".to_string(),
            ChannelType::Whatsapp
        )
    );
    assert_eq!(
        closed[0],
        (
            "biz1".to_string(),
            "thread1".to_string(),
            ChannelType::Whatsapp
        )
    );
}

#[tokio::test]
async fn test_connector_registry_dispatch() {
    let conn = Connection::open_in_memory().unwrap();
    comm_db::prepare_tables(&conn).unwrap();

    let registry = ConnectorRegistry::new();
    let mock = Arc::new(MockConnector::new());
    registry.register("whatsapp", mock.clone()).await;

    let msg = InteractionMessage::new(
        "biz1".to_string(),
        "thread1".to_string(),
        "cust1".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Inbound,
        "Hello registry".to_string(),
    );

    let event = ConnectorEvent::MessageReceived(msg.clone());
    comm_db::persist_event(&conn, &event, "whatsapp").unwrap();
    registry.dispatch_event(event, "whatsapp").await.unwrap();

    let received = mock.get_received_messages();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0], msg);

    let events = comm_db::list_events(&conn, "thread1").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "message_received");
    assert_eq!(events[0].1, "whatsapp");
}

#[tokio::test]
async fn test_runtime_processes_message_flow() {
    let conn = Connection::open_in_memory().unwrap();
    biz_db::prepare_tables(&conn).unwrap();
    comm_db::prepare_tables(&conn).unwrap();

    let contact = crate::business::models::ExternalContact {
        id: "contact1".to_string(),
        name: Some("Test Contact".to_string()),
        email: None,
        phone: None,
        created_at: 0,
        updated_at: 0,
    };
    crate::business::db::insert_contact(&conn, &contact).unwrap();
    crate::business::identity::link_identity(&conn, "contact1", "whatsapp", "+12345").unwrap();

    let registry = Arc::new(ConnectorRegistry::new());
    let mock = Arc::new(MockConnector::new());
    registry.register("whatsapp", mock.clone()).await;

    let db_arc = Arc::new(AsyncMutex::new(conn));
    let provider = Arc::new(StubProvider::new("<response>done</response>"));
    let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));
    let planner = PlannerAgent::new(
        PlannerConfig {
            model: "stub".into(),
            max_subtasks: 5,
            temperature: 0.0,
            max_tokens: 128,
        },
        provider.clone(),
    );
    let worker = WorkerAgent::new(
        WorkerConfig {
            model: "stub".into(),
            command_timeout: Duration::from_secs(5),
            allow_high_risk: true,
            external_apis: ExternalApiConfig::default(),
        },
        Arc::clone(&meta_brain),
        Arc::clone(&db_arc),
    )
    .unwrap();

    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&db_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let engine = Arc::new(CustomerInteractionRuntime::new(
        orchestrator,
        Arc::clone(&db_arc),
        registry.clone(),
    ));
    let _workers = engine.start_background_workers();

    let msg = InteractionMessage::new(
        "biz1".to_string(),
        "thread1".to_string(),
        "+12345".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Inbound,
        "I want to buy a plan".to_string(),
    );

    let reply = engine.handle_message(msg).await.unwrap();

    // Sleep to let background outbox worker dispatch the message
    tokio::time::sleep(Duration::from_millis(200)).await;

    let sent = mock.get_sent_messages();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].body.contains("done") || sent[0].body.contains("shortly"));

    let conn_guard = db_arc.lock().await;
    let events = comm_db::list_events(&conn_guard, &reply.thread_id).unwrap();
    assert!(events.iter().any(|e| e.0 == "message_received"));
    assert!(events.iter().any(|e| e.0 == "message_sent"));

    let opp = biz_db::get_opportunity_by_thread(&conn_guard, &reply.thread_id)
        .unwrap()
        .unwrap();
    assert_eq!(opp.status, OpportunityStatus::Qualified);
}

#[tokio::test]
async fn test_authorization_request_flow() {
    let conn = Connection::open_in_memory().unwrap();
    biz_db::prepare_tables(&conn).unwrap();
    comm_db::prepare_tables(&conn).unwrap();

    let contact = crate::business::identity::resolve_contact(&conn, "whatsapp", "+12345").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    let eng = workflow::propose_engagement(
        &conn,
        &contact.id,
        &thread.id,
        "Demo Call",
        1700000000,
        1700003600,
    )
    .unwrap();

    let req = workflow::request_authorization(&conn, &eng.id, "agent").unwrap();
    assert_eq!(req.status, AuthorizationStatus::Pending);

    let events = comm_db::list_events(&conn, &thread.id).unwrap();
    assert!(events.iter().any(|e| e.0 == "authorization_requested"));
}

#[tokio::test]
async fn test_escalation_flow() {
    let conn = Connection::open_in_memory().unwrap();
    biz_db::prepare_tables(&conn).unwrap();
    comm_db::prepare_tables(&conn).unwrap();

    let contact = crate::business::identity::resolve_contact(&conn, "whatsapp", "+12345").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    escalation::escalate_thread(&conn, &thread.id, "unsupported_inquiry").unwrap();

    let events = comm_db::list_events(&conn, &thread.id).unwrap();
    assert!(events.iter().any(|e| e.0 == "escalation_requested"));
}
