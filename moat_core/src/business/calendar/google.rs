use crate::business::calendar::provider::{CalendarEvent, CalendarProvider};
use anyhow::{anyhow, Result};
use reqwest::Client;
use rusqlite::Connection;
use serde_json::json;
use std::sync::{Arc, Mutex};

pub struct GoogleCalendarProvider {
    client: Client,
    db: Arc<Mutex<Connection>>,
    business_id: String,
    base_url: String,
    oauth_url: String,
}

impl GoogleCalendarProvider {
    pub fn new(db: Arc<Mutex<Connection>>, business_id: String) -> Self {
        Self {
            client: Client::new(),
            db,
            business_id,
            base_url: "https://www.googleapis.com".to_string(),
            oauth_url: "https://oauth2.googleapis.com/token".to_string(),
        }
    }

    pub fn with_urls(
        db: Arc<Mutex<Connection>>,
        business_id: String,
        base_url: String,
        oauth_url: String,
    ) -> Self {
        Self {
            client: Client::new(),
            db,
            business_id,
            base_url,
            oauth_url,
        }
    }

    fn run_async<F, T>(&self, fut: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(move || handle.block_on(fut)),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                rt.block_on(fut)
            }
        }
    }

    async fn get_valid_token(&self) -> Result<String> {
        let creds = {
            let conn = self.db.lock().unwrap();
            crate::business::db::load_calendar_credentials(&conn, &self.business_id, "google")?
        };

        let (access_token, refresh_token, expires_at) = match creds {
            Some(c) => c,
            None => {
                return Err(anyhow!(
                    "No Google Calendar credentials found for business {}",
                    self.business_id
                ))
            }
        };

        let now = chrono::Utc::now().timestamp();
        if expires_at <= now + 30 {
            self.refresh_token_flow(&refresh_token).await
        } else {
            Ok(access_token)
        }
    }

    async fn refresh_token_flow(&self, refresh_token: &str) -> Result<String> {
        let client_id = {
            let conn = self.db.lock().unwrap();
            crate::business::db::read_google_config(&conn, &self.business_id)?
                .client_id
                .ok_or_else(|| anyhow!("Google client_id not found in configuration"))?
        };

        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
        let params = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];

        let refresh_url = &self.oauth_url;

        let response = self.client.post(refresh_url).form(&params).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to refresh Google OAuth token (status {}): {}",
                status,
                text
            ));
        }

        let body: serde_json::Value = response.json().await?;
        let new_access = body
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Token response missing access_token"))?
            .to_string();

        let expires_in = body
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);

        let new_expires_at = chrono::Utc::now().timestamp() + expires_in;

        // Save back to DB
        {
            let conn = self.db.lock().unwrap();
            crate::business::db::save_calendar_credentials(
                &conn,
                &self.business_id,
                "google",
                &new_access,
                refresh_token,
                new_expires_at,
            )?;
        }

        Ok(new_access)
    }

    async fn execute_api_call<F, Fut, T>(&self, call_fn: F) -> Result<T>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response>>,
        T: serde::de::DeserializeOwned,
    {
        let mut token = self.get_valid_token().await?;
        let mut response = call_fn(token.clone()).await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            eprintln!("[GoogleCalendarProvider] Received 401 Unauthorized, attempting forced token refresh.");
            let creds = {
                let conn = self.db.lock().unwrap();
                crate::business::db::load_calendar_credentials(&conn, &self.business_id, "google")?
            };
            if let Some((_, refresh_token, _)) = creds {
                token = self.refresh_token_flow(&refresh_token).await?;
                response = call_fn(token).await?;
            }
        }

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Google Calendar API request failed (status {}): {}",
                status,
                err_text
            ));
        }

        let text = response.text().await?;
        if text.trim().is_empty() {
            serde_json::from_str::<T>("null")
                .map_err(|e| anyhow!("Failed to parse empty response: {}", e))
        } else {
            serde_json::from_str::<T>(&text).map_err(|e| {
                anyhow!(
                    "Failed to parse Google API response: {}. Body was: {}",
                    e,
                    text
                )
            })
        }
    }

    async fn check_availability_async(&self, start_time: i64, end_time: i64) -> Result<bool> {
        let url = format!("{}/calendar/v3/freeBusy", self.base_url);

        let time_min = chrono::DateTime::from_timestamp(start_time, 0)
            .ok_or_else(|| anyhow!("Invalid start_time"))?
            .to_rfc3339();
        let time_max = chrono::DateTime::from_timestamp(end_time, 0)
            .ok_or_else(|| anyhow!("Invalid end_time"))?
            .to_rfc3339();

        let body = json!({
            "timeMin": time_min,
            "timeMax": time_max,
            "items": [{"id": "primary"}]
        });

        let client = &self.client;
        let response_val: serde_json::Value = self
            .execute_api_call(|token| {
                let body = body.clone();
                let url = url.clone();
                async move {
                    client
                        .post(&url)
                        .bearer_auth(token)
                        .json(&body)
                        .send()
                        .await
                        .map_err(Into::into)
                }
            })
            .await?;

        let busy_slots = response_val
            .get("calendars")
            .and_then(|c| c.get("primary"))
            .and_then(|p| p.get("busy"))
            .and_then(|b| b.as_array());

        if let Some(slots) = busy_slots {
            Ok(slots.is_empty())
        } else {
            Ok(true)
        }
    }

    async fn create_event_async(
        &self,
        title: String,
        start_time: i64,
        end_time: i64,
    ) -> Result<CalendarEvent> {
        let url = format!("{}/calendar/v3/calendars/primary/events", self.base_url);

        let time_min = chrono::DateTime::from_timestamp(start_time, 0)
            .ok_or_else(|| anyhow!("Invalid start_time"))?
            .to_rfc3339();
        let time_max = chrono::DateTime::from_timestamp(end_time, 0)
            .ok_or_else(|| anyhow!("Invalid end_time"))?
            .to_rfc3339();

        let body = json!({
            "summary": title,
            "start": {
                "dateTime": time_min
            },
            "end": {
                "dateTime": time_max
            }
        });

        let client = &self.client;
        let response_val: serde_json::Value = self
            .execute_api_call(|token| {
                let body = body.clone();
                let url = url.clone();
                async move {
                    client
                        .post(&url)
                        .bearer_auth(token)
                        .json(&body)
                        .send()
                        .await
                        .map_err(Into::into)
                }
            })
            .await?;

        let id = response_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Google Calendar response missing event id"))?
            .to_string();

        Ok(CalendarEvent {
            id,
            title,
            start_time,
            end_time,
        })
    }

    async fn update_event_async(
        &self,
        event_id: String,
        title: String,
        start_time: i64,
        end_time: i64,
    ) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendar/v3/calendars/primary/events/{}",
            self.base_url, event_id
        );

        let time_min = chrono::DateTime::from_timestamp(start_time, 0)
            .ok_or_else(|| anyhow!("Invalid start_time"))?
            .to_rfc3339();
        let time_max = chrono::DateTime::from_timestamp(end_time, 0)
            .ok_or_else(|| anyhow!("Invalid end_time"))?
            .to_rfc3339();

        let body = json!({
            "summary": title,
            "start": {
                "dateTime": time_min
            },
            "end": {
                "dateTime": time_max
            }
        });

        let client = &self.client;
        let response_val: serde_json::Value = self
            .execute_api_call(|token| {
                let body = body.clone();
                let url = url.clone();
                async move {
                    client
                        .put(&url)
                        .bearer_auth(token)
                        .json(&body)
                        .send()
                        .await
                        .map_err(Into::into)
                }
            })
            .await?;

        let id = response_val
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Google Calendar response missing event id"))?
            .to_string();

        Ok(CalendarEvent {
            id,
            title,
            start_time,
            end_time,
        })
    }

    async fn cancel_event_async(&self, event_id: String) -> Result<()> {
        let url = format!(
            "{}/calendar/v3/calendars/primary/events/{}",
            self.base_url, event_id
        );

        let client = &self.client;
        let _: serde_json::Value = self
            .execute_api_call(|token| {
                let url = url.clone();
                async move {
                    client
                        .delete(&url)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(Into::into)
                }
            })
            .await?;

        Ok(())
    }
}

impl CalendarProvider for GoogleCalendarProvider {
    fn check_availability(&self, start_time: i64, end_time: i64) -> Result<bool> {
        self.run_async(async move { self.check_availability_async(start_time, end_time).await })
    }

    fn create_event(&self, title: &str, start_time: i64, end_time: i64) -> Result<CalendarEvent> {
        self.run_async(async move {
            self.create_event_async(title.to_string(), start_time, end_time)
                .await
        })
    }

    fn update_event(
        &self,
        event_id: &str,
        title: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<CalendarEvent> {
        self.run_async(async move {
            self.update_event_async(
                event_id.to_string(),
                title.to_string(),
                start_time,
                end_time,
            )
            .await
        })
    }

    fn cancel_event(&self, event_id: &str) -> Result<()> {
        self.run_async(async move { self.cancel_event_async(event_id.to_string()).await })
    }
}
