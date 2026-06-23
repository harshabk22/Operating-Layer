use super::models::{WhatsAppOutboundRequest, WhatsAppOutboundText};
use crate::communication::connector::ChannelConnector;
use crate::communication::types::{ChannelType, InteractionMessage};
use crate::MoatError;
use crate::MoatResult;
use async_trait::async_trait;
use reqwest::Client;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

pub struct WhatsAppConnector {
    client: Client,
    db: Arc<Mutex<Connection>>,
    base_url: String,
    phone_number_id: String,
    access_token_override: Option<String>,
}

impl WhatsAppConnector {
    pub fn new(db: Arc<Mutex<Connection>>, phone_number_id: String) -> Self {
        Self {
            client: Client::new(),
            db,
            base_url: "https://graph.facebook.com".to_string(),
            phone_number_id,
            access_token_override: None,
        }
    }

    pub fn with_url(
        db: Arc<Mutex<Connection>>,
        phone_number_id: String,
        base_url: String,
        access_token: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            db,
            base_url,
            phone_number_id,
            access_token_override: access_token,
        }
    }

    async fn get_access_token(&self) -> String {
        if let Some(token) = &self.access_token_override {
            return token.clone();
        }
        // Load from DB
        let creds = {
            let conn = self.db.lock().unwrap();
            crate::business::db::load_calendar_credentials(&conn, "default_business", "whatsapp")
        };
        match creds {
            Ok(Some((access_token, _, _))) => access_token,
            _ => std::env::var("WHATSAPP_ACCESS_TOKEN").unwrap_or_default(),
        }
    }
}

#[async_trait]
impl ChannelConnector for WhatsAppConnector {
    async fn receive_message(&self, _message: InteractionMessage) -> MoatResult<()> {
        Ok(())
    }

    async fn send_message(&self, message: InteractionMessage) -> MoatResult<()> {
        let mut to_number = message.customer_id.clone();
        if !to_number.starts_with("whatsapp:") {
            let db = self.db.lock().unwrap();
            let table_exists: bool = db
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='business_identity_mappings'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0)
                == 1;

            if table_exists {
                let mut stmt = db
                    .prepare("SELECT external_id FROM business_identity_mappings WHERE contact_id = ?1 AND channel = 'whatsapp'")
                    .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
                let mut rows = stmt
                    .query([&to_number])
                    .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
                if let Some(row) = rows
                    .next()
                    .map_err(|e| MoatError::Orchestrator(e.to_string()))?
                {
                    to_number = row
                        .get::<_, String>(0)
                        .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
                }
            }
        }

        if to_number.starts_with("whatsapp:") {
            to_number = to_number.replace("whatsapp:", "");
        }
        if to_number.starts_with('+') {
            to_number = to_number[1..].to_string();
        }

        let payload = WhatsAppOutboundRequest {
            messaging_product: "whatsapp".to_string(),
            recipient_type: "individual".to_string(),
            to: to_number,
            message_type: "text".to_string(),
            text: WhatsAppOutboundText {
                preview_url: false,
                body: message.body.clone(),
            },
        };

        let token = self.get_access_token().await;
        let url = format!("{}/v18.0/{}/messages", self.base_url, self.phone_number_id);

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                let conn = self.db.lock().unwrap();
                let _ = crate::business::db::increment_kpi_metric(&conn, "whatsapp_failures");
                MoatError::Orchestrator(format!("Failed to send WhatsApp message: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let err_text = response.text().await.unwrap_or_default();
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "whatsapp_failures");
            return Err(MoatError::Orchestrator(format!(
                "WhatsApp API returned error status {}: {}",
                status, err_text
            )));
        }

        {
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "messages_sent");
        }

        Ok(())
    }

    async fn open_thread(
        &self,
        _business_id: &str,
        _thread_id: &str,
        _customer_id: &str,
        _channel: ChannelType,
    ) -> MoatResult<()> {
        Ok(())
    }

    async fn close_thread(
        &self,
        _business_id: &str,
        _thread_id: &str,
        _channel: ChannelType,
    ) -> MoatResult<()> {
        Ok(())
    }

    async fn health_check(&self) -> MoatResult<bool> {
        Ok(true)
    }
}
