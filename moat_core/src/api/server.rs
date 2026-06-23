use crate::api::auth::{require_auth, require_owner_role, AuthStore};
use crate::api::google_routes::*;
use crate::api::routes::*;
use crate::api::ws::ws_handler;
use crate::business::operator_api::OperatorService;
use crate::business::owner::OwnerService;
use crate::business::runtime::CustomerInteractionRuntime;
use axum::{
    middleware::from_fn,
    response::Html,
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

async fn serve_dashboard() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
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
        .route("/opportunities", get(list_opportunities_handler))
        .route("/opportunities/:id", get(get_opportunity_handler))
        .route(
            "/opportunities/:id/stage",
            patch(update_opportunity_stage_handler),
        )
        // Analytics Endpoints
        .route("/analytics/overview", get(analytics_overview_handler))

        // WhatsApp Personal QR Endpoint
        .route("/connectors/whatsapp/qr", get(whatsapp_qr_handler))
        // WhatsApp Personal Status Endpoint
        .route("/connectors/whatsapp/status", get(whatsapp_status_handler))
        // WebSocket endpoint
        .route("/ws", get(ws_handler))
        // Google Connections Endpoints
        .route("/connections/google/connect", post(google_connect_handler))
        .route("/connections/google/status", get(google_status_handler))
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
        .route("/platform/tenants", get(crate::api::routes::list_tenants_handler))
        .route("/platform/tenants/:id", delete(crate::api::routes::delete_tenant_handler))
        .route("/platform/tenants/:id/rollback", post(crate::api::routes::rollback_tenant_handler))
        .route("/platform/packages/:id/upgrade", post(crate::api::routes::upgrade_package_handler))
        .route("/platform/packages/:id/rollback", post(crate::api::routes::rollback_package_handler))
        .route("/platform/packages/:id", delete(crate::api::routes::delete_package_handler))
        .route("/platform/tenants/:id/backup", post(crate::api::routes::backup_tenant_handler))
        .route("/platform/tenants/:id/restore", post(crate::api::routes::restore_tenant_handler))
        // sales
        .route("/sales/leads", post(crate::api::routes::create_lead_handler))
        .route("/sales/leads", get(crate::api::routes::list_leads_handler))
        .route("/sales/leads/:id", delete(crate::api::routes::delete_lead_handler))
        .route("/sales/leads/:id/qualify", post(crate::api::routes::qualify_lead_handler))
        .route("/sales/opportunities", post(crate::api::routes::create_opportunity_handler))
        .route("/sales/opportunities", get(crate::api::routes::list_opportunities_handler))
        .route("/sales/opportunities/:id", delete(crate::api::routes::delete_opportunity_handler))
        .route("/sales/opportunities/:id", patch(crate::api::routes::update_opportunity_handler))
        // operations
        .route("/operations/visits", post(crate::api::routes::schedule_visit_handler))
        .route("/operations/visits", get(crate::api::routes::list_visits_handler))
        .route("/operations/notifications", post(crate::api::routes::send_notification_handler))
        .route("/operations/notifications", get(crate::api::routes::list_notifications_handler))
        .route("/operations/templates", post(crate::api::routes::create_template_handler))
        .route("/operations/templates", get(crate::api::routes::list_templates_handler))
        .route("/operations/templates/:id", delete(crate::api::routes::delete_template_handler))
        .route("/operations/approvals/:id/approve", post(crate::api::routes::approve_action_handler))
        .route("/operations/approvals", get(crate::api::routes::list_approvals_handler))
        // realestate
        .route("/realestate/properties", post(crate::api::routes::create_property_handler))
        .route("/realestate/properties", get(crate::api::routes::list_properties_handler))
        .route("/realestate/properties/:id", delete(crate::api::routes::delete_property_handler))
        .route("/realestate/offers", post(crate::api::routes::create_offer_handler))
        .route("/realestate/offers", get(crate::api::routes::list_offers_handler))
        .route("/realestate/offers/:id", delete(crate::api::routes::delete_offer_handler))
        .route("/realestate/offers/:id", patch(crate::api::routes::update_offer_handler))
        .route("/realestate/deals", post(crate::api::routes::close_deal_handler))
        .route("/realestate/deals", get(crate::api::routes::list_deals_handler))
        .route("/realestate/deals/:id", delete(crate::api::routes::delete_deal_handler))
        .route("/realestate/deals/:id", patch(crate::api::routes::update_deal_handler))
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

    // General Router
    let app = Router::new()
        .route("/", get(serve_dashboard))
        .route("/login", post(login_handler))
        .route("/logout", post(logout_handler))
        .route("/oauth/callback", get(oauth_callback_handler))
        .merge(secured_router)
        .merge(auth_api_router)
        .merge(owner_api_router)
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
