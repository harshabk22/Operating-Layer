use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;
use croner::Cron;
use rusqlite::params;
use std::str::FromStr;
use uuid::Uuid;

use crate::business::runtime::CustomerInteractionRuntime;
use crate::agent::ai_agent::AgentExecutionState;

pub fn start_cron_scheduler(runtime: Arc<CustomerInteractionRuntime>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Initialize next run times for schedules on startup
        if let Err(e) = initialize_cron_schedules(&runtime).await {
            eprintln!("[CronScheduler] Initialization error: {:?}", e);
        }

        loop {
            if let Err(e) = run_cron_iteration(&runtime).await {
                eprintln!("[CronScheduler] Loop error: {:?}", e);
            }
            sleep(Duration::from_secs(10)).await;
        }
    })
}

struct CronScheduleRecord {
    id: String,
    cron_expression: String,
    objective: String,
    allowed_tools_str: Option<String>,
}

async fn initialize_cron_schedules(runtime: &CustomerInteractionRuntime) -> anyhow::Result<()> {
    let conn = runtime.db.lock().await;
    let mut stmt = conn.prepare(
        "SELECT id, cron_expression FROM business_cron_schedules WHERE is_active = 1 AND next_run_at IS NULL",
    )?;
    let mut rows = stmt.query([])?;
    let mut updates = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let cron_expr: String = row.get(1)?;
        updates.push((id, cron_expr));
    }
    drop(rows);
    drop(stmt);

    let now = Utc::now();
    for (id, cron_expr) in updates {
        if let Ok(cron) = Cron::from_str(&cron_expr) {
            if let Ok(dt) = cron.find_next_occurrence(&now, false) {
                conn.execute(
                    "UPDATE business_cron_schedules SET next_run_at = ?1 WHERE id = ?2",
                    params![dt.timestamp(), id],
                )?;
            }
        }
    }
    Ok(())
}

async fn run_cron_iteration(runtime: &CustomerInteractionRuntime) -> anyhow::Result<()> {
    let now_epoch = Utc::now().timestamp();
    let conn = runtime.db.lock().await;

    let mut stmt = conn.prepare(
        "SELECT id, cron_expression, objective, allowed_tools
         FROM business_cron_schedules
         WHERE is_active = 1 AND (next_run_at IS NULL OR next_run_at <= ?1)",
    )?;

    let mut rows = stmt.query([now_epoch])?;
    let mut to_run = Vec::new();
    while let Some(row) = rows.next()? {
        to_run.push(CronScheduleRecord {
            id: row.get(0)?,
            cron_expression: row.get(1)?,
            objective: row.get(2)?,
            allowed_tools_str: row.get(3)?,
        });
    }

    drop(rows);
    drop(stmt);

    for schedule in to_run {
        println!("[CronScheduler] Triggering schedule: {} ({})", schedule.id, schedule.objective);

        let allowed_tools = if let Some(ref s) = schedule.allowed_tools_str {
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut state = AgentExecutionState {
            session_id: Uuid::new_v4(),
            objective: schedule.objective.clone(),
            context_trace: Vec::new(),
            turns: 0,
            allowed_tools,
            scope_stack: Vec::new(),
            total_tokens: 0,
            history_tokens: Vec::new(),
        };

        let ai_agent = Arc::clone(&runtime.ai_agent);
        let schedule_id = schedule.id.clone();
        let db_clone = Arc::clone(&runtime.db);
        let cron_expr = schedule.cron_expression.clone();

        tokio::spawn(async move {
            match ai_agent.run_agent_loop(&mut state, None).await {
                Ok(outcome) => {
                    println!("[CronScheduler] Schedule {} completed: {:?}", schedule_id, outcome);
                }
                Err(e) => {
                    eprintln!("[CronScheduler] Schedule {} failed: {:?}", schedule_id, e);
                }
            }

            let now = Utc::now();
            let next_run = if let Ok(cron) = Cron::from_str(&cron_expr) {
                cron.find_next_occurrence(&now, false).map(|dt| dt.timestamp()).ok()
            } else {
                None
            };

            if let Ok(conn) = db_clone.try_lock() {
                let _ = conn.execute(
                    "UPDATE business_cron_schedules
                     SET last_run_at = ?1, next_run_at = ?2
                     WHERE id = ?3",
                    params![now.timestamp(), next_run, schedule_id],
                );
            }
        });
    }

    Ok(())
}
