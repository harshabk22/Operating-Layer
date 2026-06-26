use crate::api::auth::{require_auth, require_owner_role, AuthStore};
use crate::api::google_routes::*;
use crate::api::routes::*;
use crate::api::ws::ws_handler;
use crate::business::operator_api::OperatorService;
use crate::business::owner::OwnerService;
use crate::business::runtime::CustomerInteractionRuntime;
use axum::{
    middleware::from_fn,
    response::IntoResponse,
    routing::{delete, get, patch, post},
    Router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct ApiState {
    pub runtime: Arc<CustomerInteractionRuntime>,
    pub operator_service: Arc<OperatorService>,
    pub owner_service: Arc<OwnerService>,
    pub auth_store: Arc<AuthStore>,
}

fn get_frontend_dist_dir() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.join("frontend/dist").exists() {
        cwd.join("frontend/dist")
    } else if cwd.parent().map(|p| p.join("frontend/dist").exists()).unwrap_or(false) {
        cwd.parent().unwrap().join("frontend/dist")
    } else {
        std::path::PathBuf::from("../frontend/dist")
    }
}

async fn serve_index() -> impl axum::response::IntoResponse {
    let path = get_frontend_dist_dir().join("index.html");
    match tokio::fs::read_to_string(path).await {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(_) => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}


pub async fn start_api_server(
    runtime: Arc<CustomerInteractionRuntime>,
    port: u16,
) -> anyhow::Result<(tokio::task::JoinHandle<()>, u16)> {
    let notifier = runtime.notifier.clone();
    let operator_service = Arc::new(OperatorService::new(runtime.clone(), notifier.clone()));
    let owner_service = Arc::new(OwnerService::new(runtime.clone(), notifier.clone()));
    let auth_store = Arc::new(AuthStore::new());

    let state = ApiState {
        runtime,
        operator_service,
        owner_service,
        auth_store,
    };

    let sessions = state.auth_store.sessions.clone();

    // Authenticated API router (Operator & Owner roles allowed)
    let auth_api_router = Router::new()
        // Thread Endpoints
        .route("/threads", get(list_threads_handler))
        .route("/threads/:id", get(get_thread_handler))
        .route("/threads/:id/pause", post(pause_thread_handler))
        .route("/threads/:id/resume", post(resume_thread_handler))
        .route("/threads/:id/takeover", post(takeover_thread_handler))
        .route("/threads/:id/release", post(release_thread_handler))
        .route("/threads/:id/message", post(send_message_handler))
        .route(
            "/threads/:id/approve_suggestion",
            post(approve_suggestion_handler),
        )
        // Bookings Endpoints
        .route("/bookings", get(list_bookings_handler))
        .route("/bookings/:id", get(get_booking_handler))
        // Escalations Endpoints
        .route("/escalations", get(list_escalations_handler))
        // Opportunity Pipeline Endpoints
        .route("/opportunities/:id", get(get_opportunity_handler))
        .route(
            "/opportunities/:id/stage",
            patch(update_opportunity_stage_handler),
        )
        // Analytics Endpoints
        .route("/analytics/overview", get(analytics_overview_handler))


        // WebSocket endpoint
        .route("/ws", get(ws_handler))
        // Google Connections Endpoints
        .route("/connections/google/connect", post(google_connect_handler))
        .route("/connections/google/status", get(google_status_handler))
        .route(
            "/connections/google/oauth_config",
            get(get_google_oauth_config_handler).post(save_google_oauth_config_handler),
        )
        .route(
            "/connections/google/disconnect",
            post(google_disconnect_handler),
        )
        .route("/connections/google/config", post(google_config_handler))
        .route(
            "/connections/google/resources",
            get(google_resources_handler),
        )
        .layer(from_fn(move |req, next| {
            require_auth(sessions.clone(), req, next)
        }));

    // Owner-Only API router
    let owner_sessions = state.auth_store.sessions.clone();
    let owner_api_router = Router::new()
        // Authorization Endpoints
        .route("/authorizations", get(list_authorizations_handler))
        .route(
            "/authorizations/:id/approve",
            post(approve_authorization_handler),
        )
        .route(
            "/authorizations/:id/reject",
            post(reject_authorization_handler),
        )
        // Escalation Resolve Endpoint
        .route("/escalations/:id/resolve", post(resolve_escalation_handler))
        .layer(from_fn(require_owner_role))
        .layer(from_fn(move |req, next| {
            require_auth(owner_sessions.clone(), req, next)
        }));

    // Secured platform, sales, operations, realestate, and system routes
    let secured_sessions = state.auth_store.sessions.clone();
    let secured_router = Router::new()
        // platform
        .route("/platform/packages/install", post(crate::api::routes::install_package_handler))
        .route("/platform/packages", get(crate::api::routes::list_packages_handler))
        .route("/platform/tenants", post(crate::api::routes::deploy_tenant_handler))
        .route("/platform/tenants/:id", delete(crate::api::routes::delete_tenant_handler))
        .route("/platform/packages/:id", delete(crate::api::routes::delete_package_handler))
        .route("/platform/packages/:id/upgrade", post(crate::api::routes::upgrade_package_handler))
        .route("/platform/packages/:id/rollback", post(crate::api::routes::rollback_package_handler))

        // sales package APIs
        .route("/sales/leads", get(crate::api::routes::list_leads_handler).post(crate::api::routes::create_lead_handler))
        .route("/sales/leads/:id/qualify", post(crate::api::routes::qualify_lead_handler))
        .route("/sales/leads/:id", patch(crate::api::routes::update_lead_handler).delete(crate::api::routes::delete_lead_handler))
        .route("/sales/opportunities", get(crate::api::routes::list_opportunities_handler).post(crate::api::routes::create_opportunity_handler))
        .route("/sales/opportunities/:id", patch(crate::api::routes::update_opportunity_handler).delete(crate::api::routes::delete_opportunity_handler))

        // real estate package APIs
        .route("/realestate/properties", get(crate::api::routes::list_properties_handler).post(crate::api::routes::create_property_handler))
        .route("/realestate/properties/:id", delete(crate::api::routes::delete_property_handler))
        .route("/realestate/offers", get(crate::api::routes::list_offers_handler).post(crate::api::routes::create_offer_handler))
        .route("/realestate/offers/:id", patch(crate::api::routes::update_offer_handler).delete(crate::api::routes::delete_offer_handler))
        .route("/realestate/deals", get(crate::api::routes::list_deals_handler).post(crate::api::routes::create_deal_handler))
        .route("/realestate/deals/:id", patch(crate::api::routes::update_deal_handler).delete(crate::api::routes::delete_deal_handler))

        // operations / notifications package APIs
        .route("/operations/notifications", post(crate::api::routes::send_notification_handler).get(crate::api::routes::list_notifications_handler))
        .route("/operations/templates", get(crate::api::routes::list_templates_handler).post(crate::api::routes::create_template_handler))
        .route("/operations/templates/:id", delete(crate::api::routes::delete_template_handler))



        // system / dashboard / rbac
        .route("/system/rebuild_projections", post(crate::api::routes::rebuild_projections_handler))
        .route("/dashboard/kpis", get(crate::api::routes::get_dashboard_kpis_handler))
        .route("/system/events", get(crate::api::routes::list_events_handler))
        .route("/system/rbac", get(crate::api::routes::rbac_guard_handler))
        // setup / supervisor
        .route("/setup/status", get(crate::api::setup::get_setup_status_handler))
        .route("/setup/configure", post(crate::api::setup::configure_provider_handler))
        .route("/setup/test/:id", post(crate::api::setup::test_connection_handler))
        .route("/supervisor/tasks", get(crate::api::setup::list_supervisor_tasks_handler))
        .route("/setup/providers", get(crate::api::setup::list_providers_handler))
        .route("/supervisor/chat", get(crate::api::setup::get_supervisor_chat_handler))
        .route("/supervisor/chat", post(crate::api::setup::post_supervisor_chat_handler))
        .layer(from_fn(crate::api::auth::rbac_middleware))
        .layer(from_fn(move |req, next| {
            require_auth(secured_sessions.clone(), req, next)
        }));

    let mut api_routes = Router::new()
        .route("/login", post(login_handler))
        .route("/logout", post(logout_handler))
        .route("/oauth/callback", get(oauth_callback_handler))
        .merge(secured_router)
        .merge(auth_api_router)
        .merge(owner_api_router);

    if let Some(registry_lock) = crate::package::DYNAMIC_PACKAGE_REGISTRY.get() {
        let registry = registry_lock.read();
        for package in registry.values() {
            api_routes = package.register_routes(api_routes);
        }
    }

    let dist_dir = get_frontend_dist_dir();
    let index_file = dist_dir.join("index.html");

    // General Router
    let app = Router::new()
        .fallback_service(
            tower_http::services::ServeDir::new(&dist_dir).fallback(
                tower_http::services::ServeFile::new(&index_file),
            ),
        )
        .route("/login", get(serve_index))
        .nest("/api", api_routes.clone())
        .merge(api_routes)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    let local_port = listener.local_addr()?.port();
    println!("[Dashboard API] Binding to 127.0.0.1:{}", local_port);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("[Dashboard API] Server error: {:?}", e);
        }
    });

    Ok((handle, local_port))
}
