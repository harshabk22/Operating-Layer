pub mod models;
pub mod service;

pub use models::*;
pub use service::*;

pub trait NotificationProvider: Send + Sync {
    fn notify_new_authorization(&self, auth_id: &str, thread_id: &str) -> anyhow::Result<()>;
    fn notify_new_escalation(
        &self,
        escalation_id: &str,
        thread_id: &str,
        reason: &str,
    ) -> anyhow::Result<()>;
    fn notify_booking_completed(&self, booking_id: &str, thread_id: &str) -> anyhow::Result<()>;
    fn notify_new_opportunity(&self, opportunity_id: &str, thread_id: &str) -> anyhow::Result<()>;
    fn notify_pipeline_stage_change(
        &self,
        opportunity_id: &str,
        from_stage: &str,
        to_stage: &str,
    ) -> anyhow::Result<()>;
}

pub struct ConsoleNotificationProvider;

impl Default for ConsoleNotificationProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleNotificationProvider {
    pub fn new() -> Self {
        Self
    }
}

impl NotificationProvider for ConsoleNotificationProvider {
    fn notify_new_authorization(&self, auth_id: &str, thread_id: &str) -> anyhow::Result<()> {
        println!("NOTIFICATION: New Authorization Required (auth: {}, thread: {})", auth_id, thread_id);
        Ok(())
    }
    fn notify_new_escalation(
        &self,
        escalation_id: &str,
        thread_id: &str,
        reason: &str,
    ) -> anyhow::Result<()> {
        println!("NOTIFICATION: New Escalation (esc: {}, thread: {}): {}", escalation_id, thread_id, reason);
        Ok(())
    }
    fn notify_booking_completed(&self, booking_id: &str, thread_id: &str) -> anyhow::Result<()> {
        println!("NOTIFICATION: Booking Completed (booking: {}, thread: {})", booking_id, thread_id);
        Ok(())
    }
    fn notify_new_opportunity(&self, opportunity_id: &str, thread_id: &str) -> anyhow::Result<()> {
        println!("NOTIFICATION: New Opportunity (opp: {}, thread: {})", opportunity_id, thread_id);
        Ok(())
    }
    fn notify_pipeline_stage_change(
        &self,
        opportunity_id: &str,
        from_stage: &str,
        to_stage: &str,
    ) -> anyhow::Result<()> {
        println!("NOTIFICATION: Pipeline Stage Changed (opp: {}): {} -> {}", opportunity_id, from_stage, to_stage);
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum MockNotification {
    NewAuthorization { auth_id: String, thread_id: String },
    NewEscalation { escalation_id: String, thread_id: String, reason: String },
    BookingCompleted { booking_id: String, thread_id: String },
    NewOpportunity { opportunity_id: String, thread_id: String },
    PipelineStageChange { opportunity_id: String, from_stage: String, to_stage: String },
}

#[cfg(test)]
pub struct MockNotificationProvider {
    pub notifications: std::sync::Mutex<Vec<MockNotification>>,
}

#[cfg(test)]
impl MockNotificationProvider {
    pub fn new() -> Self {
        Self { notifications: std::sync::Mutex::new(Vec::new()) }
    }
    
    pub fn get_notifications(&self) -> Vec<MockNotification> {
        self.notifications.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl NotificationProvider for MockNotificationProvider {
    fn notify_new_authorization(&self, auth_id: &str, thread_id: &str) -> anyhow::Result<()> {
        self.notifications.lock().unwrap().push(MockNotification::NewAuthorization { auth_id: auth_id.to_string(), thread_id: thread_id.to_string() });
        Ok(())
    }
    fn notify_new_escalation(&self, escalation_id: &str, thread_id: &str, reason: &str) -> anyhow::Result<()> {
        self.notifications.lock().unwrap().push(MockNotification::NewEscalation { escalation_id: escalation_id.to_string(), thread_id: thread_id.to_string(), reason: reason.to_string() });
        Ok(())
    }
    fn notify_booking_completed(&self, booking_id: &str, thread_id: &str) -> anyhow::Result<()> {
        self.notifications.lock().unwrap().push(MockNotification::BookingCompleted { booking_id: booking_id.to_string(), thread_id: thread_id.to_string() });
        Ok(())
    }
    fn notify_new_opportunity(&self, opportunity_id: &str, thread_id: &str) -> anyhow::Result<()> {
        self.notifications.lock().unwrap().push(MockNotification::NewOpportunity { opportunity_id: opportunity_id.to_string(), thread_id: thread_id.to_string() });
        Ok(())
    }
    fn notify_pipeline_stage_change(&self, opportunity_id: &str, from_stage: &str, to_stage: &str) -> anyhow::Result<()> {
        self.notifications.lock().unwrap().push(MockNotification::PipelineStageChange { opportunity_id: opportunity_id.to_string(), from_stage: from_stage.to_string(), to_stage: to_stage.to_string() });
        Ok(())
    }
}