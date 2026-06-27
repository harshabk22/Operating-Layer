#[cfg(test)]
mod tests {
    use super::super::db::DbStore;
    use super::super::crypto::verify_ed25519;
    use super::super::workers::{run_heartbeat_check, run_billing_cycle};
    use super::super::api::app_router;
    use super::super::AppState;
    
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use serde_json::{json, Value};
    use rusqlite::params;
    use chrono::{Utc, Duration};
    use ed25519_dalek::{SigningKey, Signer};
    use rand::rngs::OsRng;

    fn setup_test_db() -> DbStore {
        DbStore::new(":memory:").unwrap()
    }

    #[tokio::test]
    async fn test_db_initialization_and_seeding() {
        let db = setup_test_db();
        let conn = db.conn.lock().unwrap();

        let tables = vec![
            "admin_users", "customers", "customer_runtimes", "packages",
            "package_versions", "deployments", "subscriptions",
            "incidents", "audit_logs", "approvals", "rollbacks"
        ];
        
        for table in tables {
            let count: i64 = conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
                [table],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(count, 1, "Table {} should exist", table);
        }

        let admin_exists: i64 = conn.query_row(
            "SELECT count(*) FROM admin_users WHERE username = 'admin'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(admin_exists, 1);
    }

    #[test]
    fn test_ed25519_signature_verification() {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let public_key = signing_key.verifying_key();
        
        let checksum = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // Empty hash hex
        let signature = signing_key.sign(checksum.as_bytes());
        
        let pub_key_hex = hex::encode(public_key.to_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        let result = verify_ed25519(&pub_key_hex, &signature_hex, checksum);
        assert!(result.is_ok(), "Signature verification failed: {:?}", result.err());

        let bad_signature_hex = hex::encode([0u8; 64]);
        let result_bad = verify_ed25519(&pub_key_hex, &bad_signature_hex, checksum);
        assert!(result_bad.is_err());
    }

    #[tokio::test]
    async fn test_heartbeat_monitor_and_incident_storming_prevention() {
        let db = setup_test_db();
        let now = Utc::now();
        
        // Insert active customer
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO customers (id, organization_name, organization_id, status, tier, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    "cust_1",
                    "Acme Corp",
                    "acme-corp",
                    "Active",
                    "Enterprise",
                    now.to_rfc3339(),
                    now.to_rfc3339()
                ],
            ).unwrap();

            // Insert runtime with stale heartbeat
            conn.execute(
                "INSERT INTO customer_runtimes (id, customer_id, environment, status, api_key_hash, runtime_api_url, last_heartbeat_at, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "rt_1",
                    "cust_1",
                    "Production",
                    "Active",
                    "test_api_key_hash_value",
                    "https://production.acme.com",
                    (now - Duration::seconds(200)).to_rfc3339(), // Stale (greater than 180s)
                    now.to_rfc3339(),
                    now.to_rfc3339()
                ],
            ).unwrap();
        }

        // Run monitor check
        run_heartbeat_check(&db).await.unwrap();

        // Check if an incident was opened
        {
            let conn = db.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM incidents WHERE runtime_id = 'rt_1' AND incident_type = 'heartbeat_missed' AND status = 'Open'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(count, 1, "An incident should have been opened");
        }

        // Run monitor check again immediately
        run_heartbeat_check(&db).await.unwrap();

        // Check that we didn't open a duplicate incident (incident storming prevention)
        {
            let conn = db.conn.lock().unwrap();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM incidents WHERE runtime_id = 'rt_1' AND incident_type = 'heartbeat_missed' AND status = 'Open'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(count, 1, "Should still have exactly one open incident");
        }
    }

    #[tokio::test]
    async fn test_billing_worker_advances_date() {
        let db = setup_test_db();
        let now = Utc::now();
        let past_billing_time = (now - Duration::days(1)).to_rfc3339();

        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO customers (id, organization_name, organization_id, status, tier, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params!["cust_billing", "Billing Corp", "billing-corp", "Active", "Enterprise", now.to_rfc3339(), now.to_rfc3339()],
            ).unwrap();

            // Insert subscription that is due for billing
            conn.execute(
                "INSERT INTO subscriptions (id, customer_id, plan_id, status, payment_status, current_period_start, current_period_end, amount, created_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    "sub_1",
                    "cust_billing",
                    "premium",
                    "Active",
                    "Valid",
                    past_billing_time,
                    past_billing_time,
                    9999, // Amount in cents
                    now.to_rfc3339()
                ],
            ).unwrap();
        }

        // Run billing cycle
        run_billing_cycle(&db).await.unwrap();

        // Verify that current_period_end has advanced
        {
            let conn = db.conn.lock().unwrap();
            let current_end: String = conn.query_row(
                "SELECT current_period_end FROM subscriptions WHERE id = 'sub_1'",
                [],
                |row| row.get(0),
            ).unwrap();
            
            assert!(current_end > past_billing_time, "current_period_end should have advanced");
            
            let logs_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'SUBSCRIPTION_BILL_SUCCESS' AND target_id = 'sub_1'",
                [],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(logs_count, 1);
        }
    }

    #[tokio::test]
    async fn test_api_login_endpoint() {
        let db = setup_test_db();
        let state = AppState {
            db,
            jwt_secret: "test_secret_key_999".to_string(),
        };

        let app = app_router().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "username": "admin",
                            "password": "admin"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        
        let body_bytes = axum::body::to_bytes(response.into_body(), 10000).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
        
        assert!(body_json.get("token").is_some());
        
        let user_obj = body_json.get("user").unwrap();
        assert_eq!(user_obj.get("role").unwrap(), "Super Admin");
        assert_eq!(user_obj.get("username").unwrap(), "admin");
    }

    #[tokio::test]
    async fn test_api_auth_gating() {
        let db = setup_test_db();
        let state = AppState {
            db,
            jwt_secret: "test_secret_key_999".to_string(),
        };

        let app = app_router().with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/customers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
