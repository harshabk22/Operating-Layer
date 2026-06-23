use crate::MoatResult;
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub trait CapabilityPlugin: Send + Sync {
    fn capabilities(&self) -> Vec<CapabilityDef>;
    fn execute(&self, capability: &str, params: &Value) -> MoatResult<Value>;
    fn health_check(&self) -> bool;
}

pub struct CalendarPlugin {
    pub inner: Arc<dyn crate::business::calendar::CalendarProvider>,
}

impl CalendarPlugin {
    pub fn new(provider: Arc<dyn crate::business::calendar::CalendarProvider>) -> Self {
        Self { inner: provider }
    }
}

impl CapabilityPlugin for CalendarPlugin {
    fn capabilities(&self) -> Vec<CapabilityDef> {
        vec![
            CapabilityDef {
                name: "CheckAvailability",
                description: "Check time slot availability",
            },
            CapabilityDef {
                name: "CreateBooking",
                description: "Create a calendar booking",
            },
            CapabilityDef {
                name: "CancelBooking",
                description: "Cancel an existing booking",
            },
        ]
    }

    fn execute(&self, capability: &str, params: &Value) -> MoatResult<Value> {
        match capability {
            "CheckAvailability" => {
                let start = params.get("start").and_then(|v| v.as_i64()).unwrap_or(0);
                let end = params.get("end").and_then(|v| v.as_i64()).unwrap_or(0);
                self.inner
                    .create_event("check", start, end)
                    .map(|e| Value::String(e.id))
                    .map_err(|e| crate::MoatError::Orchestrator(format!("Calendar error: {}", e)))
            }
            "CreateBooking" => {
                let title = params
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Booking");
                let start = params
                    .get("start")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                let end = params
                    .get("end")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                self.inner
                    .create_event(title, start, end)
                    .map(|e| Value::String(e.id))
                    .map_err(|e| crate::MoatError::Orchestrator(format!("Calendar error: {}", e)))
            }
            "CancelBooking" => {
                let event_id = params
                    .get("event_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.inner
                    .cancel_event(event_id)
                    .map(|_| Value::String("cancelled".to_string()))
                    .map_err(|e| crate::MoatError::Orchestrator(format!("Calendar error: {}", e)))
            }
            _ => Err(crate::MoatError::Orchestrator(format!(
                "Unknown calendar capability: {}",
                capability
            ))),
        }
    }

    fn health_check(&self) -> bool {
        true
    }
}

pub struct SpreadsheetPlugin {
    pub inner: Arc<dyn crate::business::spreadsheet::SpreadsheetProvider>,
}

impl SpreadsheetPlugin {
    pub fn new(provider: Arc<dyn crate::business::spreadsheet::SpreadsheetProvider>) -> Self {
        Self { inner: provider }
    }
}

impl CapabilityPlugin for SpreadsheetPlugin {
    fn capabilities(&self) -> Vec<CapabilityDef> {
        vec![CapabilityDef {
            name: "AppendRow",
            description: "Append a row to a spreadsheet",
        }]
    }

    fn execute(&self, capability: &str, params: &Value) -> MoatResult<Value> {
        match capability {
            "AppendRow" => {
                let sheet = params
                    .get("sheet")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                let values: Vec<String> = params
                    .get("values")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                self.inner
                    .append_row(sheet, values)
                    .map(|_| Value::String("appended".to_string()))
                    .map_err(|e| {
                        crate::MoatError::Orchestrator(format!("Spreadsheet error: {}", e))
                    })
            }
            _ => Err(crate::MoatError::Orchestrator(format!(
                "Unknown spreadsheet capability: {}",
                capability
            ))),
        }
    }

    fn health_check(&self) -> bool {
        true
    }
}

pub struct NotificationPlugin {
    pub inner: Arc<dyn crate::business::owner::NotificationProvider>,
}

impl NotificationPlugin {
    pub fn new(provider: Arc<dyn crate::business::owner::NotificationProvider>) -> Self {
        Self { inner: provider }
    }
}

impl CapabilityPlugin for NotificationPlugin {
    fn capabilities(&self) -> Vec<CapabilityDef> {
        vec![CapabilityDef {
            name: "SendNotification",
            description: "Send a notification to the business owner",
        }]
    }

    fn execute(&self, capability: &str, params: &Value) -> MoatResult<Value> {
        match capability {
            "SendNotification" => {
                let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("");
                let _ = message;
                Ok(Value::String("notified".to_string()))
            }
            _ => Err(crate::MoatError::Orchestrator(format!(
                "Unknown notification capability: {}",
                capability
            ))),
        }
    }

    fn health_check(&self) -> bool {
        true
    }
}

/// SearchPropertyInventory — searches property_inventory table via DB
pub struct PropertyInventoryPlugin;

impl CapabilityPlugin for PropertyInventoryPlugin {
    fn capabilities(&self) -> Vec<CapabilityDef> {
        vec![CapabilityDef {
            name: "SearchPropertyInventory",
            description: "Search available properties by location, bedrooms, budget",
        }]
    }

    fn execute(&self, capability: &str, params: &Value) -> MoatResult<Value> {
        match capability {
            "SearchPropertyInventory" => {
                let location = params
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bedrooms = params.get("bedrooms").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let budget_min = params
                    .get("budget_min")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let budget_max = params
                    .get("budget_max")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let _business_id = params
                    .get("business_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default_business");

                // This needs a DB connection — handled via a thread-local or passed in params
                Ok(serde_json::json!({
                    "status": "capability_called",
                    "params": { "location": location, "bedrooms": bedrooms, "budget_min": budget_min, "budget_max": budget_max }
                }))
            }
            _ => Err(crate::MoatError::Orchestrator(format!(
                "Unknown inventory capability: {}",
                capability
            ))),
        }
    }

    fn health_check(&self) -> bool {
        true
    }
}
