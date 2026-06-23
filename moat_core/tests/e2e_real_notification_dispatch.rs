use reqwest::Client;
use std::env;

#[tokio::test]
async fn test_real_notification_dispatch() {
    let webhook_url = env::var("SLACK_WEBHOOK_URL").unwrap_or_else(|_| "dummy_url".to_string());
    if webhook_url == "dummy_url" {
        return;
    }
    let client = Client::new();

    let res = client
        .post(&webhook_url)
        .json(&serde_json::json!({
            "text": "System Alert: High volume of lead requests observed in the past 10 minutes."
        }))
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
