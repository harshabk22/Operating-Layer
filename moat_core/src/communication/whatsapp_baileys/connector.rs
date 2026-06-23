const BRIDGE_JS: &str = include_str!("bridge/bridge.js");
const PACKAGE_JSON: &str = include_str!("bridge/package.json");

use crate::business::runtime::CustomerInteractionRuntime;
use crate::communication::connector::ChannelConnector;
use crate::communication::types::{ChannelType, InteractionMessage};
use crate::MoatError;
use crate::MoatResult;
use async_trait::async_trait;
use reqwest::Client;
use rusqlite::Connection;
use std::fs;
use std::process::Child;
use std::sync::{Arc, Mutex, RwLock, Weak};

#[derive(Clone)]
pub struct WhatsAppBaileysConnector {
    db: Arc<Mutex<Connection>>,
    business_id: String,
    port: u16,
    child: Arc<Mutex<Option<Child>>>,
    runtime: Arc<RwLock<Option<Weak<CustomerInteractionRuntime>>>>,
    latest_qr: Arc<Mutex<Option<String>>>,
    latest_status: Arc<Mutex<String>>,
    client: Client,
}

impl WhatsAppBaileysConnector {
    pub fn new(db: Arc<Mutex<Connection>>, business_id: String) -> Self {
        let port = std::env::var("WHATSAPP_BAILEYS_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(3003);

        Self {
            db,
            business_id,
            port,
            child: Arc::new(Mutex::new(None)),
            runtime: Arc::new(RwLock::new(None)),
            latest_qr: Arc::new(Mutex::new(None)),
            latest_status: Arc::new(Mutex::new("unpaired".to_string())),
            client: Client::new(),
        }
    }

    pub fn set_runtime(&self, runtime: Weak<CustomerInteractionRuntime>) {
        let mut rt = self.runtime.write().unwrap();
        *rt = Some(runtime);
    }

    pub async fn start(self: &Arc<Self>) -> MoatResult<()> {
        self.spawn_subprocess()?;

        // Wait for health check to succeed
        let mut ready = false;
        for _ in 0..50 {
            if let Ok(resp) = self
                .client
                .get(format!("http://127.0.0.1:{}/health", self.port))
                .send()
                .await
            {
                if resp.status().is_success() {
                    ready = true;
                    break;
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        if !ready {
            return Err(MoatError::Orchestrator(
                "WhatsApp bridge failed to start".to_string(),
            ));
        }

        // Spawn watcher loops
        let self_clone1 = self.clone();
        tokio::spawn(async move {
            self_clone1.directory_sync_loop().await;
        });

        let self_clone2 = self.clone();
        tokio::spawn(async move {
            self_clone2.message_polling_loop().await;
        });

        let self_clone3 = self.clone();
        tokio::spawn(async move {
            self_clone3.health_watcher_loop().await;
        });

        Ok(())
    }

    fn spawn_subprocess(&self) -> MoatResult<()> {
        let home =
            dirs::home_dir().ok_or_else(|| MoatError::Orchestrator("No home dir".to_string()))?;
        let bridge_dir = home.join(".ageex").join("whatsapp_baileys");
        fs::create_dir_all(&bridge_dir).map_err(|e| MoatError::Orchestrator(e.to_string()))?;

        let package_json_path = bridge_dir.join("package.json");
        let bridge_js_path = bridge_dir.join("bridge.js");

        fs::write(&package_json_path, PACKAGE_JSON)
            .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
        fs::write(&bridge_js_path, BRIDGE_JS)
            .map_err(|e| MoatError::Orchestrator(e.to_string()))?;

        // Install dependencies if not present
        let node_modules_path = bridge_dir.join("node_modules");
        if !node_modules_path.exists() {
            println!("[Baileys] Installing npm dependencies...");
            let npm_bin = if cfg!(target_os = "windows") {
                "npm.cmd"
            } else {
                "npm"
            };
            let status = std::process::Command::new(npm_bin)
                .arg("install")
                .current_dir(&bridge_dir)
                .status()
                .map_err(|e| {
                    MoatError::Orchestrator(format!("Failed to run npm install: {}", e))
                })?;
            if !status.success() {
                return Err(MoatError::Orchestrator("npm install failed".to_string()));
            }
        }

        let session_dir = bridge_dir.join("session");
        fs::create_dir_all(&session_dir).map_err(|e| MoatError::Orchestrator(e.to_string()))?;

        // Populate directory from SQLite
        let sync = crate::communication::whatsapp_baileys::store::DirectorySync::new(
            self.db.clone(),
            self.business_id.clone(),
            "whatsapp_baileys".to_string(),
            session_dir.to_str().unwrap().to_string(),
        );
        sync.populate_directory()?;

        let node_bin = "node";
        let port_str = self.port.to_string();
        let session_str = session_dir.to_str().unwrap().to_string();

        let child = std::process::Command::new(node_bin)
            .arg("bridge.js")
            .arg("--port")
            .arg(&port_str)
            .arg("--session")
            .arg(&session_str)
            .current_dir(&bridge_dir)
            .spawn()
            .map_err(|e| MoatError::Orchestrator(format!("Failed to spawn Node process: {}", e)))?;

        let mut child_lock = self.child.lock().unwrap();
        *child_lock = Some(child);

        Ok(())
    }

    async fn directory_sync_loop(&self) {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let session_dir = home.join(".ageex").join("whatsapp_baileys").join("session");
        let sync = crate::communication::whatsapp_baileys::store::DirectorySync::new(
            self.db.clone(),
            self.business_id.clone(),
            "whatsapp_baileys".to_string(),
            session_dir.to_str().unwrap().to_string(),
        );

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            if let Err(e) = sync.sync_to_db() {
                eprintln!("[Baileys Sync] Error syncing to DB: {:?}", e);
            }
        }
    }

    async fn message_polling_loop(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

            let runtime_opt = {
                let rt_lock = self.runtime.read().unwrap();
                rt_lock.as_ref().and_then(|weak| weak.upgrade())
            };

            let runtime = match runtime_opt {
                Some(rt) => rt,
                None => continue,
            };

            let url = format!("http://127.0.0.1:{}/messages", self.port);
            if let Ok(resp) = self.client.get(&url).send().await {
                match resp.json::<Vec<crate::communication::whatsapp_baileys::events::BaileysInboundMessage>>().await {
                    Ok(messages) => {
                        for msg in messages {
                            let idempotency_key = format!("whatsapp_baileys:{}", msg.message_id);

                            let is_new = {
                                let conn = runtime.db.lock().await;
                                match crate::business::reliability::idempotency::try_acquire(&conn, &idempotency_key) {
                                    Ok(crate::business::reliability::idempotency::IdempotencyStatus::Acquired) => true,
                                    _ => false,
                                }
                            };

                            if !is_new {
                                continue;
                            }

                            let external_id = normalize_jid_to_external_id(&msg.sender_id);

                            {
                                let conn = runtime.db.lock().await;
                                let existing = crate::business::db::get_contact_by_identity(&conn, "whatsapp", &external_id).unwrap_or(None);
                                if existing.is_none() {
                                    let contact_id = uuid::Uuid::new_v4().to_string();
                                    let contact = crate::business::models::ExternalContact {
                                        id: contact_id.clone(),
                                        name: Some(msg.sender_name.clone()),
                                        email: None,
                                        phone: Some(external_id.replace("whatsapp:", "")),
                                        created_at: chrono::Utc::now().timestamp(),
                                        updated_at: chrono::Utc::now().timestamp(),
                                    };
                                    let _ = crate::business::db::insert_contact(&conn, &contact);

                                    let mapping = crate::business::models::ContactIdentityMapping {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        contact_id,
                                        channel: "whatsapp".to_string(),
                                        external_id: external_id.clone(),
                                    };
                                    let _ = crate::business::db::insert_identity_mapping(&conn, &mapping);
                                }

                                let _ = crate::business::db::increment_kpi_metric(&conn, "messages_received");
                                let _ = crate::business::db::increment_kpi_metric(&conn, "whatsapp_baileys_messages_received");
                            }

                            let reply_res = runtime.handle_interaction("whatsapp", &external_id, &msg.body).await;

                            {
                                let conn = runtime.db.lock().await;
                                match reply_res {
                                    Ok(reply) => {
                                        let _ = crate::business::reliability::idempotency::complete(&conn, &idempotency_key, &reply);
                                    }
                                    Err(e) => {
                                        eprintln!("[Baileys Connector] Error handling interaction: {:?}", e);
                                        let _ = crate::business::reliability::idempotency::fail(&conn, &idempotency_key);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[Baileys] Failed to parse inbound messages JSON: {:?}", e);
                    }
                }
            }
        }
    }

    async fn health_watcher_loop(&self) {
        let mut prev_state = "unpaired".to_string();

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            let url = format!("http://127.0.0.1:{}/health", self.port);
            let state_opt = if let Ok(resp) = self.client.get(&url).send().await {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    json.get("status")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            let current_state = state_opt.unwrap_or_else(|| "disconnected".to_string());

            if current_state != prev_state {
                println!(
                    "[Baileys Status] Status changed: {} -> {}",
                    prev_state, current_state
                );

                let conn = self.db.lock().unwrap();
                if current_state == "connected" {
                    if prev_state == "reconnecting" || prev_state == "disconnected" {
                        let _ = crate::business::db::increment_kpi_metric(
                            &conn,
                            "whatsapp_baileys_reconnect_count",
                        );
                    }
                } else if current_state == "disconnected" || current_state == "reconnecting" {
                    if prev_state == "connected" {
                        let _ = crate::business::db::increment_kpi_metric(
                            &conn,
                            "whatsapp_baileys_disconnect_count",
                        );
                    }
                } else if current_state == "pairing" && prev_state == "unpaired" {
                    let _ = crate::business::db::increment_kpi_metric(
                        &conn,
                        "whatsapp_baileys_pairing_count",
                    );
                }

                *self.latest_status.lock().unwrap() = current_state.clone();
                prev_state = current_state;
            }

            if prev_state == "pairing" {
                let qr_url = format!("http://127.0.0.1:{}/qr", self.port);
                if let Ok(resp) = self.client.get(&qr_url).send().await {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        let qr = json
                            .get("qr")
                            .and_then(|q| q.as_str())
                            .map(|s| s.to_string());
                        *self.latest_qr.lock().unwrap() = qr;
                    }
                }
            } else {
                *self.latest_qr.lock().unwrap() = None;
            }
        }
    }
}

impl Drop for WhatsAppBaileysConnector {
    fn drop(&mut self) {
        let mut lock = self.child.lock().unwrap();
        if let Some(mut child) = lock.take() {
            println!(
                "[Baileys] Terminating Node bridge subprocess (PID: {})...",
                child.id()
            );
            let _ = child.kill();
        }
    }
}

pub(crate) fn normalize_jid_to_external_id(jid: &str) -> String {
    let clean = jid.split('@').next().unwrap_or(jid);
    let number = clean.split(':').next().unwrap_or(clean);
    if jid.contains("@lid") || (number.len() == 15 && number.starts_with('1')) {
        format!("whatsapp:{}@lid", number)
    } else {
        format!("whatsapp:+{}", number)
    }
}

pub(crate) fn external_id_to_jid(external_id: &str) -> String {
    let clean_id = external_id.replace("whatsapp:", "");
    if clean_id.contains('@') {
        return clean_id;
    }
    let number = clean_id.replace("+", "").replace(" ", "");
    if number.len() == 15 && number.starts_with('1') {
        format!("{}@lid", number)
    } else {
        format!("{}@s.whatsapp.net", number)
    }
}

#[async_trait]
impl ChannelConnector for WhatsAppBaileysConnector {
    async fn receive_message(&self, _message: InteractionMessage) -> MoatResult<()> {
        Ok(())
    }

    async fn send_message(&self, message: InteractionMessage) -> MoatResult<()> {
        if *self.latest_status.lock().unwrap() != "connected" {
            return Err(MoatError::Orchestrator(
                "Cannot send message: Baileys is not connected".to_string(),
            ));
        }

        let mut target_customer_id = message.customer_id.clone();
        if !target_customer_id.starts_with("whatsapp:") {
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
                    .query([&target_customer_id])
                    .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
                if let Some(row) = rows
                    .next()
                    .map_err(|e| MoatError::Orchestrator(e.to_string()))?
                {
                    target_customer_id = row
                        .get::<_, String>(0)
                        .map_err(|e| MoatError::Orchestrator(e.to_string()))?;
                }
            }
        }

        let jid = external_id_to_jid(&target_customer_id);
        let url = format!("http://127.0.0.1:{}/send", self.port);
        let payload = serde_json::json!({
            "chatId": jid,
            "message": message.body,
        });

        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                let conn = self.db.lock().unwrap();
                let _ =
                    crate::business::db::increment_kpi_metric(&conn, "whatsapp_baileys_failures");
                MoatError::Orchestrator(format!("Failed to send Baileys message: {}", e))
            })?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "whatsapp_baileys_failures");
            return Err(MoatError::Orchestrator(format!(
                "Baileys bridge returned error: {}",
                err_text
            )));
        }

        {
            let conn = self.db.lock().unwrap();
            let _ = crate::business::db::increment_kpi_metric(&conn, "messages_sent");
            let _ =
                crate::business::db::increment_kpi_metric(&conn, "whatsapp_baileys_messages_sent");
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
        Ok(*self.latest_status.lock().unwrap() == "connected")
    }

    async fn get_qr_code(&self) -> MoatResult<Option<String>> {
        Ok(self.latest_qr.lock().unwrap().clone())
    }
}

// WhatsApp Multiplexer
pub struct WhatsAppMultiplexer {
    business: Arc<crate::communication::whatsapp::WhatsAppConnector>,
    baileys: Arc<WhatsAppBaileysConnector>,
    db: Arc<Mutex<Connection>>,
}

impl WhatsAppMultiplexer {
    pub fn new(
        business: Arc<crate::communication::whatsapp::WhatsAppConnector>,
        baileys: Arc<WhatsAppBaileysConnector>,
        db: Arc<Mutex<Connection>>,
    ) -> Self {
        Self {
            business,
            baileys,
            db,
        }
    }

    async fn is_baileys_active(&self) -> bool {
        let conn = self.db.lock().unwrap();
        crate::communication::db::has_baileys_credentials(&conn, "default_business")
            .unwrap_or(false)
    }
}

#[async_trait]
impl ChannelConnector for WhatsAppMultiplexer {
    async fn receive_message(&self, message: InteractionMessage) -> MoatResult<()> {
        if self.is_baileys_active().await {
            self.baileys.receive_message(message).await
        } else {
            self.business.receive_message(message).await
        }
    }

    async fn send_message(&self, message: InteractionMessage) -> MoatResult<()> {
        if self.is_baileys_active().await {
            self.baileys.send_message(message).await
        } else {
            self.business.send_message(message).await
        }
    }

    async fn open_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        customer_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()> {
        if self.is_baileys_active().await {
            self.baileys
                .open_thread(business_id, thread_id, customer_id, channel)
                .await
        } else {
            self.business
                .open_thread(business_id, thread_id, customer_id, channel)
                .await
        }
    }

    async fn close_thread(
        &self,
        business_id: &str,
        thread_id: &str,
        channel: ChannelType,
    ) -> MoatResult<()> {
        if self.is_baileys_active().await {
            self.baileys
                .close_thread(business_id, thread_id, channel)
                .await
        } else {
            self.business
                .close_thread(business_id, thread_id, channel)
                .await
        }
    }

    async fn health_check(&self) -> MoatResult<bool> {
        if self.is_baileys_active().await {
            self.baileys.health_check().await
        } else {
            self.business.health_check().await
        }
    }

    async fn get_qr_code(&self) -> MoatResult<Option<String>> {
        self.baileys.get_qr_code().await
    }
}
