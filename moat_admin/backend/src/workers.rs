use std::time::Duration;
use chrono::{DateTime, Utc};
use rusqlite::params;
use croner::Cron;
use crate::db::DbStore;

pub async fn start_workers(db: DbStore) {
    // 1. Spawn Heartbeat Monitor (Runs every 60 seconds)
    let db_clone = db.clone();
    tokio::spawn(async move {
        println!("[Workers] Starting 60s Heartbeat Monitor...");
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = run_heartbeat_check(&db_clone).await {
                eprintln!("[Heartbeat Monitor] Error: {:?}", e);
            }
        }
    });

    // 2. Spawn Daily Billing Worker (Runs at midnight using croner)
    let db_clone = db.clone();
    tokio::spawn(async move {
        println!("[Workers] Starting midnight Daily Billing Worker...");
        let cron = match Cron::new("0 0 * * *").parse() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[Billing Worker] Failed to parse cron expression: {:?}", e);
                return;
            }
        };

        loop {
            let now = Utc::now();
            if let Ok(next) = cron.find_next_occurrence(&now, false) {
                let duration = match (next - now).to_std() {
                    Ok(d) => d,
                    Err(_) => Duration::from_secs(0),
                };
                
                tokio::time::sleep(duration).await;
                
                println!("[Billing Worker] Triggered at midnight Utc.");
                if let Err(e) = run_billing_cycle(&db_clone).await {
                    eprintln!("[Billing Worker] Error: {:?}", e);
                }
            } else {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        }
    });
}

pub async fn run_heartbeat_check(db: &DbStore) -> Result<(), rusqlite::Error> {
    let conn = db.conn.lock().unwrap();
    let now = Utc::now();

    // Query active customer runtimes to check heartbeats
    let mut stmt = conn.prepare(
        "SELECT r.id, c.organization_name, r.last_heartbeat_at 
         FROM customer_runtimes r
         JOIN customers c ON r.customer_id = c.id
         WHERE c.status = 'Active' AND r.status = 'Active'"
    )?;
    
    let runtimes = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut unresponsive_runtimes = Vec::new();
    for rt in runtimes {
        if let Ok((id, org_name, last_hb_str)) = rt {
            let is_offline = match last_hb_str {
                Some(hb_str) => {
                    if let Ok(last_hb) = DateTime::parse_from_rfc3339(&hb_str) {
                        let diff = now.signed_duration_since(last_hb);
                        diff.num_seconds() > 180 // 3 missed heartbeats (180s)
                    } else {
                        true // Invalid format -> offline
                    }
                }
                None => true, // Never checked in
            };

            if is_offline {
                unresponsive_runtimes.push((id, org_name));
            }
        }
    }

    // Process offline runtimes and open incidents
    for (runtime_id, org_name) in unresponsive_runtimes {
        // Prevent incident storming: check if "Heartbeat Missing" incident is already open for this runtime_id
        let mut check_stmt = conn.prepare(
            "SELECT COUNT(*) FROM incidents WHERE runtime_id = ? AND incident_type = 'heartbeat_missed' AND status = 'Open'"
        )?;
        let open_count: i64 = check_stmt.query_row([&runtime_id], |row| row.get(0))?;

        if open_count == 0 {
            let incident_id = uuid::Uuid::new_v4().to_string();
            let now_str = now.to_rfc3339();
            
            // Set runtime status to Unreachable
            let _ = conn.execute(
                "UPDATE customer_runtimes SET status = 'Unreachable' WHERE id = ?",
                params![runtime_id],
            );

            // Insert incident
            conn.execute(
                "INSERT INTO incidents (id, runtime_id, incident_type, severity, summary, details, status, reported_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    incident_id,
                    runtime_id,
                    "heartbeat_missed",
                    "Critical",
                    "Heartbeat Missing",
                    format!("Runtime {} for org {} missed heartbeats for over 180s.", runtime_id, org_name),
                    "Open",
                    now_str
                ],
            )?;

            // Insert audit log
            conn.execute(
                "INSERT INTO audit_logs (id, username, action, target_type, target_id, details, created_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    "system",
                    "CREATE_INCIDENT",
                    "incidents",
                    incident_id,
                    format!("Opened incident for missing heartbeat on runtime {}", runtime_id),
                    now_str
                ],
            )?;
            println!("[Heartbeat Monitor] Opened incident for unresponsive runtime: {} (org: {})", runtime_id, org_name);
        }
    }

    Ok(())
}

pub async fn run_billing_cycle(db: &DbStore) -> Result<(), rusqlite::Error> {
    let conn = db.conn.lock().unwrap();
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    // Query active subscriptions that are due for billing
    let mut stmt = conn.prepare(
        "SELECT s.id, s.customer_id, s.plan_id, s.amount, s.current_period_end, s.payment_status 
         FROM subscriptions s
         WHERE s.status = 'Active' AND s.current_period_end <= ?"
    )?;
    
    let due_subs = stmt.query_map([&now_str], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut subs_to_process = Vec::new();
    for sub in due_subs {
        if let Ok(s) = sub {
            subs_to_process.push(s);
        }
    }

    for (sub_id, customer_id, plan_id, amount, current_period_end, payment_status) in subs_to_process {
        println!("[Billing Worker] Processing renewal for subscription: {} (customer: {})", sub_id, customer_id);
        
        if payment_status == "SimulateFailure" {
            // Update subscription status to Past Due
            conn.execute(
                "UPDATE subscriptions SET status = 'Past Due' WHERE id = ?",
                params![sub_id],
            )?;

            // Suspend the customer org
            conn.execute(
                "UPDATE customers SET status = 'Suspended' WHERE id = ?",
                params![customer_id],
            )?;

            // Log incident for billing failure
            let incident_id = uuid::Uuid::new_v4().to_string();
            // Try to find a runtime_id for the incident mapping
            let runtime_id: Option<String> = conn.query_row(
                "SELECT id FROM customer_runtimes WHERE customer_id = ? LIMIT 1",
                [&customer_id],
                |row| row.get(0),
            ).ok();

            if let Some(rt_id) = runtime_id {
                conn.execute(
                    "INSERT INTO incidents (id, runtime_id, incident_type, severity, summary, details, status, reported_at) 
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        incident_id,
                        rt_id,
                        "user_reported",
                        "High",
                        "Subscription Past Due",
                        format!("Customer {} subscription {} renewal charge of ${:.2} failed.", customer_id, sub_id, (amount as f64) / 100.0),
                        "Open",
                        now_str
                    ],
                )?;
            }

            conn.execute(
                "INSERT INTO audit_logs (id, username, action, target_type, target_id, details, created_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    "system_billing",
                    "SUBSCRIPTION_BILL_FAILED",
                    "subscriptions",
                    sub_id,
                    format!("Failed to bill customer {} for subscription renewal", customer_id),
                    now_str
                ],
            )?;
        } else {
            // Advance subscription current_period_start and current_period_end
            let old_end_date = match DateTime::parse_from_rfc3339(&current_period_end) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => now,
            };
            let next_end = old_end_date + chrono::Duration::days(30);
            let next_end_str = next_end.to_rfc3339();

            conn.execute(
                "UPDATE subscriptions SET current_period_start = ?, current_period_end = ? WHERE id = ?",
                params![current_period_end, next_end_str, sub_id],
            )?;

            // Log the billing transaction in audit logs
            conn.execute(
                "INSERT INTO audit_logs (id, username, action, target_type, target_id, details, created_at) 
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    "system_billing",
                    "SUBSCRIPTION_BILL_SUCCESS",
                    "subscriptions",
                    sub_id,
                    format!("Successfully renewed subscription {} for plan '{}' (amount: ${:.2})", sub_id, plan_id, (amount as f64) / 100.0),
                    now_str
                ],
            )?;
        }
    }

    Ok(())
}
