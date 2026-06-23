use rusqlite::Connection;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use uuid::Uuid;

use crate::agent::consent::ConsentBridge;
use crate::memory::meta_brain::MetaBrain;

use crate::agent::interaction_orchestrator::{
    BoxFuture, LlmMessage, LlmProvider, PlannerAgent, ReactContinuation, ReactOutcome, RiskBudget,
    SubTaskResult, UiActionBridge, WorkerAgent,
};

use crate::{MoatError, MoatResult};

// ═════════════════════════════════════════════════════════════════════════════
// § ReAct Loop Constants
// ═════════════════════════════════════════════════════════════════════════════

pub(crate) const LOOP_WINDOW: usize = 8;
pub(crate) const HARD_STOP_SUCCESS: usize = 3;
pub(crate) const HARD_STOP_FAILURE: usize = 3;
pub(crate) const SOFT_WARN_SAME_CALL: usize = 3;

pub mod context;
pub mod loop_detector;
pub mod yield_state;
pub mod agent;
pub use yield_state::YieldSnapshot;
pub mod recovery;

pub use recovery::ContextRehydrator;

// ═════════════════════════════════════════════════════════════════════════════
// § Orchestrator
// ═════════════════════════════════════════════════════════════════════════════
pub struct Orchestrator {
    pub planner: PlannerAgent,
    pub worker: Arc<WorkerAgent>,
    pub meta_brain: Arc<AsyncRwLock<MetaBrain>>,
    pub db: Arc<AsyncMutex<Connection>>,
    pub injector_config: crate::agent::injector::InjectorConfig,
    pub active_agent: Option<crate::agent::llm_types::AgentSpec>,
    pub ui_bridge: Option<Arc<dyn UiActionBridge>>,
    pub cancel_flag: Arc<AtomicBool>,
    pub high_risk_approvals: Option<Arc<RiskBudget>>,
    pub session_id: Option<Uuid>,
}

impl Orchestrator {
    pub fn new(
        planner: PlannerAgent,
        worker: WorkerAgent,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
        injector_config: crate::agent::injector::InjectorConfig,
    ) -> Self {
        Self {
            planner,
            worker: Arc::new(worker),
            meta_brain,
            db,
            injector_config,
            active_agent: None,
            ui_bridge: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            high_risk_approvals: None,
            session_id: None,
        }
    }

    pub fn new_with_arc(
        planner: PlannerAgent,
        worker: Arc<WorkerAgent>,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
        injector_config: crate::agent::injector::InjectorConfig,
    ) -> Self {
        Self {
            planner,
            worker,
            meta_brain,
            db,
            injector_config,
            active_agent: None,
            ui_bridge: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            high_risk_approvals: None,
            session_id: None,
        }
    }

    pub fn with_active_agent_opt(
        mut self,
        active_agent: Option<crate::agent::llm_types::AgentSpec>,
    ) -> Self {
        self.active_agent = active_agent;
        self
    }

    pub fn with_ui_bridge(mut self, bridge: Arc<dyn UiActionBridge>) -> Self {
        let old = &*self.worker;
        let new_worker = WorkerAgent {
            config: old.config.clone(),
            meta_brain: Arc::clone(&old.meta_brain),
            db: Arc::clone(&old.db),
            consent_bridge: old.consent_bridge.clone(),
            ui_bridge: Some(Arc::clone(&bridge)),
            del: old.del.clone(),
        };
        self.worker = Arc::new(new_worker);
        self.ui_bridge = Some(bridge);
        self
    }

    pub fn with_ui_bridge_opt(mut self, bridge: Option<Arc<dyn UiActionBridge>>) -> Self {
        if let Some(ref b) = bridge {
            let old = &*self.worker;
            let new_worker = WorkerAgent {
                config: old.config.clone(),
                meta_brain: Arc::clone(&old.meta_brain),
                db: Arc::clone(&old.db),
                consent_bridge: old.consent_bridge.clone(),
                ui_bridge: Some(Arc::clone(b)),
                del: old.del.clone(),
            };
            self.worker = Arc::new(new_worker);
        } else {
            let old = &*self.worker;
            let new_worker = WorkerAgent {
                config: old.config.clone(),
                meta_brain: Arc::clone(&old.meta_brain),
                db: Arc::clone(&old.db),
                consent_bridge: old.consent_bridge.clone(),
                ui_bridge: None,
                del: old.del.clone(),
            };
            self.worker = Arc::new(new_worker);
        }
        self.ui_bridge = bridge;
        self
    }

    pub fn with_cancel_flag(mut self, cancel_flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = cancel_flag;
        self
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_session_id_opt(mut self, session_id: Option<Uuid>) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_db(mut self, db: Arc<AsyncMutex<Connection>>) -> Self {
        self.db = db;
        let old = &*self.worker;
        let new_worker = WorkerAgent {
            config: old.config.clone(),
            meta_brain: Arc::clone(&old.meta_brain),
            db: Arc::clone(&self.db),
            consent_bridge: old.consent_bridge.clone(),
            ui_bridge: old.ui_bridge.clone(),
            del: old.del.clone(),
        };
        self.worker = Arc::new(new_worker);
        self
    }

    pub fn with_planner(&self, planner: PlannerAgent) -> Self {
        Self {
            planner,
            worker: Arc::clone(&self.worker),
            meta_brain: Arc::clone(&self.meta_brain),
            db: Arc::clone(&self.db),
            injector_config: self.injector_config.clone(),
            active_agent: self.active_agent.clone(),
            ui_bridge: self.ui_bridge.clone(),
            cancel_flag: Arc::clone(&self.cancel_flag),
            high_risk_approvals: self.high_risk_approvals.clone(),
            session_id: self.session_id,
        }
    }

    pub fn with_high_risk_approvals(mut self, approvals: Arc<RiskBudget>) -> Self {
        self.high_risk_approvals = Some(approvals);
        self
    }

    pub fn with_agent_spec(&self, active_agent: crate::agent::llm_types::AgentSpec) -> Self {
        let mut cloned = self.with_planner(self.planner.clone());
        cloned.active_agent = Some(active_agent);
        cloned
    }

    pub fn worker_consent_bridge(&self) -> Option<Arc<ConsentBridge>> {
        self.worker.consent_bridge.clone()
    }

    pub fn planner_provider(&self) -> Arc<dyn LlmProvider> {
        self.planner.provider.clone()
    }

    pub fn injector_config_ref(&self) -> &crate::agent::injector::InjectorConfig {
        &self.injector_config
    }

    pub fn set_planner_model(&self, model: &str) {
        self.planner.set_model(model);
    }

    pub async fn run_react(
        &self,
        objective: &str,
        context_bootstrap: &str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> MoatResult<(Vec<SubTaskResult>, Vec<LlmMessage>)> {
        match self
            .run_react_scoped_mode(
                objective,
                context_bootstrap,
                root_node_id,
                status_tx,
                None,
                None,
                None,
            )
            .await?
        {
            ReactOutcome::Completed {
                results,
                context_snapshot,
            } => Ok((results, context_snapshot)),
            ReactOutcome::Yielded(_) => Err(MoatError::Orchestrator(
                "unexpected yielded sub-agent request in inline ReAct mode".into(),
            )),
            ReactOutcome::Terminated(kill_msg) => Err(MoatError::Orchestrator(kill_msg)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_react_outcome(
        &self,
        objective: &str,
        context_bootstrap: &str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
        continuation: Option<ReactContinuation>,
        injected_observation: Option<String>,
        execution_world_state: Option<
            Arc<tokio::sync::RwLock<crate::agent::system::ExecutionWorldState>>,
        >,
    ) -> MoatResult<ReactOutcome> {
        self.run_react_scoped_mode(
            objective,
            context_bootstrap,
            root_node_id,
            status_tx,
            continuation,
            injected_observation,
            execution_world_state,
        )
        .await
    }

    pub fn run_react_scoped<'a>(
        &'a self,
        objective: &'a str,
        context_bootstrap: &'a str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
    ) -> BoxFuture<'a, MoatResult<Vec<SubTaskResult>>> {
        Box::pin(async move {
            match self
                .run_react_scoped_mode(
                    objective,
                    context_bootstrap,
                    root_node_id,
                    status_tx,
                    None,
                    None,
                    None,
                )
                .await?
            {
                ReactOutcome::Completed { results, .. } => Ok(results),
                ReactOutcome::Yielded(_) => Err(MoatError::Orchestrator(
                    "unexpected yielded sub-agent request in inline scoped ReAct mode".into(),
                )),
                ReactOutcome::Terminated(kill_msg) => Err(MoatError::Orchestrator(kill_msg)),
            }
        })
    }

    pub fn run_react_scoped_outcome<'a>(
        &'a self,
        objective: &'a str,
        context_bootstrap: &'a str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
        continuation: Option<ReactContinuation>,
        injected_observation: Option<String>,
    ) -> BoxFuture<'a, MoatResult<ReactOutcome>> {
        self.run_react_scoped_mode(
            objective,
            context_bootstrap,
            root_node_id,
            status_tx,
            continuation,
            injected_observation,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_react_scoped_mode<'a>(
        &'a self,
        objective: &'a str,
        context_bootstrap: &'a str,
        root_node_id: u64,
        status_tx: Option<tokio::sync::mpsc::Sender<String>>,
        continuation: Option<ReactContinuation>,
        _injected_observation: Option<String>,
        execution_world_state: Option<
            Arc<tokio::sync::RwLock<crate::agent::system::ExecutionWorldState>>,
        >,
    ) -> BoxFuture<'a, MoatResult<ReactOutcome>> {
        use crate::agent::interaction_loop::agent::Agent;
        let mut agent = Agent::new(
            self.active_agent.clone(),
            Arc::clone(&self.worker),
            Arc::clone(&self.meta_brain),
            Arc::clone(&self.db),
            self.planner.clone(),
            self.injector_config.clone(),
        );
        agent.ui_bridge = self.ui_bridge.clone();
        agent.cancel_flag = Arc::clone(&self.cancel_flag);
        agent.execution_world_state = execution_world_state;
        agent.max_turns = self.planner.config.max_subtasks.max(1);
        agent.high_risk_approvals = self.high_risk_approvals.clone();
        agent.session_id = self.session_id;

        Box::pin(async move {
            agent
                .run(
                    objective,
                    context_bootstrap,
                    root_node_id,
                    status_tx,
                    continuation,
                    None,
                    None,
                )
                .await
        })
    }
}
