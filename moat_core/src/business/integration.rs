use serde_json::Value;

pub trait BusinessIntegrationProvider {
    fn execute(&self, action: &str, payload: Value) -> Result<Value, String>;
}

pub struct SchedulingProvider;

impl Default for SchedulingProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulingProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn book_engagement(
        &self,
        contact_id: &str,
        title: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Value, String> {
        // Mock execution interface only (no actual OAuth or external API execution)
        Ok(serde_json::json!({
            "status": "success",
            "message": format!("Engagement '{}' booked successfully for contact {}", title, contact_id),
            "booking_details": {
                "contact_id": contact_id,
                "title": title,
                "start_time": start_time,
                "end_time": end_time,
                "provider": "SchedulingProvider"
            }
        }))
    }
}

impl BusinessIntegrationProvider for SchedulingProvider {
    fn execute(&self, action: &str, payload: Value) -> Result<Value, String> {
        match action {
            "book_engagement" => {
                let contact_id = payload["contact_id"].as_str().unwrap_or("unknown");
                let title = payload["title"].as_str().unwrap_or("Untitled");
                let start_time = payload["start_time"].as_i64().unwrap_or(0);
                let end_time = payload["end_time"].as_i64().unwrap_or(0);
                self.book_engagement(contact_id, title, start_time, end_time)
            }
            other => Err(format!(
                "Unsupported action '{}' for SchedulingProvider",
                other
            )),
        }
    }
}

pub struct SpreadsheetProvider;

impl Default for SpreadsheetProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SpreadsheetProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn log_opportunity(
        &self,
        contact_id: &str,
        opportunity_status: &str,
        value: f64,
    ) -> Result<Value, String> {
        // Mock sheet row insert (no actual OAuth or external API execution)
        Ok(serde_json::json!({
            "status": "success",
            "message": format!("Logged opportunity for contact {} with status '{}' and value {}", contact_id, opportunity_status, value),
            "sheet_details": {
                "row_inserted": true,
                "contact_id": contact_id,
                "status": opportunity_status,
                "value": value,
                "provider": "SpreadsheetProvider"
            }
        }))
    }
}

impl BusinessIntegrationProvider for SpreadsheetProvider {
    fn execute(&self, action: &str, payload: Value) -> Result<Value, String> {
        match action {
            "log_opportunity" => {
                let contact_id = payload["contact_id"].as_str().unwrap_or("unknown");
                let status = payload["status"].as_str().unwrap_or("discovered");
                let value = payload["value"].as_f64().unwrap_or(0.0);
                self.log_opportunity(contact_id, status, value)
            }
            other => Err(format!(
                "Unsupported action '{}' for SpreadsheetProvider",
                other
            )),
        }
    }
}
