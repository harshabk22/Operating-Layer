use crate::MoatResult;
use rusqlite::Connection;
use std::fs::File;
use std::io::Write;

pub fn generate_reports(conn: &Connection) -> MoatResult<()> {
    generate_metrics_report(conn)?;
    generate_queue_report(conn)?;
    generate_opportunity_intelligence_report(conn)?;
    generate_decision_flow_report()?;
    Ok(())
}

fn get_report_path(filename: &str) -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("MOAT_WORKSPACE_DIR") {
        return std::path::PathBuf::from(dir).join(filename);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(parent) = cwd.parent() {
            if let Ok(content) = std::fs::read_to_string(parent.join("Cargo.toml")) {
                if content.contains("[workspace]") {
                    return parent.join(filename);
                }
            }
        }
        if let Ok(content) = std::fs::read_to_string(cwd.join("Cargo.toml")) {
            if content.contains("[workspace]") {
                return cwd.join(filename);
            }
        }
        return cwd.join(filename);
    }
    std::path::PathBuf::from(filename)
}

fn write_report(filename: &str, content: &str) -> MoatResult<()> {
    let path = get_report_path(filename);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match File::create(&path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(content.as_bytes()) {
                eprintln!("[Reports] Failed to write {}: {}", path.display(), e);
            }
        }
        Err(e) => {
            eprintln!("[Reports] Failed to create {}: {}", path.display(), e);
        }
    }
    Ok(())
}

fn generate_metrics_report(conn: &Connection) -> MoatResult<()> {
    let opportunities_created: i64 = conn
        .query_row("SELECT COUNT(*) FROM business_opportunities", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let opportunities_converted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_opportunities WHERE status = 'Converted'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let engagements_requested: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_scheduled_engagements",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let engagements_authorized: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_authorization_requests WHERE status = 'Approved'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let escalations_triggered: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_operator_escalations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let content = format!(
        "# Business Metrics KPI Report\n\n\
         This report details key performance indicators persisted by the Business Operating Layer.\n\n\
         ## Persisted Metric Counters\n\n\
         | Metric KPI | Current Value |\n\
         |---|---|\n\
         | **Opportunities Created** | {} |\n\
         | **Opportunities Converted** | {} |\n\
         | **Engagements Requested** | {} |\n\
         | **Engagements Authorized** | {} |\n\
         | **Escalations Triggered** | {} |\n\n\
         *Last Updated: {}*\n",
        opportunities_created,
        opportunities_converted,
        engagements_requested,
        engagements_authorized,
        escalations_triggered,
        chrono::Utc::now().to_rfc3339()
    );

    write_report("BUSINESS_METRICS.md", &content)
}

fn generate_queue_report(conn: &Connection) -> MoatResult<()> {
    let mut stmt = conn
        .prepare("SELECT thread_id, customer_id, reason, priority, created_at FROM business_operator_queue ORDER BY created_at DESC")
        ?;
    let mut rows = stmt.query([])?;

    let mut table_rows = String::new();
    while let Some(row) = rows.next()? {
        let thread_id: String = row.get(0)?;
        let customer_id: String = row.get(1)?;
        let reason: String = row.get(2)?;
        let priority: String = row.get(3)?;
        let created_at: i64 = row.get(4)?;

        table_rows.push_str(&format!(
            "| `{}` | `{}` | {} | **{}** | {} |\n",
            thread_id, customer_id, reason, priority, created_at
        ));
    }

    let content = format!(
        "# Operator Queue Model\n\n\
         This report details the backend operator dashboard queue data model and currently escalated queue items.\n\n\
         ## Operator Queue Items\n\n\
         | Thread ID | Customer ID | Reason | Priority | Created At |\n\
         |---|---|---|---|---|\n\
         {}\n\
         *Last Updated: {}*\n",
        if table_rows.is_empty() { "| (Queue is empty) | | | | | \n".to_string() } else { table_rows },
        chrono::Utc::now().to_rfc3339()
    );

    write_report("OPERATOR_QUEUE_MODEL.md", &content)
}

fn generate_opportunity_intelligence_report(conn: &Connection) -> MoatResult<()> {
    let mut stmt = conn
        .prepare("SELECT opportunity_id, intent_score, urgency_score, budget_score, qualification_score, total_score, rating, updated_at FROM business_opportunity_scores ORDER BY updated_at DESC")
        ?;
    let mut rows = stmt.query([])?;

    let mut table_rows = String::new();
    while let Some(row) = rows.next()? {
        let opp_id: String = row.get(0)?;
        let intent: i32 = row.get(1)?;
        let urgency: i32 = row.get(2)?;
        let budget: i32 = row.get(3)?;
        let qual: i32 = row.get(4)?;
        let total: i32 = row.get(5)?;
        let rating: String = row.get(6)?;

        table_rows.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | **{}** | **{}** |\n",
            opp_id, intent, urgency, budget, qual, total, rating
        ));
    }

    let content = format!(
        "# Opportunity Intelligence Report\n\n\
         This report details scored opportunities qualified by the OpportunityEvaluator.\n\n\
         ## Scored Opportunities\n\n\
         | Opportunity ID | Intent Score | Urgency Score | Budget Score | Qualification Score | Total Score | Rating |\n\
         |---|---|---|---|---|---|---|\n\
         {}\n\
         *Last Updated: {}*\n",
        if table_rows.is_empty() { "| (No opportunities scored yet) | | | | | | \n".to_string() } else { table_rows },
        chrono::Utc::now().to_rfc3339()
    );

    write_report("OPPORTUNITY_INTELLIGENCE.md", &content)
}

fn generate_decision_flow_report() -> MoatResult<()> {
    let content = "# Engagement Decision Flow\n\n\
         This document outlines the evaluation routing logic applied by the Business Policy Layer.\n\n\
         ## Flow Schema Diagram\n\n\
         ```\n\
         Inbound Message\n\
               │\n\
               ▼\n\
         Is intent 'refund'? ───────────────► ESCALATE\n\
               │ No\n\
               ▼\n\
         Is intent 'support'? ──────────────► ESCALATE\n\
               │ No\n\
               ▼\n\
         Is intent 'booking' or 'schedule'? ─► REQUEST_AUTHORIZATION\n\
               │ No\n\
               ▼\n\
         Is intent 'pricing'? ──────────────► ANSWER_ONLY\n\
               │ No\n\
               ▼\n\
         Is rating 'critical'? ─────────────► ESCALATE\n\
               │ No\n\
               ▼\n\
         Is rating 'hot' && objective is\n\
         qualify/schedule? ─────────────────► PROPOSE_ENGAGEMENT\n\
               │ No\n\
               ▼\n\
         Is intent 'close'? ────────────────► CLOSE_THREAD\n\
               │ No\n\
               ▼\n\
         Default ───────────────────────────► ANSWER_ONLY\n\
         ```\n";

    write_report("ENGAGEMENT_DECISION_FLOW.md", content)
}
