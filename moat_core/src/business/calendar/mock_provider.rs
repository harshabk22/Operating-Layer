use super::provider::{CalendarEvent, CalendarProvider};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

pub struct MockCalendarProvider {
    pub should_fail: AtomicBool,
    events: Mutex<HashMap<String, CalendarEvent>>,
}

impl Default for MockCalendarProvider {
    fn default() -> Self {
        Self {
            should_fail: AtomicBool::new(false),
            events: Mutex::new(HashMap::new()),
        }
    }
}

impl MockCalendarProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_should_fail(&self, fail: bool) {
        self.should_fail.store(fail, Ordering::Relaxed);
    }

    pub fn get_event(&self, id: &str) -> Option<CalendarEvent> {
        let evts = self.events.lock().unwrap();
        evts.get(id).cloned()
    }
}

impl CalendarProvider for MockCalendarProvider {
    fn check_availability(&self, _start_time: i64, _end_time: i64) -> Result<bool> {
        if self.should_fail.load(Ordering::Relaxed) {
            return Err(anyhow!("Simulated calendar availability check failure"));
        }
        Ok(true)
    }

    fn create_event(&self, title: &str, start_time: i64, end_time: i64) -> Result<CalendarEvent> {
        if self.should_fail.load(Ordering::Relaxed) {
            return Err(anyhow!("Simulated calendar event creation failure"));
        }
        let id = Uuid::new_v4().to_string();
        let event = CalendarEvent {
            id: id.clone(),
            title: title.to_string(),
            start_time,
            end_time,
        };
        let mut evts = self.events.lock().unwrap();
        evts.insert(id, event.clone());
        Ok(event)
    }

    fn update_event(
        &self,
        event_id: &str,
        title: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<CalendarEvent> {
        if self.should_fail.load(Ordering::Relaxed) {
            return Err(anyhow!("Simulated calendar event update failure"));
        }
        let mut evts = self.events.lock().unwrap();
        if let Some(event) = evts.get_mut(event_id) {
            event.title = title.to_string();
            event.start_time = start_time;
            event.end_time = end_time;
            Ok(event.clone())
        } else {
            Err(anyhow!("Calendar event not found"))
        }
    }

    fn cancel_event(&self, event_id: &str) -> Result<()> {
        if self.should_fail.load(Ordering::Relaxed) {
            return Err(anyhow!("Simulated calendar event cancellation failure"));
        }
        let mut evts = self.events.lock().unwrap();
        if evts.remove(event_id).is_some() {
            Ok(())
        } else {
            Err(anyhow!("Calendar event not found"))
        }
    }
}
