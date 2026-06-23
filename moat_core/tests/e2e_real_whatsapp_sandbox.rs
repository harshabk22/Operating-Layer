use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_real_whatsapp_sandbox_integration() {
    let api_key =
        std::env::var("WHATSAPP_SANDBOX_API_KEY").unwrap_or_else(|_| "test_key".to_string());
    if api_key == "test_key" {
        println!("Skipping real test due to missing credentials");
        return;
    }
    let client = Client::new();
    let payload = json!({
        "messaging_product": "whatsapp",
        "to": "1234567890",
        "type": "template",
        "template": {
            "name": "hello_world",
            "language": { "code": "en_US" }
        }
    });

    let res = client
        .post("https://graph.facebook.com/v17.0/sandbox_id/messages")
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success());
}
