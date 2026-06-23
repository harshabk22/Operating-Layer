use crate::business::calendar::CalendarProvider;
use crate::business::db;
use crate::business::models::{AuthorizationStatus, EngagementStatus, ThreadStatus};
use crate::business::spreadsheet::SpreadsheetProvider;
use crate::business::workflow;
use crate::MoatError;
use crate::MoatResult;
use rusqlite::Connection;

pub struct BookingExecutionService;

impl BookingExecutionService {
    pub fn execute_booking(
        conn: &Connection,
        request_id: &str,
        calendar: &dyn CalendarProvider,
        spreadsheet: &dyn SpreadsheetProvider,
    ) -> MoatResult<String> {
        // 1. Get request
        let req = db::get_authorization_request(conn, request_id)?.ok_or_else(|| {
            MoatError::Orchestrator(format!("Authorization request {} not found", request_id))
        })?;

        if req.status != AuthorizationStatus::Pending {
            return Err(MoatError::Orchestrator(format!(
                "Cannot execute booking: request {} is not pending (status: {:?})",
                request_id, req.status
            )));
        }

        let engagement = db::get_engagement(conn, &req.engagement_id)?.ok_or_else(|| {
            MoatError::Orchestrator(format!("Engagement {} not found", req.engagement_id))
        })?;

        // 2. Calendar Event creation
        let calendar_event = match calendar.create_event(
            &engagement.title,
            engagement.start_time,
            engagement.end_time,
        ) {
            Ok(evt) => evt,
            Err(e) => {
                // Log error
                eprintln!("[BookingExecutionService] Calendar creation failed: {}", e);
                // Increment bookings_failed_total
                db::increment_kpi_metric(conn, "bookings_failed_total")?;
                // Return error (booking aborted, authorization remains Pending and visible)
                return Err(MoatError::Orchestrator(format!(
                    "Calendar booking aborted: {}",
                    e
                )));
            }
        };

        // Increment calendar_events_created_total
        db::increment_kpi_metric(conn, "calendar_events_created_total")?;

        // 3. Spreadsheet Entry
        if let Err(e) = spreadsheet.append_row(
            "Bookings",
            vec![
                engagement.id.clone(),
                engagement.contact_id.clone(),
                engagement.title.clone(),
                engagement.start_time.to_string(),
                engagement.end_time.to_string(),
                calendar_event.id.clone(),
            ],
        ) {
            eprintln!("[BookingExecutionService] Spreadsheet append failed: {}", e);
        }

        // 4. Appointment Record & Authorization Update
        db::update_engagement_status(conn, &engagement.id, EngagementStatus::Authorized)?;
        db::update_authorization_status(conn, &req.id, AuthorizationStatus::Approved)?;
        workflow::update_thread_status(conn, &req.thread_id, ThreadStatus::Active)?;

        // Increment KPI metrics
        db::increment_kpi_metric(conn, "authorization_approved_total")?;
        db::increment_kpi_metric(conn, "bookings_created_total")?;

        // 5. Confirmation Message
        let formatted_time = chrono::DateTime::from_timestamp(engagement.start_time, 0)
            .map(|dt| dt.to_rfc2822())
            .unwrap_or_else(|| engagement.start_time.to_string());
        let msg = format!(
            "Confirmed: Your appointment '{}' has been scheduled for {} (Calendar ID: {}).",
            engagement.title, formatted_time, calendar_event.id
        );

        Ok(msg)
    }
}
