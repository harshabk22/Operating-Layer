use moat_core::v2::dashboard::core::{DashboardController, QueryBus, ViewComponent};

#[test]
fn test_dashboard_strictly_queries_read_models() {
    // 1. Instantiate the CQRS Query Bus
    let mut query_bus = QueryBus::new();

    // 2. Register Read Models (Projections)
    // The Dashboard MUST only read from these, never from the Event Store directly.
    query_bus.register_query("get_case_view", |_params| {
        Some("Case View Data Loaded".to_string())
    });

    query_bus.register_query("get_actor_view", |_params| {
        Some("Actor View Data Loaded".to_string())
    });

    query_bus.register_query("get_incident_view", |_params| {
        Some("Incident View Data Loaded".to_string())
    });

    query_bus.register_query("get_projection_view", |_params| {
        Some("Projection View Data Loaded".to_string())
    });

    // 3. Instantiate the DashboardController
    let mut dashboard = DashboardController::new();

    // 4. Add UI View Components that map to the Read Models
    dashboard.add_component(ViewComponent::new("case_panel", "get_case_view"));
    dashboard.add_component(ViewComponent::new("actor_panel", "get_actor_view"));
    dashboard.add_component(ViewComponent::new("incident_panel", "get_incident_view"));
    dashboard.add_component(ViewComponent::new(
        "projection_panel",
        "get_projection_view",
    ));

    // 5. Render the Dashboard
    let output = dashboard.render_dashboard(&query_bus, "{}".to_string());

    // 6. Validate the output to prove the Dashboard queried the Read Models successfully
    assert_eq!(output.len(), 4, "Dashboard should render 4 components.");

    assert_eq!(
        output[0], "Component[case_panel]: Case View Data Loaded",
        "Case View should be queried from QueryBus"
    );
    assert_eq!(
        output[1], "Component[actor_panel]: Actor View Data Loaded",
        "Actor View should be queried from QueryBus"
    );
    assert_eq!(
        output[2], "Component[incident_panel]: Incident View Data Loaded",
        "Incident View should be queried from QueryBus"
    );
    assert_eq!(
        output[3], "Component[projection_panel]: Projection View Data Loaded",
        "Projection View should be queried from QueryBus"
    );

    println!("Success: Dashboard strictly queries Read Models via QueryBus.");
}
