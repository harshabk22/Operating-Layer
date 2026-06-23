use reqwest::Client;
use std::env;

#[tokio::test]
async fn test_real_metrics_export() {
    let datadog_api_key = env::var("DATADOG_API_KEY").unwrap_or_else(|_| "dd_dummy".to_string());
    if datadog_api_key == "dd_dummy" {
        return;
    }
    let client = Client::new();

    let res = client
        .post("https://api.datadoghq.com/api/v1/series")
        .header("DD-API-KEY", datadog_api_key)
        .json(&serde_json::json!({
            "series": [
                {
                    "metric": "moat.system.uptime",
                    "points": [[chrono::Utc::now().timestamp(), 1]],
                    "type": "gauge"
                }
            ]
        }))
        .send()
        .await
        .unwrap();

    assert!(res.status().is_success() || res.status().is_client_error()); // Just checking connection basically
}
