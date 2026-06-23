use reqwest::Client;
use std::env;

#[tokio::test]
async fn test_real_inbox_escalation() {
    let api_url = env::var("SUPPORT_SYSTEM_API_URL")
        .unwrap_or_else(|_| "http://localhost:8080/api".to_string());
    let token = env::var("SUPPORT_SYSTEM_TOKEN").unwrap_or_else(|_| "test_token".to_string());
    if token == "test_token" {
        return;
    }
    let client = Client::new();

    let res = client
        .post(&format!("{}/tickets", api_url))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "subject": "Escalation: Unknown Intent from User",
            "priority": "high",
            "body": "User requested to speak with a human agent."
        }))
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
