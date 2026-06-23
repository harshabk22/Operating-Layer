use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDefinition {
    pub name: String,
    pub symbol: String,
    pub total_supply: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorDefinition {
    pub id: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub condition: String,
    pub error_message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConstraintBuilder {
    constraints: Vec<Constraint>,
}

impl ConstraintBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_condition(
        mut self,
        condition: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        self.constraints.push(Constraint {
            condition: condition.into(),
            error_message: error_message.into(),
        });
        self
    }

    pub fn build(self) -> Vec<Constraint> {
        self.constraints
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMachineDefinition {
    pub states: Vec<String>,
    pub initial_state: String,
    pub transitions: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct StateMachineBuilder {
    states: Vec<String>,
    initial_state: Option<String>,
    transitions: HashMap<String, Vec<String>>,
}

impl StateMachineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_state(mut self, state: impl Into<String>) -> Self {
        self.states.push(state.into());
        self
    }

    pub fn set_initial_state(mut self, state: impl Into<String>) -> Self {
        self.initial_state = Some(state.into());
        self
    }

    pub fn add_transition(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.transitions
            .entry(from.into())
            .or_default()
            .push(to.into());
        self
    }

    pub fn build(self) -> Result<StateMachineDefinition, &'static str> {
        let initial_state = self.initial_state.ok_or("Initial state not set")?;
        if !self.states.contains(&initial_state) {
            return Err("Initial state must be one of the defined states");
        }
        Ok(StateMachineDefinition {
            states: self.states,
            initial_state,
            transitions: self.transitions,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDefinition {
    pub name: String,
    pub version: String,
    pub assets: Vec<AssetDefinition>,
    pub actors: Vec<ActorDefinition>,
    pub state_machines: Vec<StateMachineDefinition>,
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, Default)]
pub struct PackageBuilder {
    name: Option<String>,
    version: Option<String>,
    assets: Vec<AssetDefinition>,
    actors: Vec<ActorDefinition>,
    state_machines: Vec<StateMachineDefinition>,
    constraints: Vec<Constraint>,
}

impl PackageBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn set_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn add_asset(mut self, asset: AssetDefinition) -> Self {
        self.assets.push(asset);
        self
    }

    pub fn add_actor(mut self, actor: ActorDefinition) -> Self {
        self.actors.push(actor);
        self
    }

    pub fn add_state_machine(mut self, state_machine: StateMachineDefinition) -> Self {
        self.state_machines.push(state_machine);
        self
    }

    pub fn add_constraints(mut self, mut constraints: Vec<Constraint>) -> Self {
        self.constraints.append(&mut constraints);
        self
    }

    pub fn build(self) -> Result<PackageDefinition, &'static str> {
        let name = self.name.ok_or("Package name not set")?;
        let version = self.version.ok_or("Package version not set")?;

        Ok(PackageDefinition {
            name,
            version,
            assets: self.assets,
            actors: self.actors,
            state_machines: self.state_machines,
            constraints: self.constraints,
        })
    }
}
