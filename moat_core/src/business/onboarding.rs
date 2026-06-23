use crate::business::db;
use crate::business::models::{BusinessProfile, OnboardingStatus};
use crate::MoatResult;
use rusqlite::Connection;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

pub struct OnboardingService {
    db: Arc<Mutex<Connection>>,
}

impl OnboardingService {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub async fn get_status(&self, business_id: &str) -> MoatResult<OnboardingStatus> {
        let conn = self.db.lock().await;
        if let Some(profile) = db::get_business_profile(&conn, business_id)? {
            if profile.onboarding_completed {
                Ok(OnboardingStatus::Complete { profile })
            } else {
                Ok(OnboardingStatus::Incomplete)
            }
        } else {
            Ok(OnboardingStatus::Incomplete)
        }
    }

    pub async fn update_profile(
        &self,
        business_id: &str,
        business_name: String,
        business_description: String,
        timezone: String,
        owner_name: String,
        owner_email: String,
    ) -> MoatResult<BusinessProfile> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let conn = self.db.lock().await;
        let mut completed = false;

        // Check if all required fields are provided
        if !business_name.is_empty()
            && !business_description.is_empty()
            && !timezone.is_empty()
            && !owner_name.is_empty()
            && !owner_email.is_empty()
        {
            completed = true;
        }

        db::insert_business_profile(
            &conn,
            business_id,
            &business_name,
            &business_description,
            &timezone,
            &owner_name,
            &owner_email,
            completed,
            timestamp,
        )?;

        let profile = db::get_business_profile(&conn, business_id)?.unwrap();
        Ok(profile)
    }

    pub async fn complete_onboarding(&self, business_id: &str) -> MoatResult<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let conn = self.db.lock().await;
        db::mark_onboarding_complete(&conn, business_id, timestamp)
    }
}
