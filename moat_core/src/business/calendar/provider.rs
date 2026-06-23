use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start_time: i64,
    pub end_time: i64,
}

pub trait CalendarProvider: Send + Sync {
    fn check_availability(&self, start_time: i64, end_time: i64) -> Result<bool>;
    fn create_event(&self, title: &str, start_time: i64, end_time: i64) -> Result<CalendarEvent>;
    fn update_event(
        &self,
        event_id: &str,
        title: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<CalendarEvent>;
    fn cancel_event(&self, event_id: &str) -> Result<()>;
}
