use moat_core::agent::injector::InjectorConfig;
use moat_core::agent::interaction_orchestrator::Orchestrator;
use moat_core::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
use moat_core::agent::worker::WorkerAgent;
use moat_core::business::runtime::CustomerInteractionRuntime;
use moat_core::communication::ConnectorRegistry;
use moat_core::integration::external_api::{ExternalApiConfig, ProviderFactory};
use moat_core::memory::meta_brain::MetaBrain;
use moat_core::planner::{PlannerAgent, PlannerConfig, WorkerConfig};
use moat_core::MoatResult;
// Removed System import
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

struct LocalStubProvider {
    response: String,
}

impl LlmProvider for LocalStubProvider {
    fn chat_complete<'a>(
        &'a self,
        _model: &'a str,
        _messages: Vec<LlmMessage>,
        _tools: Option<Vec<moat_core::agent::interaction_orchestrator::LlmTool>>,
        _max_tokens: usize,
        _temperature: f32,
        _response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        let text = self.response.clone();
        Box::pin(async move {
            Ok(LlmResponse {
                content: Some(text),
                tool_calls: None,
                tokens: 42,
                structured: None,
            })
        })
    }
}

async fn setup_runtime() -> anyhow::Result<Arc<CustomerInteractionRuntime>> {
    let cwd = std::env::current_dir()?;
    let base_dir = if cwd.ends_with("moat_core") {
        cwd
    } else {
        cwd.join("moat_core")
    };
    let db_path = base_dir.join("business.db");
    let seed_path = base_dir.join("moat_core_restored.db");

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if !db_path.exists() && seed_path.exists() {
        println!("[Bootstrap] business.db not found. Copying seed from moat_core_restored.db...");
        std::fs::copy(&seed_path, &db_path)?;
    }

    // Delete legacy sessions.db file
    if let Some(home) = dirs::home_dir() {
        let sessions_db = home.join(".ageex").join("sessions.db");
        if sessions_db.exists() {
            println!("[Bootstrap] Deleting legacy sessions.db file...");
            let _ = std::fs::remove_file(sessions_db);
        }
    }

    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

    // Drop legacy v2 tables if they exist
    let _ = conn.execute("DROP TABLE IF EXISTS v2_events", []);
    let _ = conn.execute("DROP TABLE IF EXISTS v2_snapshots", []);

    moat_core::business::db::prepare_tables(&conn)?;
    moat_core::communication::db::prepare_tables(&conn)?;

    let conn_arc = Arc::new(AsyncMutex::new(conn));
    let registry = Arc::new(ConnectorRegistry::new());

    let whatsapp_business = Arc::new(moat_core::communication::whatsapp::WhatsAppConnector::new(
        Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&db_path)?)),
        std::env::var("WHATSAPP_PHONE_NUMBER_ID").unwrap_or_else(|_| "default".to_string()),
    ));

    let whatsapp_baileys = Arc::new(
        moat_core::communication::whatsapp_baileys::WhatsAppBaileysConnector::new(
            Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&db_path)?)),
            "default_business".to_string(),
        ),
    );

    registry
        .register("whatsapp_business", whatsapp_business.clone())
        .await;
    registry
        .register("whatsapp_baileys", whatsapp_baileys.clone())
        .await;

    let multiplexer = Arc::new(
        moat_core::communication::whatsapp_baileys::WhatsAppMultiplexer::new(
            whatsapp_business,
            whatsapp_baileys.clone(),
            Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&db_path)?)),
        ),
    );
    registry.register("whatsapp", multiplexer).await;

    let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));

    let ext_config = moat_core::integration::external_api::ExternalApiConfig::default();
    if let Ok(prov) = std::env::var("AGEEX_PROVIDER") {
        *ext_config.default_provider.lock().unwrap() = prov;
    }
    let provider_factory =
        moat_core::integration::external_api::ExternalProviderFactory::new(ext_config);
    let provider_name = provider_factory.get_default_provider();

    let (provider, model): (Arc<dyn moat_core::agent::llm_types::LlmProvider>, String) =
        match provider_factory.provider_for(&provider_name) {
            Ok(p) => {
                let default_model = if provider_name == "google" {
                    "gemini-2.5-flash"
                } else {
                    "deepseek-chat"
                };
                let model =
                    std::env::var("AGEEX_MODEL").unwrap_or_else(|_| default_model.to_string());
                (p, model)
            }
            Err(e) => {
                eprintln!("[Bootstrap] Warning: failed to load real LLM provider '{}': {}. Falling back to LocalStubProvider.", provider_name, e);
                let p = Arc::new(LocalStubProvider {
                    response: "<response>done</response>".to_string(),
                });
                (p, "stub".to_string())
            }
        };

    let planner = PlannerAgent::new(
        PlannerConfig {
            model: model.clone(),
            max_subtasks: 5,
            temperature: 0.0,
            max_tokens: 1024,
        },
        provider.clone(),
    );

    let worker = WorkerAgent::new(
        WorkerConfig {
            model: model.clone(),
            command_timeout: std::time::Duration::from_secs(5),
            allow_high_risk: true,
            external_apis: ExternalApiConfig::default(),
        },
        Arc::clone(&meta_brain),
        Arc::clone(&conn_arc),
    )?;

    let orchestrator = Arc::new(Orchestrator::new(
        planner,
        worker,
        Arc::clone(&meta_brain),
        Arc::clone(&conn_arc),
        InjectorConfig {
            top_k_memories: 10,
            max_memory_chars: 1000,
        },
    ));

    let db_conn = Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&db_path)?));
    let cal: Arc<dyn moat_core::business::calendar::CalendarProvider> =
        Arc::new(moat_core::business::calendar::GoogleCalendarProvider::new(
            db_conn.clone(),
            "default_business".to_string(),
        ));

    let sheet: Arc<dyn moat_core::business::spreadsheet::SpreadsheetProvider> =
        Arc::new(moat_core::business::spreadsheet::GoogleSheetsProvider::new(
            db_conn.clone(),
            "default_business".to_string(),
        ));

    let notifier = Arc::new(moat_core::business::owner::ConsoleNotificationProvider::new());

    let runtime = Arc::new(CustomerInteractionRuntime::with_providers(
        orchestrator,
        conn_arc,
        registry,
        cal,
        sheet,
        notifier,
    ));

    whatsapp_baileys.set_runtime(Arc::downgrade(&runtime));
    if let Err(e) = whatsapp_baileys.start().await {
        eprintln!(
            "[Bootstrap] Failed to start WhatsApp Baileys connector: {:?}",
            e
        );
    }

    // Bootload installed packages from business.db
    {
        println!("[Bootstrap] Bootloading installed packages from business.db...");
        let conn = rusqlite::Connection::open(&db_path)?;
        let mut stmt = conn.prepare("SELECT id FROM projections WHERE name = 'PackageInstalled'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for pkg_id in rows.flatten() {
            println!("[Bootstrap] Loading package: {}", pkg_id);
            if let Some(manifest) = moat_core::api::routes::read_package_manifest(&pkg_id) {
                moat_core::api::routes::register_package_core(&manifest, &meta_brain).await;
                println!("[Bootstrap] Package {} hydrated successfully.", pkg_id);
            } else {
                eprintln!("[Bootstrap] Warning: manifest not found for package {}", pkg_id);
            }
        }
    }

    Ok(runtime)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    println!("╔══════════════════════════════════════════╗");
    println!("║    Moat Core Runtime Daemon              ║");
    println!("╚══════════════════════════════════════════╝");
    println!("Mode: Production Runtime");
    println!();

    // Start the Operator Dashboard & Control Surface API server
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8080);

    let runtime = setup_runtime().await?;
    let (_server_handle, bound_port) =
        moat_core::api::server::start_api_server(runtime.clone(), port).await?;
    println!(
        "[Dashboard API] Running in the background on port {}",
        bound_port
    );

    // Start background workers and WhatsApp Webhook Server
    let webhook_port = std::env::var("WHATSAPP_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8081);
    let _webhook_handle = runtime
        .clone()
        .start_webhook(&format!("0.0.0.0:{}", webhook_port));
    println!(
        "[WhatsApp API] Webhook server running in the background on port {}",
        webhook_port
    );

    println!("── System ready ──────────────────────────────");
    println!("Running background services. Press Ctrl+C to exit.");

    // Suspend indefinitely
    tokio::signal::ctrl_c().await?;
    println!("Shutting down.");

    Ok(())
}
