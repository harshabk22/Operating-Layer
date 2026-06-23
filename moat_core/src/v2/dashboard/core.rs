use std::collections::HashMap;

/// CommandBus: handles state mutations.
pub struct CommandBus {
    handlers: HashMap<String, Box<dyn Fn(String) -> Result<(), String> + Send + Sync>>,
}

impl Default for CommandBus {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBus {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    pub fn register_command<F>(&mut self, command_name: &str, handler: F)
    where
        F: Fn(String) -> Result<(), String> + Send + Sync + 'static,
    {
        self.handlers
            .insert(command_name.to_string(), Box::new(handler));
    }

    pub fn dispatch(&self, command_name: &str, payload: String) -> Result<(), String> {
        if let Some(handler) = self.handlers.get(command_name) {
            handler(payload)
        } else {
            Err("Command handler not found".to_string())
        }
    }
}

/// QueryBus: routes read queries to the appropriate Read Model.
pub struct QueryBus {
    routes: HashMap<String, Box<dyn Fn(String) -> Option<String> + Send + Sync>>,
}

impl Default for QueryBus {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryBus {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn register_query<F>(&mut self, query_name: &str, handler: F)
    where
        F: Fn(String) -> Option<String> + Send + Sync + 'static,
    {
        self.routes
            .insert(query_name.to_string(), Box::new(handler));
    }

    pub fn execute_query(&self, query_name: &str, params: String) -> Option<String> {
        self.routes
            .get(query_name)
            .and_then(|handler| handler(params))
    }
}

/// ViewComponent: a UI element that gets its data purely via QueryBus.
pub struct ViewComponent {
    pub id: String,
    pub query_name: String,
}

impl ViewComponent {
    pub fn new(id: &str, query_name: &str) -> Self {
        Self {
            id: id.to_string(),
            query_name: query_name.to_string(),
        }
    }

    pub fn render(&self, query_bus: &QueryBus, params: String) -> String {
        let data = query_bus
            .execute_query(&self.query_name, params)
            .unwrap_or_else(|| "No Data".to_string());
        format!("Component[{}]: {}", self.id, data)
    }
}

/// DashboardController: Orchestrates the view.
/// The dashboard framework must ONLY read from CQRS Read Models (do not mutate state directly).
/// It must use the Command Bus for writes.
pub struct DashboardController {
    pub components: Vec<ViewComponent>,
}

impl Default for DashboardController {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardController {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn add_component(&mut self, component: ViewComponent) {
        self.components.push(component);
    }

    pub fn render_dashboard(&self, query_bus: &QueryBus, params: String) -> Vec<String> {
        self.components
            .iter()
            .map(|c| c.render(query_bus, params.clone()))
            .collect()
    }

    pub fn handle_action(
        &self,
        command_bus: &CommandBus,
        action: &str,
        payload: String,
    ) -> Result<(), String> {
        command_bus.dispatch(action, payload)
    }
}
