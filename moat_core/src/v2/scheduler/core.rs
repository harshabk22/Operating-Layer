use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Available,
    Unavailable(String),
}

#[derive(Debug, Clone)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ResourceReservation {
    pub id: Uuid,
    pub participant_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    pub id: Uuid,
    pub title: String,
    pub reservations: Vec<ResourceReservation>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}

pub struct Scheduler {
    reservations: Arc<RwLock<HashMap<Uuid, Vec<ResourceReservation>>>>,
    events: Arc<RwLock<HashMap<Uuid, ScheduledEvent>>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            reservations: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn check_availability(
        &self,
        participant_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Availability {
        let lock = self.reservations.read().unwrap();
        if let Some(participant_reservations) = lock.get(&participant_id) {
            for res in participant_reservations {
                if start_time < res.end_time && end_time > res.start_time {
                    return Availability::Unavailable(format!(
                        "Conflict with reservation {}",
                        res.id
                    ));
                }
            }
        }
        Availability::Available
    }

    pub fn schedule_event(
        &self,
        title: String,
        participant_ids: Vec<Uuid>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ScheduledEvent, String> {
        if start_time >= end_time {
            return Err("Start time must be before end time".to_string());
        }

        // Must query availability prior to creating a ScheduledEvent
        for &pid in &participant_ids {
            if let Availability::Unavailable(reason) =
                self.check_availability(pid, start_time, end_time)
            {
                return Err(format!("Participant {} unavailable: {}", pid, reason));
            }
        }

        let mut lock = self.reservations.write().unwrap();

        for &pid in &participant_ids {
            if let Some(participant_reservations) = lock.get(&pid) {
                for res in participant_reservations {
                    if start_time < res.end_time && end_time > res.start_time {
                        return Err(format!(
                            "Participant {} unavailable: Conflict with reservation {}",
                            pid, res.id
                        ));
                    }
                }
            }
        }

        let mut new_reservations = Vec::new();
        for &pid in &participant_ids {
            let res = ResourceReservation {
                id: Uuid::new_v4(),
                participant_id: pid,
                start_time,
                end_time,
            };
            new_reservations.push(res.clone());
            lock.entry(pid).or_default().push(res);
        }

        let event = ScheduledEvent {
            id: Uuid::new_v4(),
            title,
            reservations: new_reservations,
            start_time,
            end_time,
        };

        let mut events_lock = self.events.write().unwrap();
        events_lock.insert(event.id, event.clone());

        Ok(event)
    }
}
