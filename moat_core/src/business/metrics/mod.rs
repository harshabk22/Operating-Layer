use crate::MoatResult;
use rusqlite::Connection;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReliabilityMetrics {
    pub average_message_latency: f64,
    pub average_booking_latency: f64,
    pub calendar_failures: i64,
    pub sheets_failures: i64,
    pub notification_failures: i64,
    pub total_retry_count: i64,
    pub pending_job_count: i64,
    pub pending_outbox_count: i64,
}

pub fn get_reliability_metrics(conn: &Connection) -> MoatResult<ReliabilityMetrics> {
    let avg_msg_latency = crate::business::db::get_avg_analytics_metric(conn, "message_latency", 0, 0)?;
    let avg_booking_latency =
        crate::business::db::get_avg_analytics_metric(conn, "booking_latency", 0, 0)?;

    let calendar_failures: i64 = conn.query_row(
        "SELECT metric_value FROM business_kpi_metrics WHERE metric_key = 'calendar_failures_total'",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let sheets_failures: i64 = conn.query_row(
        "SELECT metric_value FROM business_kpi_metrics WHERE metric_key = 'sheets_failures_total'",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let notification_failures: i64 = conn.query_row(
        "SELECT metric_value FROM business_kpi_metrics WHERE metric_key = 'notification_failures_total'",
        [],
        |r| r.get(0),
    ).unwrap_or(0);

    let total_retry_count: i64 = conn
        .query_row(
            "SELECT metric_value FROM business_kpi_metrics WHERE metric_key = 'total_retry_count'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let pending_job_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_background_jobs WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let pending_outbox_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM business_message_outbox WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(ReliabilityMetrics {
        average_message_latency: avg_msg_latency,
        average_booking_latency: avg_booking_latency,
        calendar_failures,
        sheets_failures,
        notification_failures,
        total_retry_count,
        pending_job_count,
        pending_outbox_count,
    })
}
