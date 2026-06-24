use crate::business::spreadsheet::provider::SpreadsheetProvider;
use anyhow::{anyhow, Result};
use reqwest::Client;
use rusqlite::Connection;
use serde_json::json;
use std::sync::{Arc, Mutex};

pub struct GoogleSheetsProvider {
    client: Client,
    db: Arc<Mutex<Connection>>,
    business_id: String,
    base_url: String,
    oauth_url: String,
}

impl GoogleSheetsProvider {
    fn get_spreadsheet_id(&self) -> Result<String> {
        let conn = self.db.lock().unwrap();
        let config = crate::business::db::read_google_config(&conn, &self.business_id)?;
        config
            .selected_spreadsheet_id
            .ok_or_else(|| anyhow!("Google Spreadsheet not configured"))
    }
    pub fn new(db: Arc<Mutex<Connection>>, business_id: String) -> Self {
        Self {
            client: Client::new(),
            db,
            business_id,
            base_url: "https://sheets.googleapis.com".to_string(),
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
            crate::business::db::load_calendar_credentials(
                &conn,
                &self.business_id,
                "google_sheets",
            )?
        };

        let (access_token, refresh_token, expires_at) = match creds {
            Some(c) => c,
            None => {
                return Err(anyhow!(
                    "No Google Sheets credentials found for business {}",
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
                "Failed to refresh Google Sheets OAuth token (status {}): {}",
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
                "google_sheets",
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
            let creds = {
                let conn = self.db.lock().unwrap();
                crate::business::db::load_calendar_credentials(
                    &conn,
                    &self.business_id,
                    "google_sheets",
                )?
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
                "Google Sheets API request failed (status {}): {}",
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

    async fn append_row_async(&self, sheet_name: &str, values: Vec<String>) -> Result<()> {
        let url = format!(
            "{}/v4/spreadsheets/{}/values/{}!A:Z:append?valueInputOption=USER_ENTERED",
            self.base_url,
            self.get_spreadsheet_id()?,
            sheet_name
        );
        let body = json!({
            "values": [values]
        });
        let client = &self.client;
        let _: serde_json::Value = self
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

        // Increment kpi metrics
        {
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "sheet_updates");
        }

        Ok(())
    }

    async fn update_row_async(
        &self,
        sheet_name: &str,
        row_index: usize,
        values: Vec<String>,
    ) -> Result<()> {
        let row_num = row_index + 1;
        let url = format!(
            "{}/v4/spreadsheets/{}/values/{}!A{}:Z{}?valueInputOption=USER_ENTERED",
            self.base_url,
            self.get_spreadsheet_id()?,
            sheet_name,
            row_num,
            row_num
        );
        let body = json!({
            "values": [values]
        });
        let client = &self.client;
        let _: serde_json::Value = self
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

        // Increment kpi metrics
        {
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "sheet_updates");
        }

        Ok(())
    }

    async fn lookup_row_async(
        &self,
        sheet_name: &str,
        query: &str,
    ) -> Result<Option<(usize, Vec<String>)>> {
        let url = format!(
            "{}/v4/spreadsheets/{}/values/{}!A:Z",
            self.base_url,
            self.get_spreadsheet_id()?,
            sheet_name
        );
        let client = &self.client;
        let response_val: serde_json::Value = self
            .execute_api_call(|token| {
                let url = url.clone();
                async move {
                    client
                        .get(&url)
                        .bearer_auth(token)
                        .send()
                        .await
                        .map_err(Into::into)
                }
            })
            .await?;

        let values = response_val.get("values").and_then(|v| v.as_array());

        if let Some(rows) = values {
            for (index, row) in rows.iter().enumerate() {
                if let Some(cells) = row.as_array() {
                    let mut row_strings = Vec::new();
                    let mut found = false;
                    for cell in cells {
                        let val = cell.as_str().unwrap_or("").to_string();
                        if val.contains(query) {
                            found = true;
                        }
                        row_strings.push(val);
                    }
                    if found {
                        return Ok(Some((index, row_strings)));
                    }
                }
            }
        }

        Ok(None)
    }
}

impl SpreadsheetProvider for GoogleSheetsProvider {
    fn append_row(&self, sheet_name: &str, values: Vec<String>) -> Result<()> {
        let creds_exist = {
            let conn = self.db.lock().unwrap();
            crate::business::db::load_calendar_credentials(&conn, &self.business_id, "google_sheets")
                .map(|c| c.is_some())
                .unwrap_or(false)
        };
        if !creds_exist {
            println!("[GoogleSheetsProvider] No credentials found for business {}. Falling back to sandbox append_row.", self.business_id);
            return Ok(());
        }
        self.run_async(self.append_row_async(sheet_name, values))
    }

    fn update_row(&self, sheet_name: &str, row_index: usize, values: Vec<String>) -> Result<()> {
        let creds_exist = {
            let conn = self.db.lock().unwrap();
            crate::business::db::load_calendar_credentials(&conn, &self.business_id, "google_sheets")
                .map(|c| c.is_some())
                .unwrap_or(false)
        };
        if !creds_exist {
            println!("[GoogleSheetsProvider] No credentials found for business {}. Falling back to sandbox update_row.", self.business_id);
            return Ok(());
        }
        self.run_async(self.update_row_async(sheet_name, row_index, values))
    }

    fn lookup_row(&self, sheet_name: &str, query: &str) -> Result<Option<(usize, Vec<String>)>> {
        let creds_exist = {
            let conn = self.db.lock().unwrap();
            crate::business::db::load_calendar_credentials(&conn, &self.business_id, "google_sheets")
                .map(|c| c.is_some())
                .unwrap_or(false)
        };
        if !creds_exist {
            println!("[GoogleSheetsProvider] No credentials found for business {}. Falling back to sandbox lookup_row.", self.business_id);
            return Ok(None);
        }
        self.run_async(self.lookup_row_async(sheet_name, query))
    }
}
