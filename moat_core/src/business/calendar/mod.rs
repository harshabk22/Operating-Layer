pub mod google;
pub mod provider;

#[cfg(test)]
pub mod mock_provider;
#[cfg(test)]
pub use mock_provider::MockCalendarProvider;

pub use google::GoogleCalendarProvider;
pub use provider::{CalendarEvent, CalendarProvider};
