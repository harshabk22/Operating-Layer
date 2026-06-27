use std::net::SocketAddr;
use moat_admin_backend::{db::DbStore, api::app_router, workers::start_workers, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let _ = dotenvy::dotenv();

    println!("╔══════════════════════════════════════════╗");
    println!("║    Moat Admin Platform Backend           ║");
    println!("╚══════════════════════════════════════════╝");
    println!("Mode: Administration Control Plane");
    println!();

    // Configuration
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(9090); // Independent port

    let db_path = std::env::var("ADMIN_DB_PATH")
        .unwrap_or_else(|_| "admin.db".to_string());

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "admin_secret_key_123".to_string());

    // Initialize database
    let db = DbStore::new(&db_path)?;
    println!("[Database] Initialized SQLite database: {}", db_path);

    // Start background workers (Heartbeat check & Billing cron)
    start_workers(db.clone()).await;

    // Create AppState
    let state = AppState {
        db,
        jwt_secret,
    };

    // Build Axum router
    let app = app_router().with_state(state);

    // Run HTTP API Server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[Server] Running Axum HTTP API on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
