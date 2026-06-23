use crate::agent::capabilities::Capability;
use crate::agent::consent::ConsentBridge;
use crate::agent::execution::{DelErrorCode, DelExecutor, ExecutionRequest, ExecutionResult};
use crate::agent::interaction_orchestrator::{
    ActionIntent, RiskBudget, TaskStatus, UiActionBridge, WorkerConfig,
};
use crate::memory::meta_brain::{MetaBrain, PermissionFlags};
use crate::MoatResult;
use rusqlite::Connection;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock};

pub mod consent {
    use std::collections::HashMap;
    use std::sync::LazyLock;
    use std::sync::Mutex;

    static DENIALS: LazyLock<Mutex<HashMap<String, usize>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub fn cascade_denial_increment(key: &str) {
        if let Ok(mut map) = DENIALS.lock() {
            *map.entry(key.to_string()).or_insert(0) += 1;
        }
    }

    pub fn cascade_denial_count(key: &str) -> usize {
        if let Ok(map) = DENIALS.lock() {
            map.get(key).copied().unwrap_or(0)
        } else {
            0
        }
    }

    pub fn value_to_string(value: Option<&serde_json::Value>) -> String {
        match value {
            None => String::new(),
            Some(serde_json::Value::Null) => String::new(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        }
    }
}

pub struct WorkerAgent {
    pub config: WorkerConfig,
    pub meta_brain: Arc<AsyncRwLock<MetaBrain>>,
    pub db: Arc<AsyncMutex<Connection>>,
    pub consent_bridge: Option<Arc<ConsentBridge>>,
    pub ui_bridge: Option<Arc<dyn UiActionBridge>>,
    pub del: DelExecutor,
}

impl WorkerAgent {
    pub fn new(
        config: WorkerConfig,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
    ) -> Result<Self, anyhow::Error> {
        let consent_bridge = Arc::new(ConsentBridge::new(Box::new(|_| {})));
        let del = DelExecutor::new(Arc::clone(&meta_brain), Arc::clone(&consent_bridge))
            .map_err(|e| anyhow::anyhow!("[WorkerAgent] DEL initialization failed: {}", e))?;
        Ok(Self {
            config,
            meta_brain,
            db,
            consent_bridge: None,
            ui_bridge: None,
            del,
        })
    }

    pub fn with_consent_bridge(
        config: WorkerConfig,
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        db: Arc<AsyncMutex<Connection>>,
        consent_bridge: Arc<ConsentBridge>,
    ) -> Result<Self, anyhow::Error> {
        let del = DelExecutor::new(Arc::clone(&meta_brain), Arc::clone(&consent_bridge))
            .map_err(|e| anyhow::anyhow!("[WorkerAgent] DEL initialization failed: {}", e))?;
        Ok(Self {
            config,
            meta_brain,
            db,
            consent_bridge: Some(consent_bridge),
            ui_bridge: None,
            del,
        })
    }

    pub async fn execute_capability_task(
        &self,
        intent: &ActionIntent,
        _root_node_id: u64,
        session_id: &str,
        agent_label: &str,
        _high_risk_approvals: Arc<RiskBudget>,
        execution_world_state: Option<
            Arc<tokio::sync::RwLock<crate::agent::system::ExecutionWorldState>>,
        >,
    ) -> MoatResult<TaskStatus> {
        if let Some(ref state_lock) = execution_world_state {
            let state = state_lock.read().await;
            if state.is_capability_denied(&intent.action, &intent.target) {
                return Ok(TaskStatus::Failed {
                    exit_code: None,
                    stderr: "[SYSTEM SHORT-CIRCUIT] This capability was already denied in this session. You MUST use 'done' or 'user:ask_choice'. Do not retry.".into(),
                });
            }
        }

        match intent.action.as_str() {
            "meta:query_brain" => {
                let keyword = intent
                    .payload
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let brain = self.meta_brain.read().await;
                let result = brain.extract_relevant_subgraph(&keyword);
                Ok(TaskStatus::Success { stdout: result })
            }
            "net:curl" => {
                let domain = if let Ok(url) = url::Url::parse(&intent.target) {
                    url.host_str().unwrap_or(&intent.target).to_string()
                } else if let Ok(url) = url::Url::parse(&format!("http://{}", intent.target)) {
                    url.host_str().unwrap_or(&intent.target).to_string()
                } else {
                    intent.target.clone()
                };
                if !self
                    .check_domain_permission(&domain, intent, session_id, agent_label)
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: net permission not granted for {:?}",
                            domain
                        ),
                    });
                }
                Ok(TaskStatus::Success {
                    stdout: "domain allowed".to_string(),
                })
            }
            "fs_read" | "read_file" | "fs:read" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_READ,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: read permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "read_file",
                        Capability::FileRead,
                        args.into(),
                        session_id,
                    )
                    .await;
                Ok(execution_result_to_task_status(del_result))
            }
            "fs_write" | "write_file" | "fs:write" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_WRITE,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: write permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let data = intent
                    .payload
                    .as_ref()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                args.insert("content".to_string(), json!(data));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "write_file",
                        Capability::FileWrite,
                        args.into(),
                        session_id,
                    )
                    .await;
                if del_result.success {
                    let db = self.db.lock().await;
                    if let Err(err) = crate::memory::mark_sandbox_written(&db, &resolved, true) {
                        eprintln!(
                            "[Memory] failed to mark sandbox-written file {}: {err}",
                            resolved.display()
                        );
                    }
                }
                Ok(execution_result_to_task_status(del_result))
            }
            "fs_exists" | "fs:exists" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_READ,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: read permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "fs_exists",
                        Capability::FileExists,
                        args.into(),
                        session_id,
                    )
                    .await;
                Ok(execution_result_to_task_status(del_result))
            }
            "fs_delete" | "fs:delete" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_WRITE,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: write permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "fs_delete",
                        Capability::FileDelete,
                        args.into(),
                        session_id,
                    )
                    .await;
                Ok(execution_result_to_task_status(del_result))
            }
            "fs_mkdir" | "fs:mkdir" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_WRITE,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: write permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "fs_mkdir",
                        Capability::FileWrite,
                        args.into(),
                        session_id,
                    )
                    .await;
                Ok(execution_result_to_task_status(del_result))
            }
            "fs_list" | "list_files" | "fs:list" => {
                let resolved = match resolve_workspace_target(&intent.target) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(TaskStatus::Failed {
                            exit_code: None,
                            stderr: e,
                        })
                    }
                };
                if !self
                    .check_path_permission(
                        PermissionFlags::FILE_READ,
                        &resolved.to_string_lossy(),
                        intent,
                        session_id,
                        agent_label,
                    )
                    .await?
                {
                    return Ok(TaskStatus::Failed {
                        exit_code: None,
                        stderr: format!(
                            "CAPABILITY DENIED: read permission not granted for {:?}",
                            resolved
                        ),
                    });
                }
                let mut args = serde_json::Map::new();
                args.insert("path".to_string(), json!(resolved.to_string_lossy()));
                let del_result = self
                    .execute_del(
                        agent_label,
                        "list_files",
                        Capability::FileRead,
                        args.into(),
                        session_id,
                    )
                    .await;
                Ok(execution_result_to_task_status(del_result))
            }
            other => Ok(TaskStatus::Failed {
                exit_code: None,
                stderr: format!("unsupported action: {other}"),
            }),
        }
    }

    async fn check_path_permission(
        &self,
        permission: PermissionFlags,
        path_str: &str,
        intent: &ActionIntent,
        session_id: &str,
        agent_label: &str,
    ) -> MoatResult<bool> {
        let expanded = crate::agent::execution::gate2_policy::Gate2Policy::expand_tilde(path_str);
        if !Path::new(&expanded).is_absolute() {
            return Ok(false);
        }
        {
            let brain = self.meta_brain.read().await;
            if brain.path_has_permission_scoped(&expanded, permission, agent_label, session_id)
                || brain.path_has_permission(&expanded, permission)
            {
                return Ok(true);
            }
        }

        if let Some(ref bridge) = self.consent_bridge {
            let approved = bridge
                .request_action_consent(intent, agent_label, session_id)
                .await?;
            if approved {
                let path_to_grant = if permission.contains(PermissionFlags::FILE_WRITE)
                    || permission.contains(PermissionFlags::FILE_DELETE)
                {
                    if let Some(parent) = Path::new(&expanded).parent() {
                        parent.to_string_lossy().to_string()
                    } else {
                        expanded.clone()
                    }
                } else {
                    expanded.clone()
                };

                {
                    let mut brain = self.meta_brain.write().await;
                    brain.grant_path_permission_scoped(
                        &path_to_grant,
                        permission,
                        agent_label,
                        session_id,
                    )?;
                }
                let _ = MetaBrain::flush_arc(Arc::clone(&self.meta_brain)).await;

                // Record approval in database
                let conn = self.db.lock().await;
                let _ = conn.execute(
                    "CREATE TABLE IF NOT EXISTS approvals (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_type TEXT NOT NULL,
                        capability TEXT NOT NULL,
                        scope_type TEXT NOT NULL,
                        scope_value TEXT NOT NULL,
                        timestamp INTEGER NOT NULL
                    )",
                    [],
                );
                let now = chrono::Utc::now().timestamp();
                let _ = conn.execute(
                    "INSERT INTO approvals (agent_type, capability, scope_type, scope_value, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        agent_label,
                        format!("{:?}", permission),
                        "path",
                        path_to_grant,
                        now
                    ],
                );
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn check_domain_permission(
        &self,
        domain: &str,
        intent: &ActionIntent,
        session_id: &str,
        agent_label: &str,
    ) -> MoatResult<bool> {
        {
            let brain = self.meta_brain.read().await;
            if brain.domain_has_permission_scoped(
                domain,
                PermissionFlags::NET_OUTBOUND,
                agent_label,
                session_id,
            ) || brain.domain_has_permission(domain, PermissionFlags::NET_OUTBOUND)
            {
                return Ok(true);
            }
        }

        if let Some(ref bridge) = self.consent_bridge {
            let approved = bridge
                .request_action_consent(intent, agent_label, session_id)
                .await?;
            if approved {
                {
                    let mut brain = self.meta_brain.write().await;
                    brain.grant_domain_permission_scoped(
                        domain,
                        PermissionFlags::NET_OUTBOUND,
                        agent_label,
                        session_id,
                    )?;
                }
                let _ = MetaBrain::flush_arc(Arc::clone(&self.meta_brain)).await;

                // Record approval in database
                let conn = self.db.lock().await;
                let _ = conn.execute(
                    "CREATE TABLE IF NOT EXISTS approvals (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        agent_type TEXT NOT NULL,
                        capability TEXT NOT NULL,
                        scope_type TEXT NOT NULL,
                        scope_value TEXT NOT NULL,
                        timestamp INTEGER NOT NULL
                    )",
                    [],
                );
                let now = chrono::Utc::now().timestamp();
                let _ = conn.execute(
                    "INSERT INTO approvals (agent_type, capability, scope_type, scope_value, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        agent_label,
                        "net:curl",
                        "domain",
                        domain,
                        now
                    ],
                );
                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn execute_del(
        &self,
        agent_id: &str,
        tool_name: &str,
        capability: Capability,
        args: serde_json::Value,
        session_id: &str,
    ) -> ExecutionResult {
        let request_id = uuid::Uuid::new_v4().to_string();
        let req = ExecutionRequest {
            request_id,
            agent_id: agent_id.to_string(),
            tool_name: tool_name.to_string(),
            capability,
            args,
            working_dir: crate::config::workspace_root(),
            timestamp: chrono::Utc::now().timestamp(),
            session_id: session_id.to_string(),
        };
        self.del.execute(req).await
    }
}

fn resolve_workspace_target(target: &str) -> Result<PathBuf, String> {
    let expanded = crate::agent::execution::gate2_policy::Gate2Policy::expand_tilde(target.trim());
    let path = Path::new(&expanded);
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let root = crate::config::workspace_root();
        Ok(root.join(path))
    }
}

fn execution_result_to_task_status(res: ExecutionResult) -> TaskStatus {
    if res.success {
        TaskStatus::Success { stdout: res.output }
    } else {
        match res.error_code {
            DelErrorCode::YieldedForConsent { request_id, .. } => {
                TaskStatus::YieldedForConsent { request_id }
            }
            _ => TaskStatus::Failed {
                exit_code: res.exit_code,
                stderr: if !res.output.is_empty() {
                    res.output
                } else {
                    res.denied_reason
                        .unwrap_or_else(|| "Capability denied".to_string())
                },
            },
        }
    }
}
