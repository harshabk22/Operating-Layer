pub mod whatsapp;
pub mod whatsapp_baileys;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use moat_core::api::server::ApiState;
use moat_core::business::runtime::CustomerInteractionRuntime;
use moat_core::package::DynamicPackage;
use moat_core::MoatResult;
use std::sync::Arc;
use std::sync::OnceLock;

pub static BAILEYS_CONNECTOR: OnceLock<Arc<self::whatsapp_baileys::connector::WhatsAppBaileysConnector>> = OnceLock::new();
pub static BUSINESS_CONNECTOR: OnceLock<Arc<self::whatsapp::provider::WhatsAppConnector>> = OnceLock::new();

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct WhatsAppSettings {
    pub phone_number_id: String,
    pub port: u16,
    pub custom_ports: String,
    pub retry_settings: String,
}

pub async fn load_settings(
    db: &Arc<tokio::sync::Mutex<rusqlite::Connection>>,
    business_id: &str,
) -> WhatsAppSettings {
    let conn = db.lock().await;
    let config_str: Option<String> = conn
        .query_row(
            "SELECT configuration FROM connection_instances WHERE business_id = ?1 AND provider_id = 'whatsapp'",
            [business_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(s) = config_str {
        let decrypted = if let Ok(key) = std::env::var("MOAT_SECRET_KEY") {
            moat_core::security::encryption::decrypt_credential(&s, &key).unwrap_or(s)
        } else {
            let key = "MoatAgeexSuperSecretKey123".to_string();
            moat_core::security::encryption::decrypt_credential(&s, &key).unwrap_or(s)
        };

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&decrypted) {
            let phone_number_id = val.get("phone_number_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let port = val.get("port")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(3003);
            let custom_ports = val.get("custom_ports")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let retry_settings = val.get("retry_settings")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();

            return WhatsAppSettings {
                phone_number_id,
                port,
                custom_ports,
                retry_settings,
            };
        }
    }

    let phone_number_id = std::env::var("WHATSAPP_PHONE_NUMBER_ID")
        .unwrap_or_else(|_| "default".to_string());
    let port = std::env::var("WHATSAPP_BAILEYS_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(3003);

    WhatsAppSettings {
        phone_number_id,
        port,
        custom_ports: "".to_string(),
        retry_settings: "default".to_string(),
    }
}

pub struct WhatsAppPackage {
    baileys_connector: std::sync::Mutex<Option<Arc<self::whatsapp_baileys::connector::WhatsAppBaileysConnector>>>,
}

impl WhatsAppPackage {
    pub fn new() -> Self {
        Self {
            baileys_connector: std::sync::Mutex::new(None),
        }
    }
}

impl Default for WhatsAppPackage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DynamicPackage for WhatsAppPackage {
    fn id(&self) -> &str {
        "whatsapp-v1"
    }

    async fn initialize(&self, runtime: Arc<CustomerInteractionRuntime>) -> MoatResult<()> {
        let conn_arc = runtime.db.clone();

        let settings = load_settings(&conn_arc, "default_business").await;

        let whatsapp_business = Arc::new(self::whatsapp::provider::WhatsAppConnector::new(
            conn_arc.clone(),
            settings.phone_number_id,
        ));

        let whatsapp_baileys = Arc::new(
            self::whatsapp_baileys::connector::WhatsAppBaileysConnector::new(
                conn_arc.clone(),
                "default_business".to_string(),
                settings.port,
            ),
        );

        let _ = BAILEYS_CONNECTOR.set(whatsapp_baileys.clone());
        let _ = BUSINESS_CONNECTOR.set(whatsapp_business.clone());

        // Store reference for shutdown
        {
            let mut lock = self.baileys_connector.lock().unwrap();
            *lock = Some(whatsapp_baileys.clone());
        }

        runtime.registry
            .register("whatsapp_business", whatsapp_business.clone())
            .await;
        runtime.registry
            .register("whatsapp_baileys", whatsapp_baileys.clone())
            .await;

        let multiplexer = Arc::new(
            self::whatsapp_baileys::connector::WhatsAppMultiplexer::new(
                whatsapp_business,
                whatsapp_baileys.clone(),
                conn_arc.clone(),
            ),
        );
        runtime.registry.register("whatsapp", multiplexer).await;

        whatsapp_baileys.set_runtime(Arc::downgrade(&runtime));

        // Start WhatsApp Baileys asynchronously
        let whatsapp_baileys_clone = whatsapp_baileys.clone();
        tokio::spawn(async move {
            if let Err(e) = whatsapp_baileys_clone.start().await {
                eprintln!(
                    "[Bootstrap] Failed to start WhatsApp Baileys connector: {:?}",
                    e
                );
            }
        });

        // Start webhook server
        let webhook_port = std::env::var("WHATSAPP_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(8081);
        let verify_token = std::env::var("WHATSAPP_VERIFY_TOKEN")
            .unwrap_or_else(|_| "default_verify_token".to_string());
        let client_secret = std::env::var("WHATSAPP_CLIENT_SECRET")
            .unwrap_or_else(|_| "default_client_secret".to_string());

        let server = Arc::new(self::whatsapp::webhook::WhatsAppWebhookServer::new(
            runtime.clone(),
            runtime.db.clone(),
            verify_token,
            client_secret,
        ));

        let addr = format!("0.0.0.0:{}", webhook_port);
        tokio::spawn(async move {
            let server_clone = server.clone();
            server_clone.start(addr);
        });

        Ok(())
    }

    fn register_routes(&self, router: Router<ApiState>) -> Router<ApiState> {
        router
            .route("/connectors/whatsapp/qr", get(whatsapp_qr_handler))
            .route("/connectors/whatsapp/status", get(whatsapp_status_handler))
            .route("/connectors/whatsapp/disconnect", post(whatsapp_disconnect_handler))
            .route("/connectors/whatsapp/connect", post(whatsapp_connect_handler))
            .route("/connectors/whatsapp/settings", post(whatsapp_settings_handler))
    }

    async fn shutdown(&self) -> MoatResult<()> {
        let connector = {
            let mut lock = self.baileys_connector.lock().unwrap();
            lock.take()
        };
        if let Some(conn) = connector {
            conn.shutdown().await?;
        }
        Ok(())
    }
}

pub async fn whatsapp_qr_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    if let Some(connector) = state.runtime.registry.lookup("whatsapp_baileys").await {
        if let Ok(Some(qr_str)) = connector.get_qr_code().await {
            if let Some(pos) = qr_str.find("base64,") {
                let b64_part = &qr_str[pos + 7..];
                if let Ok(bytes) = STANDARD.decode(b64_part) {
                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(
                        axum::http::header::CONTENT_TYPE,
                        axum::http::header::HeaderValue::from_static("image/png"),
                    );
                    return Ok((headers, bytes));
                }
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}

pub async fn whatsapp_status_handler(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    let settings = load_settings(&state.runtime.db, "default_business").await;

    let mut response_data = serde_json::json!({
        "phone_number_id": settings.phone_number_id,
        "port": settings.port,
        "custom_ports": settings.custom_ports,
        "retry_settings": settings.retry_settings,
    });

    if let Some(connector) = state.runtime.registry.lookup("whatsapp_baileys").await {
        let healthy = connector.health_check().await.unwrap_or(false);
        if healthy {
            response_data["status"] = serde_json::json!("connected");
            response_data["message"] = serde_json::json!("Connected to Baileys bridge and WhatsApp network.");
            return Ok(Json(response_data));
        }

        if let Ok(Some(_)) = connector.get_qr_code().await {
            response_data["status"] = serde_json::json!("pairing");
            response_data["message"] = serde_json::json!("Generating QR code. Please scan to connect.");
            return Ok(Json(response_data));
        }

        let has_creds = {
            let conn = state.runtime.db.lock().await;
            moat_core::communication::db::has_baileys_credentials(&conn, "default_business")
                .unwrap_or(false)
        };
        if has_creds {
            response_data["status"] = serde_json::json!("reconnecting");
            response_data["message"] = serde_json::json!("Reconnecting to WhatsApp using saved credentials...");
            return Ok(Json(response_data));
        }
    }
    response_data["status"] = serde_json::json!("unpaired");
    response_data["message"] = serde_json::json!("Unpaired. Click Connect / Retry to generate a QR code.");
    Ok(Json(response_data))
}

pub async fn whatsapp_disconnect_handler(
    State(_state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    if let Some(connector) = BAILEYS_CONNECTOR.get() {
        if let Err(e) = connector.disconnect().await {
            eprintln!("[WhatsApp API] Disconnect failed: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return Ok(Json(serde_json::json!({ "status": "unpaired", "message": "WhatsApp session cleared." })));
    }
    Err(StatusCode::NOT_FOUND)
}

pub async fn whatsapp_connect_handler(
    State(_state): State<ApiState>,
) -> Result<impl IntoResponse, StatusCode> {
    if let Some(connector) = BAILEYS_CONNECTOR.get() {
        if let Err(e) = connector.connect().await {
            eprintln!("[WhatsApp API] Connect failed: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return Ok(Json(serde_json::json!({ "status": "pairing", "message": "Spawning WhatsApp bridge..." })));
    }
    Err(StatusCode::NOT_FOUND)
}

#[derive(serde::Deserialize)]
pub struct WhatsAppSettingsRequest {
    pub phone_number_id: String,
    pub port: u16,
    pub custom_ports: String,
    pub retry_settings: String,
}

pub async fn whatsapp_settings_handler(
    State(state): State<ApiState>,
    Json(payload): Json<WhatsAppSettingsRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let business_id = "default_business";
    let config_json = serde_json::json!({
        "phone_number_id": payload.phone_number_id,
        "port": payload.port,
        "custom_ports": payload.custom_ports,
        "retry_settings": payload.retry_settings,
    });

    let config_str = config_json.to_string();
    let key = std::env::var("MOAT_SECRET_KEY").unwrap_or_else(|_| "MoatAgeexSuperSecretKey123".to_string());
    let stored_config = moat_core::security::encryption::encrypt_credential(&config_str, &key);

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    {
        let conn = state.runtime.db.lock().await;
        let _ = conn.execute(
            "INSERT INTO connection_instances (id, business_id, provider_id, configuration, created_at, updated_at) 
             VALUES (?1, ?2, 'whatsapp', ?3, ?4, ?5) 
             ON CONFLICT(business_id, provider_id) DO UPDATE SET 
                 configuration=excluded.configuration,
                 updated_at=excluded.updated_at",
            rusqlite::params![id, business_id, &stored_config, now, now],
        );
    }

    if let Some(connector) = BAILEYS_CONNECTOR.get() {
        let _ = connector.update_settings(payload.port).await;
    }
    if let Some(connector) = BUSINESS_CONNECTOR.get() {
        connector.update_phone_number_id(payload.phone_number_id);
    }

    Ok(Json(serde_json::json!({ "status": "success", "message": "Settings saved." })))
}
