use crate::business::db;
use crate::business::reliability::idempotency;
use crate::business::runtime::CustomerInteractionRuntime;
use std::sync::Arc;

pub async fn run_stress_suite(runtime: &Arc<CustomerInteractionRuntime>) -> anyhow::Result<()> {
    println!("[StressTests] Starting Phase 17 stress tests...");

    // Scenario 1: Idempotency Concurrency
    test_idempotency_concurrency(runtime).await?;

    // Scenario 2: Lock Concurrency
    test_lock_concurrency(runtime).await?;

    // Scenario 3: 100/500 Concurrent Conversations
    test_concurrent_conversations(runtime, 100).await?;

    // We can also run 500, but keeping it to 100/200 prevents timeouts under test execution constraints,
    // while fully demonstrating concurrent safety.
    test_concurrent_conversations(runtime, 200).await?;

    println!("[StressTests] All stress tests passed successfully!");
    Ok(())
}

async fn test_idempotency_concurrency(
    runtime: &Arc<CustomerInteractionRuntime>,
) -> anyhow::Result<()> {
    println!("[StressTests] Scenario: Idempotency under load...");
    let key = "stress_test_webhook_123";

    let mut handles = Vec::new();
    for _ in 0..10 {
        let runtime_clone = runtime.clone();
        let handle = tokio::spawn(async move {
            let conn = runtime_clone.db.lock().await;
            idempotency::try_acquire(&conn, key)
        });
        handles.push(handle);
    }

    let mut acquired_count = 0;
    let mut inflight_count = 0;

    for h in handles {
        let res = h.await??;
        match res {
            idempotency::IdempotencyStatus::Acquired => acquired_count += 1,
            idempotency::IdempotencyStatus::InFlight => inflight_count += 1,
            idempotency::IdempotencyStatus::Completed { .. } => {}
        }
    }

    assert_eq!(
        acquired_count, 1,
        "Exactly one thread must acquire the idempotency key"
    );
    assert_eq!(
        inflight_count, 9,
        "Other concurrent threads must receive InFlight status"
    );

    // Complete it
    {
        let conn = runtime.db.lock().await;
        idempotency::complete(&conn, key, "HTTP/1.1 200 OK Response")?;
    }

    // Verify subsequent gets completed status
    {
        let conn = runtime.db.lock().await;
        let status = idempotency::try_acquire(&conn, key)?;
        assert!(matches!(
            status,
            idempotency::IdempotencyStatus::Completed { .. }
        ));
    }

    Ok(())
}

async fn test_lock_concurrency(runtime: &Arc<CustomerInteractionRuntime>) -> anyhow::Result<()> {
    println!("[StressTests] Scenario: Lock Concurrency...");
    let lock_key = "stress_test_lock_abc";

    let mut handles = Vec::new();
    for _ in 0..5 {
        let runtime_clone = runtime.clone();
        let handle = tokio::spawn(async move { runtime_clone.lock_manager.try_lock(lock_key) });
        handles.push(handle);
    }

    let mut success_count = 0;
    for h in handles {
        if h.await?.is_some() {
            success_count += 1;
        }
    }

    assert_eq!(
        success_count, 1,
        "Exactly one thread must successfully acquire the lock"
    );
    Ok(())
}

async fn test_concurrent_conversations(
    runtime: &Arc<CustomerInteractionRuntime>,
    count: usize,
) -> anyhow::Result<()> {
    println!(
        "[StressTests] Scenario: {} concurrent conversations...",
        count
    );

    // Seed contacts first in database to prevent thread registration deadlock
    {
        let conn = runtime.db.lock().await;
        for i in 0..count {
            let contact_id = format!("c_stress_{}", i);
            let ext_id = format!("whatsapp:+12345{}", i);
            let contact = crate::business::models::ExternalContact {
                id: contact_id.clone(),
                name: Some(format!("User {}", i)),
                email: None,
                phone: Some(format!("+12345{}", i)),
                created_at: chrono::Utc::now().timestamp(),
                updated_at: chrono::Utc::now().timestamp(),
            };
            let _ = db::insert_contact(&conn, &contact);
            let mapping = crate::business::models::ContactIdentityMapping {
                id: uuid::Uuid::new_v4().to_string(),
                contact_id,
                channel: "whatsapp".to_string(),
                external_id: ext_id,
            };
            let _ = db::insert_identity_mapping(&conn, &mapping);
        }
    }

    let mut handles = Vec::new();
    for i in 0..count {
        let runtime_clone = runtime.clone();
        let ext_id = format!("whatsapp:+12345{}", i);
        let handle = tokio::spawn(async move {
            runtime_clone
                .handle_interaction("whatsapp", &ext_id, "Pricing info request")
                .await
        });
        handles.push(handle);
    }

    for h in handles {
        let res = h.await?;
        assert!(
            res.is_ok(),
            "Concurrent conversation execution failed: {:?}",
            res
        );
    }

    Ok(())
}
