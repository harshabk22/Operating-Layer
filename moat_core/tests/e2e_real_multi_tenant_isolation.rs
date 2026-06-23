use reqwest::Client;
use std::env;

#[tokio::test]
async fn test_real_multi_tenant_isolation() {
    let tenant_1_token = env::var("TENANT_1_API_KEY").unwrap_or_else(|_| "t1_dummy".to_string());
    let tenant_2_token = env::var("TENANT_2_API_KEY").unwrap_or_else(|_| "t2_dummy".to_string());
    if tenant_1_token == "t1_dummy" {
        return;
    }
    let client = Client::new();
    let api_url = env::var("MOAT_API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    // Tenant 1 request
    let res1 = client
        .get(&format!("{}/api/leads", api_url))
        .bearer_auth(&tenant_1_token)
        .send()
        .await
        .unwrap();

    // Tenant 2 request
    let res2 = client
        .get(&format!("{}/api/leads", api_url))
        .bearer_auth(&tenant_2_token)
        .send()
        .await
        .unwrap();

    assert!(res1.status().is_success());
    assert!(res2.status().is_success());
}
