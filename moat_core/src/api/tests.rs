#[cfg(test)]
mod tests {
    use crate::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
    use crate::api::server::start_api_server;
    use crate::business::db;
    use crate::business::runtime::CustomerInteractionRuntime;
    use crate::communication::ConnectorRegistry;
    use crate::memory::meta_brain::MetaBrain;
    use crate::MoatResult;
    use reqwest::{Client, StatusCode};
    use rusqlite::Connection;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};

    type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

    struct LocalStubProvider {
        response: String,
    }

    impl LlmProvider for LocalStubProvider {
        fn chat_complete<'a>(
            &'a self,
            _model: &'a str,
            _messages: Vec<LlmMessage>,
            _tools: Option<Vec<crate::agent::llm_types::LlmTool>>,
            _max_tokens: usize,
            _temperature: f32,
            _response_format: Option<serde_json::Value>,
        ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
            let text = self.response.clone();
            Box::pin(async move {
                Ok(LlmResponse {
                    content: Some(text),
                    tool_calls: None,
                    tokens: 42,
                    structured: None,
                })
            })
        }
    }

    // Helper to create a test runtime
    async fn setup_test_runtime() -> Arc<CustomerInteractionRuntime> {
        let conn = Connection::open_in_memory().unwrap();
        db::prepare_tables(&conn).unwrap();
        crate::communication::db::prepare_tables(&conn).unwrap();

        let conn_arc = Arc::new(AsyncMutex::new(conn));
        let registry = Arc::new(ConnectorRegistry::new());
        let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));

        let provider = Arc::new(LocalStubProvider {
            response: "<response>done</response>".to_string(),
        });
        let ai_agent = Arc::new(crate::agent::ai_agent::AIAgent::new(
            provider.clone(),
            Arc::clone(&conn_arc),
            Arc::clone(&meta_brain),
            None,
        ).unwrap());

        let cal = Arc::new(crate::business::calendar::MockCalendarProvider::new());
        let sheet = Arc::new(crate::business::spreadsheet::MockSpreadsheetProvider::new());
        let notifier = Arc::new(crate::business::owner::MockNotificationProvider::new());

        Arc::new(CustomerInteractionRuntime::with_providers(
            ai_agent,
            conn_arc,
            registry,
            cal,
            sheet,
            notifier,
        ))
    }

    #[tokio::test]
    async fn test_dashboard_server_auth_and_roles() {
        std::env::set_var("OPERATOR_PASSWORD", "operator123");
        std::env::set_var("OWNER_PASSWORD", "owner123");
        let runtime = setup_test_runtime().await;

        // Start the server on a random port
        let (server_handle, port) = start_api_server(runtime.clone(), 0).await.unwrap();

        // Create a plain HTTP client
        let client = Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);

        // 1. Verify that GET / renders the dashboard
        let res = client.get(&base_url).send().await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.text().await.unwrap();
        assert!(body.contains("MoatAgeex"));

        // 2. Verify that unauthenticated requests to API endpoints return 401 Unauthorized
        let res = client
            .get(format!("{}/threads", base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 3. Verify bad login credentials return 401
        let res = client
            .post(format!("{}/login", base_url))
            .json(&json!({ "username": "operator", "password": "wrongpassword" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 4. Verify successful operator login
        let login_res = client
            .post(format!("{}/login", base_url))
            .json(&json!({ "username": "operator", "password": "operator123" }))
            .send()
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        let operator_cookie = login_res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // 5. Verify operator can list threads (200 OK)
        let res = client
            .get(format!("{}/threads", base_url))
            .header("cookie", &operator_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 6. Verify operator cannot list authorizations (403 Forbidden - Owner only)
        let res = client
            .get(format!("{}/authorizations", base_url))
            .header("cookie", &operator_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        // 7. Logout
        let res = client
            .post(format!("{}/logout", base_url))
            .header("cookie", &operator_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 8. Verify post-logout requests are rejected (401 Unauthorized)
        let res = client
            .get(format!("{}/threads", base_url))
            .header("cookie", &operator_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 9. Verify successful owner login
        let login_res = client
            .post(format!("{}/login", base_url))
            .json(&json!({ "username": "owner", "password": "owner123" }))
            .send()
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        let owner_cookie = login_res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // 10. Verify owner CAN list authorizations (200 OK)
        let res = client
            .get(format!("{}/authorizations", base_url))
            .header("cookie", &owner_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Clean up server task
        server_handle.abort();
    }

    #[tokio::test]
    async fn test_system_users_lifecycle() {
        let runtime = setup_test_runtime().await;
        let (server_handle, port) = start_api_server(runtime.clone(), 0).await.unwrap();

        let client = Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);

        // 1. Login as operator
        let login_res = client
            .post(format!("{}/login", base_url))
            .json(&json!({ "username": "operator", "password": "admin" }))
            .send()
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        let cookie = login_res
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        // 2. GET /api/system/users returns default seeded users
        let res = client
            .get(format!("{}/api/system/users", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let users: Vec<serde_json::Value> = res.json().await.unwrap();
        assert!(users.len() >= 3);
        let usernames: Vec<&str> = users.iter().filter_map(|u| u["username"].as_str()).collect();
        assert!(usernames.contains(&"owner"));
        assert!(usernames.contains(&"operator"));
        assert!(usernames.contains(&"viewer"));

        // 3. POST /api/system/users to create a new user
        let new_user = json!({
            "username": "sarah_connor",
            "role": "Operator",
            "email": "sarah@moatageex.io",
            "status": "active"
        });
        let create_res = client
            .post(format!("{}/api/system/users", base_url))
            .header("cookie", &cookie)
            .json(&new_user)
            .send()
            .await
            .unwrap();
        assert_eq!(create_res.status(), StatusCode::OK);
        let created: serde_json::Value = create_res.json().await.unwrap();
        assert_eq!(created["username"], "sarah_connor");
        assert_eq!(created["role"], "Operator");
        assert_eq!(created["email"], "sarah@moatageex.io");
        assert_eq!(created["status"], "active");

        // 4. GET /api/system/users returns merged list containing sarah_connor
        let res2 = client
            .get(format!("{}/api/system/users", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let users2: Vec<serde_json::Value> = res2.json().await.unwrap();
        let usernames2: Vec<&str> = users2.iter().filter_map(|u| u["username"].as_str()).collect();
        assert!(usernames2.contains(&"sarah_connor"));

        // 5. Verify projection in DB
        {
            let conn = runtime.db.lock().await;
            let (proj_id, proj_name): (String, String) = conn.query_row(
                "SELECT id, name FROM projections WHERE id = 'sarah_connor'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap();
            assert_eq!(proj_id, "sarah_connor");
            assert_eq!(proj_name, "UserCreated");
        }

        server_handle.abort();
    }

    #[tokio::test]
    async fn test_observability_endpoints_and_kpi_reliability() {
        let runtime = setup_test_runtime().await;

        // Seed some data in jobs, outbox, audit, and opportunities
        {
            let conn = runtime.db.lock().await;
            conn.execute(
                "INSERT INTO business_background_jobs (id, job_type, payload, status, retries, max_retries, run_at, last_error, created_at)
                 VALUES ('job_1', 'sync_calendar', '{}', 'completed', 2, 5, 1700000000, 'timeout retry', 1699999900)",
                [],
            ).unwrap();

            conn.execute(
                "INSERT INTO business_message_outbox (id, business_id, thread_id, customer_id, channel, direction, body, status, retries, max_retries, run_at, last_error, created_at)
                 VALUES ('outbox_1', 'default_business', 'thread_1', 'cust_42', 'whatsapp', 'outbound', 'Hello!', 'sent', 0, 3, 1700000010, NULL, 1699999910)",
                [],
            ).unwrap();

            conn.execute(
                "INSERT INTO business_owner_audit_trail (id, action, target_id, performed_by, performed_at, reason)
                 VALUES ('audit_1', 'approve_booking', 'booking_99', 'operator_1', 1700000020, 'Verified availability')",
                [],
            ).unwrap();
        }

        let (server_handle, port) = start_api_server(runtime.clone(), 0).await.unwrap();
        let client = Client::new();
        let base_url = format!("http://127.0.0.1:{}", port);

        // Login as operator
        let login_res = client
            .post(format!("{}/login", base_url))
            .json(&json!({ "username": "operator", "password": "admin" }))
            .send()
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        let cookie = login_res.headers().get("set-cookie").unwrap().to_str().unwrap().to_string();

        // 1. Verify /api/system/jobs alias fields
        let res_jobs = client
            .get(format!("{}/api/system/jobs", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res_jobs.status(), StatusCode::OK);
        let jobs: Vec<serde_json::Value> = res_jobs.json().await.unwrap();
        assert!(!jobs.is_empty());
        assert_eq!(jobs[0]["retries"], 2);
        assert_eq!(jobs[0]["retry_count"], 2);
        assert_eq!(jobs[0]["run_at"], 1700000000);
        assert_eq!(jobs[0]["scheduled_at"], 1700000000);
        assert_eq!(jobs[0]["last_error"], "timeout retry");
        assert_eq!(jobs[0]["error_message"], "timeout retry");

        // 2. Verify /api/system/outbox alias fields
        let res_outbox = client
            .get(format!("{}/api/system/outbox", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res_outbox.status(), StatusCode::OK);
        let outbox: Vec<serde_json::Value> = res_outbox.json().await.unwrap();
        assert!(!outbox.is_empty());
        assert_eq!(outbox[0]["customer_id"], "cust_42");
        assert_eq!(outbox[0]["recipient"], "cust_42");
        assert_eq!(outbox[0]["run_at"], 1700000010);
        assert_eq!(outbox[0]["sent_at"], 1700000010);

        // 3. Verify /api/system/audit alias fields
        let res_audit = client
            .get(format!("{}/api/system/audit", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(res_audit.status(), StatusCode::OK);
        let audit: Vec<serde_json::Value> = res_audit.json().await.unwrap();
        assert!(!audit.is_empty());
        assert_eq!(audit[0]["target_id"], "booking_99");
        assert_eq!(audit[0]["entity_id"], "booking_99");
        assert_eq!(audit[0]["performed_by"], "operator_1");
        assert_eq!(audit[0]["actor"], "operator_1");
        assert_eq!(audit[0]["performed_at"], 1700000020);
        assert_eq!(audit[0]["created_at"], 1700000020);
        assert_eq!(audit[0]["reason"], "Verified availability");
        assert_eq!(audit[0]["details"], "Verified availability");

        // 4. Test Lead creation, update, qualification and KPI counting
        let lead_id = "test_lead_kpi_1";
        let create_lead_res = client
            .post(format!("{}/api/sales/leads", base_url))
            .header("cookie", &cookie)
            .json(&json!({ "id": lead_id, "name": "Alice Smith", "email": "alice@example.com" }))
            .send()
            .await
            .unwrap();
        assert_eq!(create_lead_res.status(), StatusCode::OK);

        // Check KPI: leads count = 1
        let kpi_res1 = client
            .get(format!("{}/api/dashboard/kpis", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(kpi_res1.status(), StatusCode::OK);
        let kpi1: serde_json::Value = kpi_res1.json().await.unwrap();
        assert_eq!(kpi1["leads"], 1);

        // Update lead name -> projection becomes LeadUpdated
        let update_lead_res = client
            .patch(format!("{}/api/sales/leads/{}", base_url, lead_id))
            .header("cookie", &cookie)
            .json(&json!({ "name": "Alice Cooper" }))
            .send()
            .await
            .unwrap();
        assert_eq!(update_lead_res.status(), StatusCode::OK);

        // Verify lead is STILL counted in KPI (not lost due to name change to LeadUpdated)
        let kpi_res2 = client
            .get(format!("{}/api/dashboard/kpis", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        let kpi2: serde_json::Value = kpi_res2.json().await.unwrap();
        assert_eq!(kpi2["leads"], 1, "Lead must still be counted in KPI after LeadUpdated");

        // Qualify lead -> projection becomes LeadQualified
        let qual_res = client
            .post(format!("{}/api/sales/leads/{}/qualify", base_url, lead_id))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(qual_res.status(), StatusCode::OK);

        // Verify lead is STILL counted in KPI (not lost due to qualification)
        let kpi_res3 = client
            .get(format!("{}/api/dashboard/kpis", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        let kpi3: serde_json::Value = kpi_res3.json().await.unwrap();
        assert_eq!(kpi3["leads"], 1, "Lead must still be counted in KPI after LeadQualified");

        // 5. Test Opportunity Won Deal Deduplication
        let opp_id = format!("opp_{}", lead_id);
        // Mark opportunity won
        let won_res1 = client
            .patch(format!("{}/api/sales/opportunities/{}", base_url, opp_id))
            .header("cookie", &cookie)
            .json(&json!({ "stage": "won" }))
            .send()
            .await
            .unwrap();
        assert_eq!(won_res1.status(), StatusCode::OK);

        // Check revenue
        let kpi_res4 = client
            .get(format!("{}/api/dashboard/kpis", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        let kpi4: serde_json::Value = kpi_res4.json().await.unwrap();
        let rev1 = kpi4["revenue"].as_i64().unwrap();
        assert!(rev1 > 0, "Revenue should be positive after deal won");

        // Edit already won opportunity (e.g. change title)
        let edit_res = client
            .patch(format!("{}/api/sales/opportunities/{}", base_url, opp_id))
            .header("cookie", &cookie)
            .json(&json!({ "title": "Opportunity: Alice Cooper (VIP)" }))
            .send()
            .await
            .unwrap();
        assert_eq!(edit_res.status(), StatusCode::OK);

        // Verify revenue did NOT duplicate!
        let kpi_res5 = client
            .get(format!("{}/api/dashboard/kpis", base_url))
            .header("cookie", &cookie)
            .send()
            .await
            .unwrap();
        let kpi5: serde_json::Value = kpi_res5.json().await.unwrap();
        let rev2 = kpi5["revenue"].as_i64().unwrap();
        assert_eq!(rev1, rev2, "Revenue must NOT be duplicated on subsequent opportunity updates");

        server_handle.abort();
    }
}
