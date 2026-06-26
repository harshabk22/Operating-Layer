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
}
