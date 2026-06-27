use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use chrono::Utc;
use rusqlite::params;

use crate::{
    crypto::{hash_sha256, verify_ed25519, generate_jwt},
    middleware::{AdminUser, RuntimeClient},
    AppState,
};

pub fn app_router() -> Router<AppState> {
    Router::new()
        // Auth
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/register", post(register))
        
        // Customers
        .route("/api/v1/customers", get(list_customers).post(create_customer))
        .route("/api/v1/customers/:id", get(get_customer).delete(delete_customer))
        .route("/api/v1/customers/:id/runtimes", post(create_customer_runtime))
        .route("/api/v1/customers/:id/status", put(update_customer_status))
        .route("/api/v1/customers/:id/tier", put(update_customer_tier))
        .route("/api/v1/runtimes", get(list_runtimes))
        
        // Subscriptions
        .route("/api/v1/billing/subscriptions", get(list_subscriptions).post(create_subscription))
        .route("/api/v1/billing/subscriptions/check", post(check_subscriptions))
        
        // Packages
        .route("/api/v1/packages", get(list_packages).post(create_package))
        .route("/api/v1/packages/:id", get(get_package))
        .route("/api/v1/packages/:id/versions", post(create_package_version))
        .route("/api/v1/packages/pending", get(list_pending_packages))
        .route("/api/v1/packages/:id/versions/:version/approve", post(approve_version))
        .route("/api/v1/packages/:id/versions/:version/reject", post(reject_version))
        
        // Deployments & Rollbacks
        .route("/api/v1/deployments", get(list_deployments))
        .route("/api/v1/runtimes/:runtime_id/deploy", post(create_deployment))
        .route("/api/v1/deployments/:id/rollback", post(create_rollback))
        
        // Incidents
        .route("/api/v1/incidents", get(list_incidents).post(create_incident))
        .route("/api/v1/incidents/:id/status", put(update_incident_status))
        
        // Audit Logs
        .route("/api/v1/audit-logs", get(list_audit_logs))
        
        // Callbacks (Runtime Webhooks)
        .route("/api/v1/callbacks/heartbeat", post(runtime_heartbeat))
        .route("/api/v1/callbacks/deployments/:deployment_id", post(runtime_deployment_callback))
}

// Helpers
fn check_role(user: &AdminUser, allowed: &[&str]) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if allowed.contains(&user.role.as_str()) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": format!("Forbidden: Requires one of roles {:?}", allowed) })),
        ))
    }
}

fn log_audit(
    conn: &rusqlite::Connection,
    username: &str,
    action: &str,
    target_type: &str,
    target_id: Option<&str>,
    details: &str,
) -> Result<(), rusqlite::Error> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO audit_logs (id, username, action, target_type, target_id, details, created_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![id, username, action, target_type, target_id, details, now],
    )?;
    Ok(())
}

// Auth Handlers
#[derive(Deserialize)]
struct LoginPayload {
    username: String,
    password: Option<String>,
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> impl IntoResponse {
    let conn = state.db.conn.lock().unwrap();
    let password = payload.password.unwrap_or_default();
    let pass_hash = hash_sha256(&password);

    let user_info: Result<(String, String, String), rusqlite::Error> = conn.query_row(
        "SELECT id, email, role FROM admin_users WHERE username = ? AND password_hash = ?",
        params![payload.username, pass_hash],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match user_info {
        Ok((user_id, email, r)) => {
            let token = generate_jwt(&payload.username, &r, &state.jwt_secret).unwrap();
            let _ = log_audit(&conn, &payload.username, "LOGIN", "admin_users", Some(&user_id), "User logged in successfully");
            (StatusCode::OK, Json(json!({
                "token": token,
                "user": {
                    "id": user_id,
                    "username": payload.username,
                    "email": email,
                    "role": r
                }
            }))).into_response()
        }
        Err(_) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid username or password" })),
        ).into_response(),
    }
}

async fn logout() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "success": true })))
}

#[derive(Deserialize)]
struct RegisterPayload {
    username: String,
    password: Option<String>,
    email: String,
    role: String,
}

async fn register(
    user: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<RegisterPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let password = payload.password.unwrap_or_default();
    let pass_hash = hash_sha256(&password);
    let now = Utc::now().to_rfc3339();
    let user_id = Uuid::new_v4().to_string();

    let result = conn.execute(
        "INSERT INTO admin_users (id, username, email, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![user_id, payload.username, payload.email, pass_hash, payload.role, now, now],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "REGISTER_USER", "admin_users", Some(&user_id), &format!("Registered user {}", payload.username));
            Ok((StatusCode::CREATED, Json(json!({
                "id": user_id,
                "username": payload.username,
                "role": payload.role,
                "created_at": now
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Registration failed: {}", e) })),
        )),
    }
}

// Customers Handlers
#[derive(Deserialize)]
struct CreateCustomerPayload {
    organization_name: String,
    organization_id: String,
    tier: String,
}

async fn list_customers(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Support Engineer", "Finance", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, organization_name, organization_id, status, tier, created_at, updated_at FROM customers").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "organization_name": row.get::<_, String>(1)?,
            "organization_id": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "tier": row.get::<_, String>(4)?,
            "created_at": row.get::<_, String>(5)?,
            "updated_at": row.get::<_, String>(6)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let customers: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(customers)))
}

async fn create_customer(
    user: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateCustomerPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Support Engineer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let customer_id = Uuid::new_v4().to_string();

    let result = conn.execute(
        "INSERT INTO customers (id, organization_name, organization_id, status, tier, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![customer_id, payload.organization_name, payload.organization_id, "Active", payload.tier, now, now],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_CUSTOMER", "customers", Some(&customer_id), &format!("Created customer {}", payload.organization_name));
            Ok((StatusCode::CREATED, Json(json!({
                "id": customer_id,
                "organization_name": payload.organization_name,
                "organization_id": payload.organization_id,
                "status": "Active",
                "tier": payload.tier
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Create customer failed: {}", e) })),
        )),
    }
}

async fn get_customer(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Support Engineer", "Finance", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    
    // Fetch customer details
    let mut customer_val = conn.query_row(
        "SELECT id, organization_name, organization_id, status, tier, created_at, updated_at FROM customers WHERE id = ?",
        [&id],
        |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "organization_name": row.get::<_, String>(1)?,
                "organization_id": row.get::<_, String>(2)?,
                "status": row.get::<_, String>(3)?,
                "tier": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?,
            }))
        },
    ).map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "Customer not found"}))))?;

    // Fetch runtimes
    let mut stmt_runtimes = conn.prepare(
        "SELECT id, environment, status, runtime_api_url, last_heartbeat_at FROM customer_runtimes WHERE customer_id = ?"
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let runtimes_rows = stmt_runtimes.query_map([&id], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "environment": row.get::<_, String>(1)?,
            "status": row.get::<_, String>(2)?,
            "runtime_api_url": row.get::<_, String>(3)?,
            "last_heartbeat_at": row.get::<_, Option<String>>(4)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let runtimes: Vec<serde_json::Value> = runtimes_rows.flatten().collect();

    // Fetch active deployments
    let mut stmt_deployments = conn.prepare(
        "SELECT d.id, d.package_id, p.name, d.version, d.status, d.installed_at 
         FROM deployments d 
         JOIN customer_runtimes r ON d.runtime_id = r.id 
         JOIN packages p ON d.package_id = p.id 
         WHERE r.customer_id = ?"
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let deployments_rows = stmt_deployments.query_map([&id], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "package_id": row.get::<_, String>(1)?,
            "package_name": row.get::<_, String>(2)?,
            "version": row.get::<_, String>(3)?,
            "status": row.get::<_, String>(4)?,
            "installed_at": row.get::<_, Option<String>>(5)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let deployments: Vec<serde_json::Value> = deployments_rows.flatten().collect();

    // Fetch subscription detail
    let subscription: Option<serde_json::Value> = conn.query_row(
        "SELECT id, plan_id, status, payment_status, current_period_start, current_period_end, amount 
         FROM subscriptions WHERE customer_id = ? LIMIT 1",
        [&id],
        |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "plan_id": row.get::<_, String>(1)?,
                "status": row.get::<_, String>(2)?,
                "payment_status": row.get::<_, String>(3)?,
                "current_period_start": row.get::<_, String>(4)?,
                "current_period_end": row.get::<_, String>(5)?,
                "amount": row.get::<_, i64>(6)?,
            }))
        },
    ).ok();

    // Fetch open incidents
    let mut stmt_incidents = conn.prepare(
        "SELECT i.id, i.runtime_id, i.incident_type, i.severity, i.summary, i.status, i.reported_at 
         FROM incidents i 
         JOIN customer_runtimes r ON i.runtime_id = r.id 
         WHERE r.customer_id = ? AND i.status = 'Open'"
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let incidents_rows = stmt_incidents.query_map([&id], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "runtime_id": row.get::<_, String>(1)?,
            "incident_type": row.get::<_, String>(2)?,
            "severity": row.get::<_, String>(3)?,
            "summary": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "reported_at": row.get::<_, String>(6)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let incidents: Vec<serde_json::Value> = incidents_rows.flatten().collect();

    if let Some(obj) = customer_val.as_object_mut() {
        obj.insert("runtimes".to_string(), json!(runtimes));
        obj.insert("deployments".to_string(), json!(deployments));
        obj.insert("subscription".to_string(), json!(subscription));
        obj.insert("incidents".to_string(), json!(incidents));
    }

    Ok((StatusCode::OK, Json(customer_val)))
}

#[derive(Deserialize)]
struct UpdateStatusPayload {
    status: String,
}

async fn update_customer_status(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateStatusPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Support Engineer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let result = conn.execute(
        "UPDATE customers SET status = ?, updated_at = ? WHERE id = ?",
        params![payload.status, now, id],
    );

    match result {
        Ok(rows) if rows > 0 => {
            let _ = log_audit(&conn, &user.username, "UPDATE_CUSTOMER_STATUS", "customers", Some(&id), &format!("Updated customer status to {}", payload.status));
            Ok((StatusCode::OK, Json(json!({
                "id": id,
                "status": payload.status
            }))))
        }
        _ => Err((StatusCode::NOT_FOUND, Json(json!({"error": "Customer not found"})))),
    }
}

#[derive(Deserialize)]
struct UpdateTierPayload {
    tier: String,
}

async fn update_customer_tier(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateTierPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Finance"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let result = conn.execute(
        "UPDATE customers SET tier = ?, updated_at = ? WHERE id = ?",
        params![payload.tier, now, id],
    );

    match result {
        Ok(rows) if rows > 0 => {
            let _ = log_audit(&conn, &user.username, "UPDATE_CUSTOMER_TIER", "customers", Some(&id), &format!("Updated customer tier to {}", payload.tier));
            Ok((StatusCode::OK, Json(json!({
                "id": id,
                "tier": payload.tier
            }))))
        }
        _ => Err((StatusCode::NOT_FOUND, Json(json!({"error": "Customer not found"})))),
    }
}

async fn delete_customer(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let result = conn.execute("DELETE FROM customers WHERE id = ?", [&id]);

    match result {
        Ok(rows) if rows > 0 => {
            let _ = log_audit(&conn, &user.username, "DELETE_CUSTOMER", "customers", Some(&id), "Deleted customer");
            Ok((StatusCode::OK, Json(json!({ "status": "deleted" }))))
        }
        _ => Err((StatusCode::NOT_FOUND, Json(json!({"error": "Customer not found"})))),
    }
}

// Runtimes Handlers
#[derive(Deserialize)]
struct CreateRuntimePayload {
    environment: String,
    runtime_api_url: String,
}

async fn create_customer_runtime(
    user: AdminUser,
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
    Json(payload): Json<CreateRuntimePayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let runtime_id = Uuid::new_v4().to_string();

    // Generate secure runtime API key
    let raw_key = format!("ma_{}", Uuid::new_v4().simple());
    let key_hash = hash_sha256(&raw_key);

    let result = conn.execute(
        "INSERT INTO customer_runtimes (id, customer_id, environment, status, api_key_hash, runtime_api_url, created_at, updated_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            runtime_id,
            customer_id,
            payload.environment,
            "Active",
            key_hash,
            payload.runtime_api_url,
            now,
            now
        ],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_RUNTIME", "customer_runtimes", Some(&runtime_id), &format!("Generated runtime node for customer {}", customer_id));
            Ok((StatusCode::CREATED, Json(json!({
                "id": runtime_id,
                "customer_id": customer_id,
                "environment": payload.environment,
                "status": "Active",
                "runtime_api_url": payload.runtime_api_url,
                "api_key": raw_key
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Create runtime failed: {}", e) })),
        )),
    }
}

async fn list_runtimes(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, customer_id, environment, status, runtime_api_url, last_heartbeat_at FROM customer_runtimes").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "customer_id": row.get::<_, String>(1)?,
            "environment": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "runtime_api_url": row.get::<_, String>(4)?,
            "last_heartbeat_at": row.get::<_, Option<String>>(5)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let runtimes: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(runtimes)))
}

// Subscriptions Handlers
#[derive(Deserialize)]
struct CreateSubscriptionPayload {
    customer_id: String,
    plan_id: String,
    amount: i64,
    payment_status: String,
}

async fn list_subscriptions(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Finance"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, customer_id, plan_id, status, payment_status, current_period_start, current_period_end, amount, created_at FROM subscriptions").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "customer_id": row.get::<_, String>(1)?,
            "plan_id": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "payment_status": row.get::<_, String>(4)?,
            "current_period_start": row.get::<_, String>(5)?,
            "current_period_end": row.get::<_, String>(6)?,
            "amount": row.get::<_, i64>(7)?,
            "created_at": row.get::<_, String>(8)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let subs: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(subs)))
}

async fn create_subscription(
    user: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateSubscriptionPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Finance"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now();
    let end = now + chrono::Duration::days(30);
    let sub_id = Uuid::new_v4().to_string();

    let result = conn.execute(
        "INSERT INTO subscriptions (id, customer_id, plan_id, status, payment_status, current_period_start, current_period_end, amount, created_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            sub_id,
            payload.customer_id,
            payload.plan_id,
            "Active",
            payload.payment_status,
            now.to_rfc3339(),
            end.to_rfc3339(),
            payload.amount,
            now.to_rfc3339()
        ],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_SUBSCRIPTION", "subscriptions", Some(&sub_id), &format!("Created subscription to plan {} for customer {}", payload.plan_id, payload.customer_id));
            Ok((StatusCode::CREATED, Json(json!({
                "id": sub_id,
                "customer_id": payload.customer_id,
                "plan_id": payload.plan_id,
                "status": "Active",
                "amount": payload.amount
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Create subscription failed: {}", e) })),
        )),
    }
}

async fn check_subscriptions(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Finance"])?;
    
    // Call background worker routine manually
    crate::workers::run_billing_cycle(&state.db).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;

    let conn = state.db.conn.lock().unwrap();
    let past_due_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM subscriptions WHERE status = 'Past Due'",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    let processed_renewals: i64 = conn.query_row(
        "SELECT COUNT(*) FROM subscriptions WHERE status = 'Active'",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    Ok((StatusCode::OK, Json(json!({
        "processed_renewals": processed_renewals,
        "past_due_count": past_due_count
    }))))
}

// Packages Handlers
#[derive(Deserialize)]
struct CreatePackagePayload {
    name: String,
    description: String,
    signature_pubkey: String,
}

async fn list_packages(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer", "Finance", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, description, latest_version, status, developer_id, signature_pubkey, created_at FROM packages").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "description": row.get::<_, Option<String>>(2)?,
            "latest_version": row.get::<_, Option<String>>(3)?,
            "status": row.get::<_, String>(4)?,
            "developer_id": row.get::<_, Option<String>>(5)?,
            "signature_pubkey": row.get::<_, String>(6)?,
            "created_at": row.get::<_, String>(7)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let packages: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(packages)))
}

async fn create_package(
    user: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<CreatePackagePayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let package_id = Uuid::new_v4().to_string();

    let result = conn.execute(
        "INSERT INTO packages (id, name, description, latest_version, status, developer_id, signature_pubkey, created_at, updated_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            package_id,
            payload.name,
            payload.description,
            None::<String>,
            "Pending",
            user.username, // Record developer username/id
            payload.signature_pubkey,
            now,
            now
        ],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_PACKAGE", "packages", Some(&package_id), &format!("Created package {}", payload.name));
            Ok((StatusCode::CREATED, Json(json!({
                "id": package_id,
                "name": payload.name,
                "status": "Pending"
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Create package failed: {}", e) })),
        )),
    }
}

async fn get_package(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer", "Finance", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut pkg_val = conn.query_row(
        "SELECT id, name, description, latest_version, status, developer_id, signature_pubkey, created_at FROM packages WHERE id = ?",
        [&id],
        |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, Option<String>>(2)?,
                "latest_version": row.get::<_, Option<String>>(3)?,
                "status": row.get::<_, String>(4)?,
                "developer_id": row.get::<_, Option<String>>(5)?,
                "signature_pubkey": row.get::<_, String>(6)?,
                "created_at": row.get::<_, String>(7)?,
            }))
        },
    ).map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "Package not found"}))))?;

    // Fetch versions
    let mut stmt_versions = conn.prepare(
        "SELECT id, version, checksum_sha256, signature, binary_url, status, created_at FROM package_versions WHERE package_id = ?"
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let versions_rows = stmt_versions.query_map([&id], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "version": row.get::<_, String>(1)?,
            "checksum_sha256": row.get::<_, String>(2)?,
            "signature": row.get::<_, String>(3)?,
            "binary_url": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "created_at": row.get::<_, String>(6)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    let versions: Vec<serde_json::Value> = versions_rows.flatten().collect();

    if let Some(obj) = pkg_val.as_object_mut() {
        obj.insert("versions".to_string(), json!(versions));
    }

    Ok((StatusCode::OK, Json(pkg_val)))
}

#[derive(Deserialize)]
struct CreateVersionPayload {
    version: String,
    checksum_sha256: String,
    signature: String,
    binary_url: String,
}

async fn create_package_version(
    user: AdminUser,
    State(state): State<AppState>,
    Path(package_id): Path<String>,
    Json(payload): Json<CreateVersionPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();

    // Query package signature public key to verify
    let signature_pubkey: String = conn.query_row(
        "SELECT signature_pubkey FROM packages WHERE id = ?",
        [&package_id],
        |row| row.get(0)
    ).map_err(|_| (StatusCode::NOT_FOUND, Json(json!({"error": "Package parent not registered"}))))?;

    // Verify cryptographic signature!
    if let Err(err_msg) = verify_ed25519(&signature_pubkey, &payload.signature, &payload.checksum_sha256) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Cryptographic signature verification failed: {}", err_msg) })),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let version_id = Uuid::new_v4().to_string();

    // Insert version record
    let result = conn.execute(
        "INSERT INTO package_versions (id, package_id, version, checksum_sha256, signature, binary_url, status, created_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            version_id,
            package_id,
            payload.version,
            payload.checksum_sha256,
            payload.signature,
            payload.binary_url,
            "Pending",
            now
        ],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_PACKAGE_VERSION", "package_versions", Some(&version_id), &format!("Registered package version {}", version_id));
            Ok((StatusCode::CREATED, Json(json!({
                "id": version_id,
                "version": payload.version,
                "status": "Pending"
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Register package version failed: {}", e) })),
        )),
    }
}

async fn list_pending_packages(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer", "Finance", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT pv.id, pv.package_id, p.name, pv.version, pv.checksum_sha256, pv.binary_url, pv.created_at 
         FROM package_versions pv 
         JOIN packages p ON pv.package_id = p.id 
         WHERE pv.status = 'Pending'"
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "package_id": row.get::<_, String>(1)?,
            "package_name": row.get::<_, String>(2)?,
            "version": row.get::<_, String>(3)?,
            "checksum_sha256": row.get::<_, String>(4)?,
            "binary_url": row.get::<_, String>(5)?,
            "created_at": row.get::<_, String>(6)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let pending: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(pending)))
}

async fn approve_version(
    user: AdminUser,
    State(state): State<AppState>,
    Path((package_id, version)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin"])?;
    
    let conn = state.db.conn.lock().unwrap();
    
    let rows = conn.execute(
        "UPDATE package_versions SET status = 'Approved' WHERE package_id = ? AND version = ? AND status = 'Pending'",
        params![package_id, version],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    if rows == 0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No pending package version matched"}))));
    }

    // Update package status and latest_version
    let _ = conn.execute(
        "UPDATE packages SET latest_version = ?, status = 'Approved' WHERE id = ?",
        params![version, package_id],
    );

    let _ = log_audit(&conn, &user.username, "APPROVE_PACKAGE_VERSION", "package_versions", Some(&package_id), &format!("Approved version {}", version));

    Ok((StatusCode::OK, Json(json!({ "success": true }))))
}

#[derive(Deserialize)]
struct RejectPayload {
    reason: String,
}

async fn reject_version(
    user: AdminUser,
    State(state): State<AppState>,
    Path((package_id, version)): Path<(String, String)>,
    Json(payload): Json<RejectPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin"])?;
    
    let conn = state.db.conn.lock().unwrap();
    
    let rows = conn.execute(
        "UPDATE package_versions SET status = 'Rejected' WHERE package_id = ? AND version = ? AND status = 'Pending'",
        params![package_id, version],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    if rows == 0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "No pending package version matched"}))));
    }

    // Update package status to rejected if matches
    let _ = conn.execute(
        "UPDATE packages SET status = 'Rejected' WHERE id = ?",
        params![package_id],
    );

    let _ = log_audit(&conn, &user.username, "REJECT_PACKAGE_VERSION", "package_versions", Some(&package_id), &format!("Rejected version {} (reason: {})", version, payload.reason));

    Ok((StatusCode::OK, Json(json!({ "success": true }))))
}

// Deployments Handlers
#[derive(Deserialize)]
struct CreateDeploymentPayload {
    package_id: String,
    version: String,
}

async fn list_deployments(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, runtime_id, package_id, version, status, error_message, installed_at, created_at FROM deployments"
    ).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "runtime_id": row.get::<_, String>(1)?,
            "package_id": row.get::<_, String>(2)?,
            "version": row.get::<_, String>(3)?,
            "status": row.get::<_, String>(4)?,
            "error_message": row.get::<_, Option<String>>(5)?,
            "installed_at": row.get::<_, Option<String>>(6)?,
            "created_at": row.get::<_, String>(7)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let deployments: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(deployments)))
}

async fn create_deployment(
    user: AdminUser,
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
    Json(payload): Json<CreateDeploymentPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();

    // Verify version exists and is approved
    let version_status: String = conn.query_row(
        "SELECT status FROM package_versions WHERE package_id = ? AND version = ?",
        params![payload.package_id, payload.version],
        |row| row.get(0),
    ).map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Requested package version not registered"}))))?;

    if version_status != "Approved" {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Requested package version is not approved for deployment"}))));
    }

    let deployment_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO deployments (id, runtime_id, package_id, version, status, created_at) 
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            deployment_id,
            runtime_id,
            payload.package_id,
            payload.version,
            "Installing",
            now
        ],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let _ = log_audit(&conn, &user.username, "CREATE_DEPLOYMENT", "deployments", Some(&deployment_id), &format!("Triggered deployment of {}@{} to runtime {}", payload.package_id, payload.version, runtime_id));

    Ok((StatusCode::CREATED, Json(json!({
        "deployment_id": deployment_id,
        "status": "Installing"
    }))))
}

async fn create_rollback(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();

    // Fetch deployment details to execute rollback
    let (runtime_id, package_id, current_version): (String, String, String) = conn.query_row(
        "SELECT runtime_id, package_id, version FROM deployments WHERE id = ?",
        [&id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Deployment record not found"}))))?;

    // Find the previous approved version for rollback target
    let target_version: String = conn.query_row(
        "SELECT version FROM package_versions WHERE package_id = ? AND status = 'Approved' AND version != ? ORDER BY created_at DESC LIMIT 1",
        params![package_id, current_version],
        |row| row.get(0),
    ).map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "No rollback target version found"}))))?;

    let rollback_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO rollbacks (id, deployment_id, from_version, to_version, reason, rolled_back_at) 
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            rollback_id,
            id,
            current_version,
            target_version,
            "Administrative Rollback",
            now
        ],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    // Spawn a new deployment that represents the rollback installation
    let new_deployment_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO deployments (id, runtime_id, package_id, version, status, created_at) 
         VALUES (?, ?, ?, ?, ?, ?)",
        params![
            new_deployment_id,
            runtime_id,
            package_id,
            target_version,
            "Installing",
            now
        ],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;

    let _ = log_audit(&conn, &user.username, "ROLLBACK_DEPLOYMENT", "rollbacks", Some(&rollback_id), &format!("Initiated rollback of deployment {} to version {}", id, target_version));

    Ok((StatusCode::CREATED, Json(json!({
        "rollback_deployment_id": rollback_id,
        "status": "Installing"
    }))))
}

// Incidents Handlers
#[derive(Deserialize)]
struct CreateIncidentPayload {
    runtime_id: String,
    incident_type: String,
    severity: String,
    summary: String,
    details: Option<String>,
}

async fn list_incidents(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer", "Read Only"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, runtime_id, incident_type, severity, summary, details, status, reported_at, resolved_at FROM incidents").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "runtime_id": row.get::<_, String>(1)?,
            "incident_type": row.get::<_, String>(2)?,
            "severity": row.get::<_, String>(3)?,
            "summary": row.get::<_, String>(4)?,
            "details": row.get::<_, Option<String>>(5)?,
            "status": row.get::<_, String>(6)?,
            "reported_at": row.get::<_, String>(7)?,
            "resolved_at": row.get::<_, Option<String>>(8)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let incidents: Vec<serde_json::Value> = rows.flatten().collect();
    Ok((StatusCode::OK, Json(incidents)))
}

async fn create_incident(
    user: AdminUser,
    State(state): State<AppState>,
    Json(payload): Json<CreateIncidentPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();
    let incident_id = Uuid::new_v4().to_string();

    let result = conn.execute(
        "INSERT INTO incidents (id, runtime_id, incident_type, severity, summary, details, status, reported_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            incident_id,
            payload.runtime_id,
            payload.incident_type,
            payload.severity,
            payload.summary,
            payload.details,
            "Open",
            now
        ],
    );

    match result {
        Ok(_) => {
            let _ = log_audit(&conn, &user.username, "CREATE_INCIDENT", "incidents", Some(&incident_id), &format!("Manually created incident: {}", payload.summary));
            Ok((StatusCode::CREATED, Json(json!({
                "id": incident_id,
                "runtime_id": payload.runtime_id,
                "incident_type": payload.incident_type,
                "severity": payload.severity,
                "summary": payload.summary,
                "status": "Open",
                "reported_at": now
            }))))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Create incident failed: {}", e) })),
        )),
    }
}

#[derive(Deserialize)]
struct UpdateIncidentStatusPayload {
    status: String,
}

async fn update_incident_status(
    user: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateIncidentStatusPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin", "Developer", "Support Engineer"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();

    let result = if payload.status == "Resolved" {
        conn.execute(
            "UPDATE incidents SET status = ?, resolved_at = ? WHERE id = ?",
            params![payload.status, now, id],
        )
    } else {
        conn.execute(
            "UPDATE incidents SET status = ? WHERE id = ?",
            params![payload.status, id],
        )
    };

    match result {
        Ok(rows) if rows > 0 => {
            let _ = log_audit(&conn, &user.username, "UPDATE_INCIDENT_STATUS", "incidents", Some(&id), &format!("Updated incident status to {}", payload.status));
            Ok((StatusCode::OK, Json(json!({ "id": id, "status": payload.status }))))
        }
        _ => Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Incident not found or update failed"})))),
    }
}

// Audit Logs Handlers
async fn list_audit_logs(
    user: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    check_role(&user, &["Super Admin"])?;
    
    let conn = state.db.conn.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, username, action, target_type, target_id, details, ip_address, created_at FROM audit_logs ORDER BY created_at DESC").map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})))
    })?;
    
    let rows = stmt.query_map([], |row| {
        Ok(json!({
            "id": row.get::<_, String>(0)?,
            "username": row.get::<_, String>(1)?,
            "action": row.get::<_, String>(2)?,
            "target_type": row.get::<_, Option<String>>(3)?,
            "target_id": row.get::<_, Option<String>>(4)?,
            "details": row.get::<_, Option<String>>(5)?,
            "ip_address": row.get::<_, Option<String>>(6)?,
            "created_at": row.get::<_, String>(7)?,
        }))
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))?;
    
    let logs: Vec<serde_json::Value> = rows.flatten().collect();
    let total = logs.len();
    Ok((StatusCode::OK, Json(json!({ "logs": logs, "total": total }))))
}

// Runtime Heartbeat Endpoint
async fn runtime_heartbeat(
    _client: RuntimeClient,
) -> impl IntoResponse {
    // RuntimeClient extractor already validates the X-API-Key and updates the last_heartbeat_at timestamp
    (StatusCode::OK, Json(json!({ "success": true })))
}

#[derive(Deserialize)]
struct RuntimeDeploymentCallbackPayload {
    status: String,
    error_message: Option<String>,
}

async fn runtime_deployment_callback(
    State(state): State<AppState>,
    Path(deployment_id): Path<String>,
    Json(payload): Json<RuntimeDeploymentCallbackPayload>,
) -> impl IntoResponse {
    let conn = state.db.conn.lock().unwrap();
    let now = Utc::now().to_rfc3339();

    let result = conn.execute(
        "UPDATE deployments SET status = ?, error_message = ?, installed_at = ? WHERE id = ?",
        params![payload.status, payload.error_message, now, deployment_id],
    );

    match result {
        Ok(rows) if rows > 0 => {
            let _ = log_audit(&conn, "runtime_client", "DEPLOYMENT_CALLBACK", "deployments", Some(&deployment_id), &format!("Runtime callback updated deployment status to {}", payload.status));
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Deployment record not found" }))).into_response()
    }
}
