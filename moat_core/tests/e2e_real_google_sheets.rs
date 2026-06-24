use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_real_google_sheets_integration() {
    let token =
        std::env::var("GOOGLE_SHEETS_OAUTH_TOKEN").unwrap_or_else(|_| "test_token".to_string());
    if token == "test_token" {
        return;
    }
    let client = Client::new();

    let payload = json!({
        "values": [
            ["New Lead", "John Doe", "john@example.com"]
        ]
    });

    let sheet_id = std::env::var("TEST_SHEET_ID").unwrap_or_else(|_| "dummy_sheet".to_string());

    let res = client.post(format!("https://sheets.googleapis.com/v4/spreadsheets/{}/values/A1:append?valueInputOption=USER_ENTERED", sheet_id))
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
