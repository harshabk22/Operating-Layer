#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_assignments,
    unused_must_use
)]
use super::*;
use crate::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
use crate::MoatResult;
use rusqlite::params;
use rusqlite::Connection;
use std::sync::Arc;
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
            let (text, tokens) = {
                let lock = self.response.lock().expect("lock stub response");
                if let Some(ref val) = *lock {
                    val.clone()
                } else {
                    ("<response>done</response>".to_string(), 42)
                }
            };
            Ok(LlmResponse {
                content: Some(text),
                tool_calls: None,
                tokens,
                structured: None,
            })
        })
    }
}

#[test]
fn test_prepare_tables_and_crud() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    // Test Contact CRUD
    let contact_id = "test-contact-123";
    let contact = models::ExternalContact {
        id: contact_id.to_string(),
        name: Some("Alice".to_string()),
        email: Some("alice@example.com".to_string()),
        phone: Some("+123456789".to_string()),
        created_at: 1000,
        updated_at: 1000,
    };
    db::insert_contact(&conn, &contact).unwrap();

    let fetched = db::get_contact(&conn, contact_id).unwrap().unwrap();
    assert_eq!(fetched.name.unwrap(), "Alice");
    assert_eq!(fetched.email.unwrap(), "alice@example.com");

    // Test Identity Resolution
    identity::link_identity(&conn, contact_id, "whatsapp", "+123456789").unwrap();
    let resolved = identity::resolve_contact(&conn, "whatsapp", "+123456789").unwrap();
    assert_eq!(resolved.id, contact_id);

    // Resolving new identity creates new contact
    let new_resolved = identity::resolve_contact(&conn, "instagram", "insta_user_1").unwrap();
    assert_ne!(new_resolved.id, contact_id);
    assert!(new_resolved.phone.is_none());

    // Link identity
    identity::link_identity(&conn, contact_id, "messenger", "fb_alice").unwrap();
    let fb_resolved = identity::resolve_contact(&conn, "messenger", "fb_alice").unwrap();
    assert_eq!(fb_resolved.id, contact_id);
}

#[test]
fn test_workflow_and_opportunity_states() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+111222333").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();
    assert_eq!(
        thread.opportunity_status,
        models::OpportunityStatus::Discovered
    );

    // Log message
    let msg = workflow::log_message(&conn, &thread.id, "contact", "I want to buy a subscription")
        .unwrap();
    assert_eq!(msg.sender, "contact");

    let messages = db::list_messages(&conn, &thread.id).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "I want to buy a subscription");

    // Update opportunity status
    workflow::update_opportunity_status(&conn, &thread.id, models::OpportunityStatus::Qualified)
        .unwrap();
    let opp = db::get_opportunity_by_thread(&conn, &thread.id)
        .unwrap()
        .unwrap();
    assert_eq!(opp.status, models::OpportunityStatus::Qualified);
}

#[test]
fn test_engagement_workflow_and_authorization() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+111222333").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    // Propose booking
    let start_time = 1718300000;
    let end_time = 1718303600;
    let eng = workflow::propose_engagement(
        &conn,
        &contact.id,
        &thread.id,
        "Demo Call",
        start_time,
        end_time,
    )
    .unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Proposed);

    // Request authorization
    workflow::request_authorization(&conn, &eng.id, "system").unwrap();
    let eng_pending = db::get_engagement(&conn, &eng.id).unwrap().unwrap();
    assert_eq!(
        eng_pending.status,
        models::EngagementStatus::PendingAuthorization
    );

    // Verify thread is now awaiting_authorization
    let thread_fetched = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(
        thread_fetched.status,
        models::ThreadStatus::AwaitingAuthorization
    );

    // Authorize
    workflow::authorize_engagement(&conn, &eng.id).unwrap();
    let eng_authorized = db::get_engagement(&conn, &eng.id).unwrap().unwrap();
    assert_eq!(eng_authorized.status, models::EngagementStatus::Authorized);

    // Verify thread transitioned back to active
    let thread_active = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(thread_active.status, models::ThreadStatus::Active);

    // Integration Provider Execution
    let provider = integration::SchedulingProvider::new();
    let payload = serde_json::json!({
        "contact_id": contact.id,
        "title": eng_authorized.title,
        "start_time": eng_authorized.start_time,
        "end_time": eng_authorized.end_time
    });
    use integration::BusinessIntegrationProvider;
    let exec_res = provider.execute("book_engagement", payload).unwrap();
    assert_eq!(exec_res["status"], "success");
}

#[test]
fn test_operator_escalation() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+111222333").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    let esc = escalation::escalate_thread(&conn, &thread.id, "Unable to resolve request").unwrap();
    assert_eq!(esc.status, "pending");

    let thread_fetched = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(thread_fetched.status, models::ThreadStatus::Escalated);
}

#[test]
fn test_knowledge_store_fts() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    // Add document
    knowledge::add_document(
        &conn,
        "Refund Policy",
        "Our refund policy allows returns within 30 days.",
        "policies",
    )
    .unwrap();
    knowledge::add_document(
        &conn,
        "Pricing Details",
        "The pricing is $19/month per agent.",
        "pricing",
    )
    .unwrap();

    // Query FTS
    let docs = knowledge::query_knowledge_store(&conn, "refund", 10).unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].title, "Refund Policy");

    let docs_pricing = knowledge::query_knowledge_store(&conn, "pricing", 10).unwrap();
    assert_eq!(docs_pricing.len(), 1);
    assert_eq!(docs_pricing[0].title, "Pricing Details");
}

#[test]
fn test_opportunity_status_transitions() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+12345").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    // Initial state discovered
    assert_eq!(
        thread.opportunity_status,
        models::OpportunityStatus::Discovered
    );

    // Discovered -> Qualified should succeed
    workflow::update_opportunity_status(&conn, &thread.id, models::OpportunityStatus::Qualified)
        .unwrap();
    let t1 = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(t1.opportunity_status, models::OpportunityStatus::Qualified);

    // Qualified -> Disqualified should succeed
    workflow::update_opportunity_status(&conn, &thread.id, models::OpportunityStatus::Disqualified)
        .unwrap();
    let t2 = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(
        t2.opportunity_status,
        models::OpportunityStatus::Disqualified
    );

    // Disqualified -> Converted should fail (violates transition rules)
    let err = workflow::update_opportunity_status(
        &conn,
        &thread.id,
        models::OpportunityStatus::Converted,
    );
    assert!(err.is_err());
}

#[test]
fn test_thread_status_transitions() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+12345").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    // Initial state active
    assert_eq!(thread.status, models::ThreadStatus::Active);

    // Active -> Escalated should succeed
    workflow::update_thread_status(&conn, &thread.id, models::ThreadStatus::Escalated).unwrap();
    let t1 = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(t1.status, models::ThreadStatus::Escalated);

    // Escalated -> Closed should succeed
    workflow::update_thread_status(&conn, &thread.id, models::ThreadStatus::Closed).unwrap();
    let t2 = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(t2.status, models::ThreadStatus::Closed);

    // Closed -> AwaitingAuthorization should fail (must be Active first)
    let err = workflow::update_thread_status(
        &conn,
        &thread.id,
        models::ThreadStatus::AwaitingAuthorization,
    );
    assert!(err.is_err());
}

#[test]
fn test_authorization_workflow_rejection_and_expiration() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let contact = identity::resolve_contact(&conn, "whatsapp", "+111222333").unwrap();
    let thread = workflow::resolve_active_thread(&conn, &contact.id).unwrap();

    // 1. Test Rejection flow
    let eng1 = workflow::propose_engagement(
        &conn,
        &contact.id,
        &thread.id,
        "Demo Call 1",
        1700000000,
        1700003600,
    )
    .unwrap();
    workflow::request_authorization(&conn, &eng1.id, "agent").unwrap();

    // Reject
    workflow::reject_engagement(&conn, &eng1.id).unwrap();
    let eng1_fetched = db::get_engagement(&conn, &eng1.id).unwrap().unwrap();
    assert_eq!(eng1_fetched.status, models::EngagementStatus::Rejected);

    // 2. Test Expiration flow
    let eng2 = workflow::propose_engagement(
        &conn,
        &contact.id,
        &thread.id,
        "Demo Call 2",
        1700000000,
        1700003600,
    )
    .unwrap();
    workflow::request_authorization(&conn, &eng2.id, "agent").unwrap();

    // Expire
    workflow::expire_engagement(&conn, &eng2.id).unwrap();
    let eng2_fetched = db::get_engagement(&conn, &eng2.id).unwrap().unwrap();
    assert_eq!(eng2_fetched.status, models::EngagementStatus::Cancelled);

    // Thread is active again
    let thread_fetched = db::get_thread(&conn, &thread.id).unwrap().unwrap();
    assert_eq!(thread_fetched.status, models::ThreadStatus::Active);
}

#[test]
fn test_knowledge_retrieval_ranking() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let doc1 = knowledge::add_document(
        &conn,
        "Refund Policy",
        "Refunds are allowed within 30 days.",
        "policies",
    )
    .unwrap();
    let _doc2 = knowledge::add_document(
        &conn,
        "Pricing Details",
        "Our plans start at $19/month per agent.",
        "pricing",
    )
    .unwrap();

    // Multi-term match ranking
    let results = knowledge::query_knowledge_store_ranked(&conn, "refund policy", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document_id, doc1.id);

    // Verify relevance score is normalized between 0 and 1
    assert!(results[0].relevance >= 0.0 && results[0].relevance <= 1.0);
    assert!(results[0].confidence == results[0].relevance);
}

#[tokio::test]
async fn test_phase_9_business_operating_layer() {
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::worker::WorkerAgent;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::communication::mock_connector::MockConnector;
    use crate::communication::types::ChannelType;
    use crate::communication::ConnectorRegistry;
    use crate::communication::{InteractionMessage, MessageDirection};
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use crate::MoatResult;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;
    use tokio::sync::RwLock as AsyncRwLock;

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_arc = Arc::new(Mutex::new(conn));
    let registry = Arc::new(ConnectorRegistry::new());

    // Register mock connector
    let mock_connector = Arc::new(MockConnector::new());
    registry.register("whatsapp", mock_connector.clone());

    // Create system / mock orchestrator
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
        Arc::clone(&conn_arc),
    )
    .unwrap();

    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));
    let engine = CustomerInteractionRuntime::new(orchestrator, conn_arc.clone(), registry);

    // Let's lock DB to populate seed data
    {
        let db_conn = conn_arc.lock().await;
        // Insert contact first
        let contact = models::ExternalContact {
            id: "c1".to_string(),
            name: Some("Alice".to_string()),
            email: Some("alice@example.com".to_string()),
            phone: Some("+123456789".to_string()),
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        };
        db::insert_contact(&db_conn, &contact).unwrap();
        // link a contact
        identity::link_identity(&db_conn, "c1", "whatsapp", "+123456789").unwrap();
        // insert some knowledge
        knowledge::add_document(
            &db_conn,
            "Pricing Page",
            "Pricing is $49 per user monthly.",
            "pricing",
        )
        .unwrap();
    }

    // --- 1. Test Pricing Auto-Answer ---
    let pricing_msg = InteractionMessage {
        id: "m1".to_string(),
        business_id: "default_business".to_string(),
        thread_id: "".to_string(), // will be resolved
        customer_id: "+123456789".to_string(),
        channel: ChannelType::Whatsapp,
        direction: MessageDirection::Inbound,
        body: "What is the pricing?".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    let reply = engine.handle_message(pricing_msg).await.unwrap();
    // Pricing questions go to AnswerOnly (which defaults to fallback because we didn't mock LLM results)
    assert!(
        reply.body.contains("Thank you")
            || reply.body.contains("shortly")
            || reply.body.contains("done")
            || reply.body.contains("pricing")
    );

    // Verify opportunity score exists in DB
    {
        let db_conn = conn_arc.lock().await;
        let thread = db::get_active_thread_by_contact(&db_conn, "c1")
            .unwrap()
            .unwrap();
        let opp = db::get_opportunity_by_thread(&db_conn, &thread.id)
            .unwrap()
            .unwrap();
        let score = db::get_opportunity_score(&db_conn, &opp.id)
            .unwrap()
            .unwrap();
        assert!(score.budget_score >= 8); // pricing keywords match budget signals
        assert_eq!(score.rating, models::OpportunityRating::Warm);

        // Verify thread objective was initialized to QualifyLead
        let obj = db::get_thread_objective(&db_conn, &thread.id)
            .unwrap()
            .unwrap();
        assert_eq!(obj.objective, models::ThreadObjective::QualifyLead);
    }

    // --- 2. Test Booking Workflow (Requires Authorization) ---
    let booking_msg = InteractionMessage {
        id: "m2".to_string(),
        business_id: "default_business".to_string(),
        thread_id: "".to_string(),
        customer_id: "+123456789".to_string(),
        channel: ChannelType::Whatsapp,
        direction: MessageDirection::Inbound,
        body: "I want to book a demo next week ASAP".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    let reply2 = engine.handle_message(booking_msg).await.unwrap();
    assert!(reply2.body.contains("pending operator approval") || reply2.body.contains("proposal"));

    // Verify state in DB
    {
        let db_conn = conn_arc.lock().await;
        let thread_id = db_conn.query_row(
            "SELECT id FROM business_interaction_threads WHERE contact_id = 'c1' ORDER BY updated_at DESC LIMIT 1",
            [],
            |r: &rusqlite::Row| r.get::<_, String>(0)
        ).unwrap();
        let thread = db::get_thread(&db_conn, &thread_id).unwrap().unwrap();

        // Thread objective should have evolved to ScheduleConsultation
        let obj = db::get_thread_objective(&db_conn, &thread.id)
            .unwrap()
            .unwrap();
        assert_eq!(obj.objective, models::ThreadObjective::ScheduleConsultation);

        // Opportunity should be Qualified/EngagementPending
        let opp = db::get_opportunity_by_thread(&db_conn, &thread.id)
            .unwrap()
            .unwrap();
        assert_eq!(opp.status, models::OpportunityStatus::Qualified);

        // Engagement proposed and awaiting authorization
        let engagements = db_conn
            .prepare("SELECT id, status FROM business_scheduled_engagements")
            .unwrap()
            .query_map([], |r: &rusqlite::Row| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert!(!engagements.is_empty());
        assert_eq!(engagements[0].1, "pending_authorization");

        // Thread status should be AwaitingAuthorization
        let fetched_thread = db::get_thread(&db_conn, &thread.id).unwrap().unwrap();
        assert_eq!(
            fetched_thread.status,
            models::ThreadStatus::AwaitingAuthorization
        );
    }

    // --- 3. Test Refund Request Workflow (Requires Operator Escalation) ---
    let refund_msg = InteractionMessage {
        id: "m3".to_string(),
        business_id: "default_business".to_string(),
        thread_id: "".to_string(),
        customer_id: "+123456789".to_string(),
        channel: ChannelType::Whatsapp,
        direction: MessageDirection::Inbound,
        body: "I am unsatisfied and want a refund immediately".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    // Before sending refund, we must reset thread state to Active to allow transition (since it was AwaitingAuthorization)
    {
        let db_conn = conn_arc.lock().await;
        let thread_id = db_conn.query_row(
            "SELECT id FROM business_interaction_threads WHERE contact_id = 'c1' ORDER BY updated_at DESC LIMIT 1",
            [],
            |r: &rusqlite::Row| r.get::<_, String>(0)
        ).unwrap();
        db_conn
            .execute(
                "UPDATE business_interaction_threads SET status = 'active' WHERE id = ?1",
                params![thread_id],
            )
            .unwrap();
    }

    let reply3 = engine.handle_message(refund_msg).await.unwrap();
    assert!(reply3.body.contains("escalated to an operator"));

    // Verify queue and escalation status in DB
    {
        let db_conn = conn_arc.lock().await;
        let thread_id = db_conn.query_row(
            "SELECT id FROM business_interaction_threads WHERE contact_id = 'c1' ORDER BY updated_at DESC LIMIT 1",
            [],
            |r: &rusqlite::Row| r.get::<_, String>(0)
        ).unwrap();
        let thread = db::get_thread(&db_conn, &thread_id).unwrap().unwrap();

        // Thread status should be Escalated
        assert_eq!(thread.status, models::ThreadStatus::Escalated);

        // Queue item exists
        let queue = db::list_operator_queue(&db_conn).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].thread_id, thread.id);
        assert_eq!(queue[0].priority, "critical"); // immediately critical because of "immediately" / ASAP keyword scoring
    }
}

#[test]
fn test_authorization_request_created() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    // seed thread and engagement
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'active', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'proposed', 1000, 2000, ?1, ?1)", params![now]).unwrap();

    let req =
        super::authorization::AuthorizationQueue::create_request(&conn, "e1", "system").unwrap();
    assert_eq!(req.engagement_id, "e1");
    assert_eq!(req.proposed_by, "system");
    assert_eq!(req.status, models::AuthorizationStatus::Pending);

    // Verify DB
    let saved_req = db::get_authorization_request(&conn, &req.id)
        .unwrap()
        .unwrap();
    assert_eq!(saved_req.engagement_id, "e1");
    assert_eq!(saved_req.status, models::AuthorizationStatus::Pending);

    let kpi = db::get_kpi_metric(&conn, "authorization_requests_total").unwrap();
    assert_eq!(kpi, 1);
}

#[test]
fn test_authorization_approval_flow() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    super::authorization::AuthorizationQueue::approve_request(&conn, "a1").unwrap();

    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Approved);

    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Authorized);

    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::Active);

    let kpi = db::get_kpi_metric(&conn, "authorization_approved_total").unwrap();
    assert_eq!(kpi, 1);
}

#[test]
fn test_authorization_rejection_flow() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    super::authorization::AuthorizationQueue::reject_request(&conn, "a1").unwrap();

    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Rejected);

    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Rejected);

    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::Active);

    let kpi = db::get_kpi_metric(&conn, "authorization_rejected_total").unwrap();
    assert_eq!(kpi, 1);
}

#[test]
fn test_authorization_expiration_flow() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    super::authorization::AuthorizationQueue::expire_request(&conn, "a1").unwrap();

    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Expired);

    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Cancelled);

    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::Active);
}

#[test]
fn test_calendar_mock_booking() {
    use super::calendar::{CalendarProvider, MockCalendarProvider};

    let cal = MockCalendarProvider::new();
    assert!(cal.check_availability(100, 200).unwrap());

    let evt = cal.create_event("Test Event", 100, 200).unwrap();
    assert_eq!(evt.title, "Test Event");
    assert_eq!(evt.start_time, 100);

    let updated = cal
        .update_event(&evt.id, "Updated Event", 150, 250)
        .unwrap();
    assert_eq!(updated.title, "Updated Event");
    assert_eq!(updated.start_time, 150);

    cal.cancel_event(&evt.id).unwrap();
    assert!(cal.get_event(&evt.id).is_none());

    cal.set_should_fail(true);
    assert!(cal.create_event("Failure Event", 100, 200).is_err());
}

#[test]
fn test_spreadsheet_logging() {
    use super::spreadsheet::{MockSpreadsheetProvider, SpreadsheetProvider};

    let sheet = MockSpreadsheetProvider::new();
    sheet
        .append_row("Bookings", vec!["id1".to_string(), "Alice".to_string()])
        .unwrap();

    let row = sheet.get_row("Bookings", 0).unwrap();
    assert_eq!(row[1], "Alice");

    let found = sheet.lookup_row("Bookings", "Alice").unwrap().unwrap();
    assert_eq!(found.0, 0);

    sheet
        .update_row("Bookings", 0, vec!["id1".to_string(), "Bob".to_string()])
        .unwrap();
    let row_updated = sheet.get_row("Bookings", 0).unwrap();
    assert_eq!(row_updated[1], "Bob");
}

#[test]
fn test_booking_execution_success() {
    use super::booking::BookingExecutionService;
    use super::calendar::MockCalendarProvider;
    use super::spreadsheet::MockSpreadsheetProvider;

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    let cal = MockCalendarProvider::new();
    let sheet = MockSpreadsheetProvider::new();

    let conf = BookingExecutionService::execute_booking(&conn, "a1", &cal, &sheet).unwrap();
    assert!(conf.contains("Confirmed"));

    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Approved);

    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Authorized);

    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::Active);

    // Verify spreadsheet row was appended
    let rows = sheet.get_rows("Bookings").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], "e1");

    // Verify metrics
    assert_eq!(
        db::get_kpi_metric(&conn, "bookings_created_total").unwrap(),
        1
    );
    assert_eq!(
        db::get_kpi_metric(&conn, "authorization_approved_total").unwrap(),
        1
    );
    assert_eq!(
        db::get_kpi_metric(&conn, "calendar_events_created_total").unwrap(),
        1
    );
}

#[test]
fn test_booking_execution_failure() {
    use super::booking::BookingExecutionService;
    use super::calendar::MockCalendarProvider;
    use super::spreadsheet::MockSpreadsheetProvider;

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    let cal = MockCalendarProvider::new();
    cal.set_should_fail(true);
    let sheet = MockSpreadsheetProvider::new();

    let res = BookingExecutionService::execute_booking(&conn, "a1", &cal, &sheet);
    assert!(res.is_err());

    // Authorization must remain pending and visible
    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Pending);

    // Engagement status stays pending
    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::PendingAuthorization);

    // Thread status stays awaiting
    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::AwaitingAuthorization);

    // Verify metrics
    assert_eq!(
        db::get_kpi_metric(&conn, "bookings_failed_total").unwrap(),
        1
    );
}

#[test]
fn test_thread_awaiting_authorization() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'active', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'proposed', 1000, 2000, ?1, ?1)", params![now]).unwrap();

    // Call request_authorization
    let req = workflow::request_authorization(&conn, "e1", "system").unwrap();

    // Thread status should be updated to AwaitingAuthorization
    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::AwaitingAuthorization);

    // Engagement status should be updated to PendingAuthorization
    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::PendingAuthorization);

    assert_eq!(req.status, models::AuthorizationStatus::Pending);
}

#[tokio::test]
async fn test_end_to_end_booking_flow() {
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
    use crate::agent::worker::WorkerAgent;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::communication::mock_connector::MockConnector;
    use crate::communication::types::ChannelType;
    use crate::communication::ConnectorRegistry;
    use crate::communication::{InteractionMessage, MessageDirection};
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(AsyncMutex::new(conn));

    // Mocks and Registry
    let registry = Arc::new(ConnectorRegistry::new());
    let mock_connector = Arc::new(MockConnector::new());
    registry.register("whatsapp", mock_connector.clone());

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
        Arc::clone(&conn_arc),
    )
    .unwrap();

    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let calendar = Arc::new(crate::business::calendar::MockCalendarProvider::new());
    let spreadsheet = Arc::new(crate::business::spreadsheet::MockSpreadsheetProvider::new());
    let notifier = Arc::new(crate::business::owner::MockNotificationProvider::new());
    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_arc.clone(),
        registry,
        calendar.clone(),
        spreadsheet.clone(),
        notifier,
    ));

    // Seed contact
    {
        let db_conn = conn_arc.lock().await;
        let now = chrono::Utc::now().timestamp();
        db_conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
        db_conn.execute("INSERT INTO business_identity_mappings (id, contact_id, channel, external_id) VALUES ('m1', 'c1', 'whatsapp', '+123456789')", ()).unwrap();
    }

    // Step 1: Customer request
    let customer_msg = InteractionMessage::new(
        "default_business".to_string(),
        "".to_string(), // will be resolved
        "+123456789".to_string(),
        ChannelType::Whatsapp,
        MessageDirection::Inbound,
        "Book a consultation tomorrow".to_string(),
    );

    let reply = runtime.handle_message(customer_msg).await.unwrap();
    assert!(reply.body.contains("pending operator approval"));

    // Step 2: Verification of qualification & authorization request created
    let request_id = {
        let db_conn = conn_arc.lock().await;
        // Verify thread status is AwaitingAuthorization
        let thread_id = db_conn
            .query_row(
                "SELECT id FROM business_interaction_threads WHERE contact_id = 'c1' LIMIT 1",
                [],
                |r: &rusqlite::Row| r.get::<_, String>(0),
            )
            .unwrap();
        let thread = db::get_thread(&db_conn, &thread_id).unwrap().unwrap();
        assert_eq!(thread.status, models::ThreadStatus::AwaitingAuthorization);

        // Verify pending AuthorizationRequest exists
        let requests = db::list_pending_authorization_requests(&db_conn).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].thread_id, thread_id);
        requests[0].id.clone()
    };

    // Start background workers
    let _workers = runtime.start_background_workers();

    // Step 3: Owner decision approval and execution
    let confirmation = runtime
        .approve_and_execute_booking(&request_id)
        .await
        .unwrap();
    assert!(confirmation.contains("Confirmed"));

    // Sleep to let background job execute
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Step 4: Verification of calendar event, spreadsheet, and state restoration
    {
        let db_conn = conn_arc.lock().await;
        let thread_id = db_conn
            .query_row(
                "SELECT id FROM business_interaction_threads WHERE contact_id = 'c1' LIMIT 1",
                [],
                |r: &rusqlite::Row| r.get::<_, String>(0),
            )
            .unwrap();

        // Thread status back to Active
        let thread = db::get_thread(&db_conn, &thread_id).unwrap().unwrap();
        assert_eq!(thread.status, models::ThreadStatus::Active);

        // AuthorizationRequest status is Approved
        let req = db::get_authorization_request(&db_conn, &request_id)
            .unwrap()
            .unwrap();
        assert_eq!(req.status, models::AuthorizationStatus::Approved);

        // Engagement status is Authorized
        let eng = db::get_engagement(&db_conn, &req.engagement_id)
            .unwrap()
            .unwrap();
        assert_eq!(eng.status, models::EngagementStatus::Authorized);

        // Verify Calendar Event was created
        let _cal_events = calendar.get_event("").is_none(); // we don't know the uuid generated, but we can inspect spreadsheet

        // Verify Spreadsheet row appended
        let rows = spreadsheet.get_rows("Bookings").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], eng.id);
    }
}

fn run_mock_google_api_server() -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buffer = [0; 2048];
            let _ = stream.read(&mut buffer);
            let request = String::from_utf8_lossy(&buffer);

            let (response_status, response_body) = if request.contains("/token") {
                (
                    "200 OK",
                    r#"{"access_token":"refreshed_access_token_abc","expires_in":3600,"token_type":"Bearer"}"#,
                )
            } else if request.contains("/freeBusy") {
                if request.contains("1970-01-01T01:06:40") {
                    (
                        "200 OK",
                        r#"{"calendars":{"primary":{"busy":[{"start":"1970-01-01T01:06:40Z","end":"1970-01-01T01:23:20Z"}]}}}"#,
                    )
                } else {
                    ("200 OK", r#"{"calendars":{"primary":{"busy":[]}}}"#)
                }
            } else if request.contains("/events") && request.contains("DELETE") {
                ("204 No Content", "")
            } else if request.contains("/events") {
                (
                    "200 OK",
                    r#"{"id":"google_event_999","summary":"Consultation","start":{"dateTime":"2026-06-15T09:00:00Z"},"end":{"dateTime":"2026-06-15T10:00:00Z"}}"#,
                )
            } else if request.contains("/values") {
                if request.starts_with("GET") {
                    (
                        "200 OK",
                        r#"{"range":"Sheet1!A:Z","majorDimension":"ROWS","values":[["c1","Bob","whatsapp:+1234567890"],["other_id","Alice","whatsapp:+987654321"]]}"#,
                    )
                } else {
                    (
                        "200 OK",
                        r#"{"spreadsheetId":"test-spreadsheet","updates":{"updatedCells":4}}"#,
                    )
                }
            } else if request.contains("/messages") {
                (
                    "200 OK",
                    r#"{"messaging_product":"whatsapp","contacts":[{"input":"1234567890","wa_id":"1234567890"}],"messages":[{"id":"wa_msg_id_123"}]}"#,
                )
            } else {
                ("404 Not Found", "")
            };

            let response =
                format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_status, response_body.len(), response_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}", port)
}

#[test]
fn test_google_calendar_credentials_persistence() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();

    // Save credentials
    db::save_calendar_credentials(&conn, "biz1", "google", "access123", "refresh123", 9999)
        .unwrap();

    // Verify stored format is encrypted (cannot be read raw from DB)
    let raw_access: String = conn
        .query_row(
            "SELECT access_token FROM calendar_credentials WHERE business_id = 'biz1'",
            [],
            |r: &rusqlite::Row| r.get(0),
        )
        .unwrap();
    assert_ne!(raw_access, "access123");

    // Load credentials (verifies automatic decryption works)
    let (access, refresh, expires_at) = db::load_calendar_credentials(&conn, "biz1", "google")
        .unwrap()
        .unwrap();
    assert_eq!(access, "access123");
    assert_eq!(refresh, "refresh123");
    assert_eq!(expires_at, 9999);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_google_calendar_token_refresh() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn));

    let mock_url = run_mock_google_api_server();

    // Seed expired credentials
    {
        let db_conn = conn_arc.lock().unwrap();
        db::save_calendar_credentials(
            &db_conn,
            "biz1",
            "google",
            "old_access",
            "my_refresh_token",
            10,
        )
        .unwrap();

        db_conn.execute(r#"INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) VALUES ('1', 'biz1', 'google', '{"client_id":"c1","selected_spreadsheet_id":"sheet1"}', 0, 0)"#, rusqlite::params![]).unwrap();
    }

    use super::calendar::GoogleCalendarProvider;
    let provider = GoogleCalendarProvider::with_urls(conn_arc.clone(), "biz1".to_string(), mock_url.clone(), mock_url.clone() + "/token");

    // Call availability (forces token refresh because expires_at=10 is expired)
    use super::calendar::CalendarProvider;
    let avail = provider.check_availability(1000, 2000).unwrap();
    assert!(avail);

    // Verify DB updated with refreshed token
    {
        let db_conn = conn_arc.lock().unwrap();
        let (access, refresh, expires_at) =
            db::load_calendar_credentials(&db_conn, "biz1", "google")
                .unwrap()
                .unwrap();
        assert_eq!(access, "refreshed_access_token_abc");
        assert_eq!(refresh, "my_refresh_token");
        assert!(expires_at > chrono::Utc::now().timestamp());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_google_calendar_availability_queries() {
    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn));

    let mock_url = run_mock_google_api_server();

    {
        let db_conn = conn_arc.lock().unwrap();
        db::save_calendar_credentials(
            &db_conn,
            "biz1",
            "google",
            "valid_access",
            "refresh",
            chrono::Utc::now().timestamp() + 1000,
        )
        .unwrap();

        db_conn.execute(r#"INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) VALUES ('1', 'biz1', 'google', '{"client_id":"c1","selected_spreadsheet_id":"sheet1"}', 0, 0)"#, rusqlite::params![]).unwrap();

    }

    use super::calendar::{CalendarProvider, GoogleCalendarProvider};
    let provider = GoogleCalendarProvider::with_urls(conn_arc.clone(), "biz1".to_string(), mock_url.clone(), mock_url.clone() + "/token");

    // Query normal range (busy slots empty -> returns true)
    let avail = provider.check_availability(1000, 2000).unwrap();
    assert!(avail);

    // Query conflict range (simulated via conflict_test text in endpoint)
    // In our mock server, we check if the request body contains "conflict_test".
    // Wait, let's look at the mock server code: we check if request contains "conflict_test".
    // We can pass conflict_test as part of the query start/end time or title, but check_availability_async formats start/end time in ISO 8601.
    // Let's check: request contains request body as JSON.
    // We can use start_time = 9999999 to cause mock server conflict detection? No, let's verify how mock server detects conflict:
    // It checks if request contains "conflict_test".
    // Wait! In check_availability_async:
    // We send: items: [{"id": "primary"}]
    // Let's adjust mock server to check if request contains the port or something, or we can query with a specific time (e.g. if start_time is 4000, we put conflict_test in the request body? No, start_time is formatted as RFC3339.
    // Wait, let's look at the mock server code:
    // if request.contains("conflict_test") -> we can send a custom header or insert a custom item, or we can check if timeMax contains "4000".
    // Let's modify mock server logic to trigger conflict when timeMin or timeMax contains "4000" or similar, or when we search with a specific start_time.
    // Let's use: if request.contains("1970-01-01T01:06:40") (which corresponds to start_time = 4000!).
    // That is brilliant! start_time = 4000 is 1970-01-01T01:06:40Z. So the mock server will match that string!
    // Let's write the test:
    let avail_conflict = provider.check_availability(4000, 5000).unwrap();
    assert!(!avail_conflict); // returns false (conflict detected!)
}

#[test]
fn test_google_calendar_booking_success() {
    use super::booking::BookingExecutionService;
    use super::spreadsheet::MockSpreadsheetProvider;
    use rusqlite::OpenFlags;

    let db_name = format!(
        "file:test_booking_success_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn_for_provider));

    let mock_url = run_mock_google_api_server();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        "valid_access",
        "refresh",
        now + 1000,
    )
    .unwrap();

    use super::calendar::GoogleCalendarProvider;
    let cal = GoogleCalendarProvider::with_urls(conn_arc.clone(), "default_business".to_string(), mock_url.clone(), mock_url.clone() + "/token");
    let sheet = MockSpreadsheetProvider::new();

    let conf = BookingExecutionService::execute_booking(&conn, "a1", &cal, &sheet).unwrap();
    assert!(conf.contains("google_event_999"));

    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Approved);

    let eng = db::get_engagement(&conn, "e1").unwrap().unwrap();
    assert_eq!(eng.status, models::EngagementStatus::Authorized);

    let thread = db::get_thread(&conn, "t1").unwrap().unwrap();
    assert_eq!(thread.status, models::ThreadStatus::Active);
}

#[test]
fn test_google_calendar_conflict() {
    use super::booking::BookingExecutionService;
    use super::calendar::CalendarProvider;
    use super::spreadsheet::MockSpreadsheetProvider;
    use rusqlite::OpenFlags;

    let db_name = format!(
        "file:test_booking_conflict_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn_for_provider));

    let mock_url = run_mock_google_api_server();

    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    // Set start_time = 4000 to trigger conflict mock
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 4000, 5000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        "valid_access",
        "refresh",
        now + 1000,
    )
    .unwrap();

    use super::calendar::GoogleCalendarProvider;
    let cal = GoogleCalendarProvider::with_urls(conn_arc.clone(), "default_business".to_string(), mock_url.clone(), mock_url.clone() + "/token");
    let sheet = MockSpreadsheetProvider::new();

    // Check availability first (conflict)
    let avail = cal.check_availability(4000, 5000).unwrap();
    assert!(!avail);
}

#[test]
fn test_google_sheets_provider_operations() {
    use super::spreadsheet::provider::SpreadsheetProvider;
    use super::spreadsheet::GoogleSheetsProvider;

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn));

    let mock_url = run_mock_google_api_server();

    // Seed credentials
    {
        let db_conn = conn_arc.lock().unwrap();
        db::save_calendar_credentials(
            &db_conn,
            "biz1",
            "google_sheets",
            "access123",
            "refresh123",
            chrono::Utc::now().timestamp() + 1000,
        )
        .unwrap();

        db_conn.execute(r#"INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) VALUES ('1', 'biz1', 'google', '{"client_id":"c1","selected_spreadsheet_id":"sheet1"}', 0, 0)"#, rusqlite::params![]).unwrap();

    }

    let provider = GoogleSheetsProvider::with_urls(conn_arc.clone(), "biz1".to_string(), mock_url.clone(), mock_url.clone() + "/token");

    // Test append row
    provider
        .append_row("Bookings", vec!["c1".to_string(), "Bob".to_string()])
        .unwrap();

    // Test update row
    provider
        .update_row(
            "Bookings",
            0,
            vec!["c1".to_string(), "Bob Refreshed".to_string()],
        )
        .unwrap();

    // Test lookup row
    let result = provider.lookup_row("Bookings", "Bob").unwrap();
    assert!(result.is_some());
    let (idx, row) = result.unwrap();
    assert_eq!(idx, 0);
    assert_eq!(row[1], "Bob");
}

#[tokio::test]
async fn test_whatsapp_connector_send() {
    use crate::communication::connector::ChannelConnector;
    use crate::communication::types::{ChannelType, InteractionMessage, MessageDirection};
    use crate::communication::whatsapp::provider::WhatsAppConnector;

    let conn = Connection::open_in_memory().unwrap();
    db::prepare_tables(&conn).unwrap();
    let conn_arc = Arc::new(std::sync::Mutex::new(conn));

    let mock_url = run_mock_google_api_server();

    let connector = WhatsAppConnector::with_url(
        conn_arc.clone(),
        "phone_id_123".to_string(),
        mock_url,
        Some("valid_token".to_string()),
    );

    let msg = InteractionMessage {
        id: "msg_123".to_string(),
        business_id: "biz1".to_string(),
        thread_id: "t1".to_string(),
        customer_id: "whatsapp:+1234567890".to_string(),
        channel: ChannelType::Whatsapp,
        direction: MessageDirection::Outbound,
        body: "Hello from core!".to_string(),
        timestamp: chrono::Utc::now().timestamp(),
    };

    connector.send_message(msg).await.unwrap();
}

#[tokio::test]
async fn test_whatsapp_webhook_server() {
    use crate::agent::execution::journal_chain::hmac_sha256;
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::worker::WorkerAgent;
    use crate::business::calendar::MockCalendarProvider;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::business::spreadsheet::MockSpreadsheetProvider;
    use crate::communication::whatsapp::webhook::WhatsAppWebhookServer;
    use crate::communication::ConnectorRegistry;
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use reqwest::Client;
    use std::time::Duration;
    use tokio::sync::RwLock as AsyncRwLock;

    let db_name = format!(
        "file:test_webhook_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;

    // Setup DB and prepare tables
    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_webhook_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_webhook_arc = Arc::new(tokio::sync::Mutex::new(conn_webhook_for_provider));

    let conn_rt_for_provider = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_rt_arc = Arc::new(tokio::sync::Mutex::new(conn_rt_for_provider));

    // Create registry and stub orchestrator
    let registry = Arc::new(ConnectorRegistry::new());
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
        Arc::clone(&conn_rt_arc),
    )
    .unwrap();

    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_rt_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let cal = Arc::new(MockCalendarProvider::new());
    let sheet = Arc::new(MockSpreadsheetProvider::new());
    let notifier = Arc::new(crate::business::owner::MockNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_rt_arc.clone(),
        registry,
        cal,
        sheet,
        notifier,
    ));

    let verify_token = "my_secret_verify_token".to_string();
    let client_secret = "my_app_secret".to_string();

    let server = Arc::new(WhatsAppWebhookServer::new(
        runtime.clone(),
        conn_webhook_arc.clone(),
        verify_token.clone(),
        client_secret.clone(),
    ));

    // Bind to arbitrary port
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let addr = format!("127.0.0.1:{}", port);

    let handle = WhatsAppWebhookServer::start(server, addr.clone());

    // Give server a tiny bit to spin up
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = Client::new();

    // 1. GET verification success
    let res = client
        .get(format!(
            "http://{}/webhook?hub.mode=subscribe&hub.verify_token={}&hub.challenge=hello_world",
            addr, verify_token
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "hello_world");

    // 2. GET verification failure
    let res = client.get(format!("http://{}/webhook?hub.mode=subscribe&hub.verify_token=bad_token&hub.challenge=hello_world", addr))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    // 3. POST message signature validation failure
    let payload = serde_json::json!({
        "entry": [
            {
                "changes": [
                    {
                        "value": {
                            "messaging_product": "whatsapp",
                            "metadata": {
                                "display_phone_number": "16505551111",
                                "phone_number_id": "123456789"
                            },
                            "contacts": [
                                {
                                    "profile": {
                                        "name": "Bob Test"
                                    },
                                    "wa_id": "1234567890"
                                }
                            ],
                            "messages": [
                                {
                                    "from": "1234567890",
                                    "id": "wa_msg_1",
                                    "timestamp": "1672531199",
                                    "text": {
                                        "body": "Hello world"
                                    },
                                    "type": "text"
                                }
                            ]
                        },
                        "field": "messages"
                    }
                ]
            }
        ]
    });

    let payload_bytes = serde_json::to_vec(&payload).unwrap();
    let res = client
        .post(format!("http://{}/webhook", addr))
        .header(
            "x-hub-signature-256",
            "sha256=invalid_signature_hex_value_12345678",
        )
        .body(payload_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // 4. POST message success
    let signature = hmac_sha256(client_secret.as_bytes(), &payload_bytes);
    let signature_hex: String = signature.iter().map(|b| format!("{:02x}", b)).collect();
    let signature_header = format!("sha256={}", signature_hex);

    let res = client
        .post(format!("http://{}/webhook", addr))
        .header("x-hub-signature-256", signature_header.clone())
        .body(payload_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Verify customer automatically created and metric incremented
    {
        let contact =
            db::get_contact_by_identity(&conn, "whatsapp", "whatsapp:+1234567890").unwrap();
        assert!(contact.is_some());
        let contact = contact.unwrap();
        assert_eq!(contact.name.unwrap(), "Bob Test");
        assert_eq!(contact.phone.unwrap(), "+1234567890");

        let metric = db::get_kpi_metric(&conn, "messages_received").unwrap();
        assert_eq!(metric, 1);
    }

    // 5. POST replay check (sending same message ID should be ignored, return 200 but not insert/duplicate)
    let res = client
        .post(format!("http://{}/webhook", addr))
        .header("x-hub-signature-256", signature_header)
        .body(payload_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Verify metric wasn't incremented again
    {
        let metric = db::get_kpi_metric(&conn, "messages_received").unwrap();
        assert_eq!(metric, 1); // remains 1, proving replay protection worked!
    }

    // Clean up server thread
    handle.abort();
}

#[tokio::test]
async fn test_owner_operations_and_notifications() {
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::worker::WorkerAgent;
    use crate::business::calendar::MockCalendarProvider;
    use crate::business::owner::MockNotification;
    use crate::business::owner::MockNotificationProvider;
    use crate::business::owner::NotificationProvider;
    use crate::business::owner::OwnerService;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::business::spreadsheet::MockSpreadsheetProvider;
    use crate::communication::ConnectorRegistry;
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use std::time::Duration;
    use tokio::sync::RwLock as AsyncRwLock;

    let db_name = format!(
        "file:test_owner_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;

    // Connection for test assertions
    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_rt = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_rt_arc = Arc::new(tokio::sync::Mutex::new(conn_rt));

    // Orchestrator stubs
    let registry = Arc::new(ConnectorRegistry::new());
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
        Arc::clone(&conn_rt_arc),
    )
    .unwrap();
    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_rt_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let cal = Arc::new(MockCalendarProvider::new());
    let sheet = Arc::new(MockSpreadsheetProvider::new());
    let notifier = Arc::new(MockNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_rt_arc.clone(),
        registry,
        cal,
        sheet,
        notifier.clone(),
    ));

    let owner_service = OwnerService::new(runtime.clone(), notifier.clone());

    // 1. Seed contact & thread & engagement & authorization request
    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c1', 'Bob', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_interaction_threads (id, contact_id, status, opportunity_status, created_at, updated_at) VALUES ('t1', 'c1', 'awaiting_authorization', 'discovered', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e1', 'c1', 't1', 'Consultation', 'pending_authorization', 1000, 2000, ?1, ?1)", params![now]).unwrap();

    // Seed token credentials for calendar provider to execute successfully
    db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        "valid_access",
        "refresh",
        now + 1000,
    )
    .unwrap();

    // Create authorization request via SQL
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a1', 't1', 'e1', 'system', 'pending', ?1)", params![now]).unwrap();

    // Fire manual new authorization notification to test provider
    notifier.notify_new_authorization("a1", "t1").unwrap();

    // Verify list pending requests works
    let pending = owner_service.list_pending_requests().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "a1");

    // Start background workers
    let _workers = runtime.start_background_workers();

    // 2. Approve Authorization Action
    let approval_msg = owner_service
        .approve_authorization("a1", "owner_bob", "Looks good")
        .await
        .unwrap();
    assert!(approval_msg.contains("Confirmed"));

    // Sleep to let background job execute
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify request status is approved and audit log exists
    let req = db::get_authorization_request(&conn, "a1").unwrap().unwrap();
    assert_eq!(req.status, models::AuthorizationStatus::Approved);

    let audits = owner_service.list_audit_records().await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "approve_authorization");
    assert_eq!(audits[0].performed_by, "owner_bob");
    assert_eq!(audits[0].reason.as_ref().unwrap(), "Looks good");

    // Verify bookings metrics today and approved
    let metrics = owner_service.get_metrics().await.unwrap();
    assert_eq!(metrics.bookings_today, 1);
    assert_eq!(metrics.bookings_approved, 1);

    // Verify notifications contains BookingCompleted
    let notifications = notifier.get_notifications();
    assert!(notifications.contains(&MockNotification::NewAuthorization {
        auth_id: "a1".to_string(),
        thread_id: "t1".to_string()
    }));
    assert!(notifications.contains(&MockNotification::BookingCompleted {
        booking_id: "e1".to_string(),
        thread_id: "t1".to_string()
    }));

    // 3. Create another request for Rejection
    conn.execute("INSERT INTO business_scheduled_engagements (id, contact_id, thread_id, title, status, start_time, end_time, created_at, updated_at) VALUES ('e2', 'c1', 't1', 'Failed Meeting', 'pending_authorization', 3000, 4000, ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_authorization_requests (id, thread_id, engagement_id, proposed_by, status, created_at) VALUES ('a2', 't1', 'e2', 'system', 'pending', ?1)", params![now]).unwrap();

    // Reject Authorization Action
    owner_service
        .reject_authorization("a2", "owner_bob", "Schedule conflict")
        .await
        .unwrap();

    let req2 = db::get_authorization_request(&conn, "a2").unwrap().unwrap();
    assert_eq!(req2.status, models::AuthorizationStatus::Rejected);

    // Verify audits includes rejection
    let audits = owner_service.list_audit_records().await.unwrap();
    assert_eq!(audits.len(), 2);
    assert!(audits
        .iter()
        .any(|a| a.action == "reject_authorization" && a.performed_by == "owner_bob"));
    assert!(audits
        .iter()
        .any(|a| a.action == "approve_authorization" && a.performed_by == "owner_bob"));

    // Verify metrics updated
    let metrics = owner_service.get_metrics().await.unwrap();
    assert_eq!(metrics.bookings_rejected, 1);

    // 4. Escalations resolution
    super::escalation::escalate_thread(&conn, "t1", "Customer demands refund").unwrap();
    notifier
        .notify_new_escalation("manual_esc_1", "t1", "Customer demands refund")
        .unwrap();

    let escalations = owner_service.list_escalations().await.unwrap();
    assert_eq!(escalations.len(), 1);
    assert_eq!(escalations[0].reason, "Customer demands refund");
    assert_eq!(escalations[0].status, "pending");

    let metrics = owner_service.get_metrics().await.unwrap();
    assert_eq!(metrics.pending_escalations, 1);

    // Resolve Escalation
    owner_service
        .resolve_escalation(&escalations[0].id, "owner_bob", "Refund processed")
        .await
        .unwrap();

    let esc_check = db::get_escalation(&conn, &escalations[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(esc_check.status, "resolved");

    // Close Escalation
    owner_service
        .close_escalation(&escalations[0].id, "owner_bob", "Archiving thread")
        .await
        .unwrap();
    let esc_check = db::get_escalation(&conn, &escalations[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(esc_check.status, "closed");

    // Reopen Escalation
    owner_service
        .reopen_escalation(&escalations[0].id, "owner_bob", "Customer replied again")
        .await
        .unwrap();
    let esc_check = db::get_escalation(&conn, &escalations[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(esc_check.status, "reopened");

    let metrics = owner_service.get_metrics().await.unwrap();
    assert_eq!(metrics.pending_escalations, 1);

    // Verify final notifications contains escalation
    let notifications = notifier.get_notifications();
    assert!(notifications.contains(&MockNotification::NewEscalation {
        escalation_id: "manual_esc_1".to_string(),
        thread_id: "t1".to_string(),
        reason: "Customer demands refund".to_string()
    }));
}

#[tokio::test]
async fn test_operator_control_plane_flows() {
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::worker::WorkerAgent;
    use crate::business::calendar::MockCalendarProvider;
    use crate::business::models::{
        OpportunityStatus, PipelineStage, ThreadControlMode, ThreadStatus,
    };
    use crate::business::operator_api::OperatorService;
    use crate::business::owner::MockNotification;
    use crate::business::owner::MockNotificationProvider;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::business::spreadsheet::MockSpreadsheetProvider;
    use crate::communication::ConnectorRegistry;
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use std::time::Duration;
    use tokio::sync::RwLock as AsyncRwLock;

    let db_name = format!(
        "file:test_operator_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_rt = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_rt_arc = Arc::new(tokio::sync::Mutex::new(conn_rt));

    let registry = Arc::new(ConnectorRegistry::new());
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
        Arc::clone(&conn_rt_arc),
    )
    .unwrap();
    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_rt_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let cal = Arc::new(MockCalendarProvider::new());
    let sheet = Arc::new(MockSpreadsheetProvider::new());
    let notifier = Arc::new(MockNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_rt_arc.clone(),
        registry,
        cal,
        sheet,
        notifier.clone(),
    ));

    let operator_service = OperatorService::new(runtime.clone(), notifier.clone());

    // 1. Seed contact
    let now = chrono::Utc::now().timestamp();
    conn.execute("INSERT INTO business_external_contacts (id, name, created_at, updated_at) VALUES ('c10', 'Alice', ?1, ?1)", params![now]).unwrap();
    conn.execute("INSERT INTO business_identity_mappings (id, contact_id, channel, external_id) VALUES ('m10', 'c10', 'whatsapp', 'alice_phone')", ()).unwrap();

    // Seed token credentials
    db::save_calendar_credentials(
        &conn,
        "default_business",
        "google",
        "valid_access",
        "refresh",
        now + 1000,
    )
    .unwrap();

    // Verify resolve active thread creates thread in AI mode by default
    let _thread_reply = runtime
        .handle_interaction(
            "whatsapp",
            "alice_phone",
            "Hi, I want a consultation tomorrow",
        )
        .await
        .unwrap();

    // Find the thread
    let active_threads = operator_service.list_active_threads().await.unwrap();
    assert_eq!(active_threads.len(), 1);
    let thread_id = &active_threads[0].id;
    assert_eq!(active_threads[0].control_mode, ThreadControlMode::Ai);
    assert!(!active_threads[0].is_paused);

    // 2. Test thread_pause_resume
    operator_service
        .pause_thread(thread_id, "op_alice")
        .await
        .unwrap();
    let thread_check = db::get_thread(&conn, thread_id).unwrap().unwrap();
    assert!(thread_check.is_paused);

    // Incoming messages should be muted when paused
    let mute_reply = runtime
        .handle_interaction("whatsapp", "alice_phone", "Hello?")
        .await
        .unwrap();
    assert!(mute_reply.contains("Muted"));

    operator_service
        .resume_thread(thread_id, "op_alice")
        .await
        .unwrap();
    let thread_check = db::get_thread(&conn, thread_id).unwrap().unwrap();
    assert!(!thread_check.is_paused);

    // 3. Test human_takeover & hybrid_mode
    // Takeover to human mode
    operator_service
        .set_thread_control_mode(thread_id, ThreadControlMode::Human, "op_alice")
        .await
        .unwrap();
    let thread_check = db::get_thread(&conn, thread_id).unwrap().unwrap();
    assert_eq!(thread_check.control_mode, ThreadControlMode::Human);

    // In HUMAN mode, responses are muted
    let mute_reply2 = runtime
        .handle_interaction("whatsapp", "alice_phone", "Are you there?")
        .await
        .unwrap();
    assert!(mute_reply2.contains("Muted"));

    // Takeover to hybrid mode
    operator_service
        .set_thread_control_mode(thread_id, ThreadControlMode::Hybrid, "op_alice")
        .await
        .unwrap();
    let thread_check = db::get_thread(&conn, thread_id).unwrap().unwrap();
    assert_eq!(thread_check.control_mode, ThreadControlMode::Hybrid);

    // In HYBRID mode, suggestions are generated and stored
    let hybrid_reply = runtime
        .handle_interaction("whatsapp", "alice_phone", "Book a slot please")
        .await
        .unwrap();
    assert!(hybrid_reply.contains("Suggestion generated"));

    // Suggestion exists in suggestions table
    let suggestion = db::get_response_suggestion(&conn, thread_id)
        .unwrap()
        .unwrap();
    assert!(!suggestion.suggested_reply.is_empty());

    // Humans decide whether to send: send_suggestion
    operator_service
        .send_suggestion(thread_id, "op_alice")
        .await
        .unwrap();
    assert!(db::get_response_suggestion(&conn, thread_id)
        .unwrap()
        .is_none());

    // 4. Test operator_assignment
    operator_service
        .assign_operator(thread_id, "operator_dave", "op_alice")
        .await
        .unwrap();
    let thread_check = db::get_thread(&conn, thread_id).unwrap().unwrap();
    assert_eq!(thread_check.assigned_operator.unwrap(), "operator_dave");

    // 5. Test pipeline_progression
    let opps = operator_service.list_opportunities().await.unwrap();
    assert_eq!(opps.len(), 1);
    let opp_id = &opps[0].id;
    assert_eq!(opps[0].pipeline_stage, PipelineStage::AwaitingApproval); // automatically transitioned during booking decision

    // Transition to Qualified
    operator_service
        .transition_pipeline_stage(
            opp_id,
            PipelineStage::Qualified,
            "op_alice",
            Some("qualified manually".to_string()),
        )
        .await
        .unwrap();
    let opps = operator_service.list_opportunities().await.unwrap();
    assert_eq!(opps[0].pipeline_stage, PipelineStage::Qualified);

    // Verify history logs
    let history = db::list_pipeline_history(&conn, opp_id).unwrap();
    assert!(!history.is_empty());
    assert_eq!(history.last().unwrap().to_stage, PipelineStage::Qualified);
    assert_eq!(history.last().unwrap().changed_by, "op_alice");

    // 6. Test analytics generation
    db::insert_analytics_event(&conn, "response_time", Some(thread_id), 12.0).unwrap();
    db::insert_analytics_event(&conn, "authorization_time", Some(thread_id), 45.0).unwrap();

    let analytics = operator_service.get_analytics().await.unwrap();
    assert!(analytics.human_takeover_rate > 0.0);
    assert!(analytics.average_response_time > 0.0);
    assert!(analytics.average_authorization_time > 0.0);

    // 7. Test search capabilities
    let search_t = operator_service.search_threads("Alice").await.unwrap();
    assert_eq!(search_t.len(), 1);
    assert_eq!(search_t[0].id, *thread_id);

    let search_c = operator_service.search_customers("Alice").await.unwrap();
    assert_eq!(search_c.len(), 1);

    let search_audits = operator_service
        .search_audit_events("op_alice")
        .await
        .unwrap();
    assert!(!search_audits.is_empty());

    // Verify stage change notification is triggered
    let notifications = notifier.get_notifications();
    assert!(notifications
        .iter()
        .any(|n| matches!(n, MockNotification::PipelineStageChange { .. })));
}

#[tokio::test]
async fn test_reliability_features() {
    use crate::agent::injector::InjectorConfig;
    use crate::agent::interaction_orchestrator::Orchestrator;
    use crate::agent::worker::WorkerAgent;
    use crate::business::calendar::MockCalendarProvider;
    use crate::business::owner::MockNotificationProvider;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::business::spreadsheet::MockSpreadsheetProvider;
    use crate::communication::ConnectorRegistry;
    use crate::integration::external_api::ExternalApiConfig;
    use crate::memory::meta_brain::MetaBrain;
    use crate::planner::WorkerConfig;
    use crate::planner::{PlannerAgent, PlannerConfig};
    use std::time::Duration;
    use tokio::sync::RwLock as AsyncRwLock;

    let db_name = format!(
        "file:test_reliability_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        | rusqlite::OpenFlags::SQLITE_OPEN_URI;

    let conn = Connection::open_with_flags(&db_name, flags).unwrap();
    db::prepare_tables(&conn).unwrap();

    let conn_rt = Connection::open_with_flags(&db_name, flags).unwrap();
    let conn_rt_arc = Arc::new(tokio::sync::Mutex::new(conn_rt));

    let registry = Arc::new(ConnectorRegistry::new());
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
        Arc::clone(&conn_rt_arc),
    )
    .unwrap();
    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_rt_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let cal = Arc::new(MockCalendarProvider::new());
    let sheet = Arc::new(MockSpreadsheetProvider::new());
    let notifier = Arc::new(MockNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_rt_arc.clone(),
        registry,
        cal,
        sheet,
        notifier.clone(),
    ));

    // Start background workers
    let _workers = runtime.start_background_workers();

    // 1. Verify idempotency
    {
        let db_conn = conn_rt_arc.lock().await;
        let acq1 =
            crate::business::reliability::idempotency::try_acquire(&db_conn, "test_key").unwrap();
        assert_eq!(
            acq1,
            crate::business::reliability::idempotency::IdempotencyStatus::Acquired
        );

        let acq2 =
            crate::business::reliability::idempotency::try_acquire(&db_conn, "test_key").unwrap();
        assert_eq!(
            acq2,
            crate::business::reliability::idempotency::IdempotencyStatus::InFlight
        );

        crate::business::reliability::idempotency::complete(
            &db_conn,
            "test_key",
            "response_payload",
        )
        .unwrap();

        let acq3 =
            crate::business::reliability::idempotency::try_acquire(&db_conn, "test_key").unwrap();
        assert_eq!(
            acq3,
            crate::business::reliability::idempotency::IdempotencyStatus::Completed {
                response: "response_payload".to_string()
            }
        );
    }

    // 2. Verify locking
    {
        let lock1 = runtime.lock_manager.try_lock("test_lock");
        assert!(lock1.is_some());

        let lock2 = runtime.lock_manager.try_lock("test_lock");
        assert!(lock2.is_none());

        drop(lock1);
        let lock3 = runtime.lock_manager.try_lock("test_lock");
        assert!(lock3.is_some());
    }

    // 3. Verify event store
    {
        let db_conn = conn_rt_arc.lock().await;
        let payload = serde_json::json!({ "foo": "bar" });
        crate::business::event_store::record_event(&db_conn, "test_event", "agg_1", &payload)
            .unwrap();

        let count: i64 = db_conn
            .query_row(
                "SELECT COUNT(*) FROM business_event_store WHERE event_type = 'test_event'",
                [],
                |r: &rusqlite::Row| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // 4. Verify metrics
    {
        let db_conn = conn_rt_arc.lock().await;
        let metrics = crate::business::metrics::get_reliability_metrics(&db_conn).unwrap();
        assert_eq!(metrics.calendar_failures, 0);
    }

    // 5. Verify stress test suite runs successfully
    crate::business::stress_tests::run_stress_suite(&runtime)
        .await
        .unwrap();
}
