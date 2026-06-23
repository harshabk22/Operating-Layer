use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_real_google_calendar_integration() {
    let token =
        std::env::var("GOOGLE_CALENDAR_OAUTH_TOKEN").unwrap_or_else(|_| "test_token".to_string());
    if token == "test_token" {
        return;
    }
    let client = Client::new();

    let payload = json!({
        "summary": "Moat Real Integration Test",
        "start": { "dateTime": "2026-07-01T10:00:00Z" },
        "end": { "dateTime": "2026-07-01T11:00:00Z" }
    });

    let res = client
        .post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
