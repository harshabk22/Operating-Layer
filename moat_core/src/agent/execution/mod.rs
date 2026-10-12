#![allow(dead_code)]
pub mod error;
pub mod gate1_schema;
pub mod gate2_policy;
pub mod gate3_sandbox;
pub mod journal;
pub mod journal_chain;
pub mod request;
pub mod result;

use crate::agent::capabilities::Capability;
use crate::agent::consent::ConsentBridge;
use crate::memory::meta_brain::{MetaBrain, PermissionFlags};
use anyhow::Result;
pub use error::ExecutionError;
use gate1_schema::Gate1Schema;
use gate2_policy::Gate2Policy;
use gate3_sandbox::Gate3Sandbox;
use journal::ExecutionJournal;
pub use request::ExecutionRequest;
pub use result::{DelErrorCode, ExecutionResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock as AsyncRwLock;

#[derive(Clone)]
pub struct DelExecutor {
    policy: Gate2Policy,
    journal: ExecutionJournal,
    meta_brain: Arc<AsyncRwLock<MetaBrain>>,
    consent_bridge: Arc<ConsentBridge>,
    consent_timeout: Duration,
}

impl DelExecutor {
    pub fn new(
        meta_brain: Arc<AsyncRwLock<MetaBrain>>,
        consent_bridge: Arc<ConsentBridge>,
    ) -> Result<Self> {
        Ok(Self {
            policy: Self::load_policy(),
            journal: ExecutionJournal::open()?,
            meta_brain,
            consent_bridge,
            consent_timeout: Duration::from_secs(300),
        })
    }

    pub fn reset_denials(&self, agent_id: &str) {
        self.journal.record_success(agent_id);
    }

    fn load_policy() -> Gate2Policy {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut candidates: Vec<std::path::PathBuf> = vec![
            dirs::home_dir()
                .unwrap_or_default()
                .join(".ageex")
                .join("policy.toml"),
            cwd.join("moat_core").join("policies").join("default.toml"),
            cwd.join("policies").join("default.toml"),
            std::path::PathBuf::from("moat_core/policies/default.toml"),
            std::path::PathBuf::from("policies/default.toml"),
        ];
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join("moat_core").join("policies").join("default.toml"));
            candidates.push(parent.join("policies").join("default.toml"));
        }
        for path in &candidates {
            if path.exists() {
                eprintln!("[DEL] Loading policy from {:?}", path);
                match Gate2Policy::load_from_file(path) {
                    Ok(p) => return p,
                    Err(e) => eprintln!("[DEL] Failed to load {:?}: {e}", path),
                }
            }
        }
        eprintln!("[DEL] No policy file found, using defaults");
        Gate2Policy::default()
    }

    fn target_from_req(&self, req: &ExecutionRequest) -> String {
        req.args
            .get("path")
            .or_else(|| req.args.get("url"))
            .or_else(|| req.args.get("app_name"))
            .or_else(|| req.args.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or(&req.tool_name)
            .to_string()
    }

    async fn request_consent(&self, req: &ExecutionRequest) -> bool {
        let target = self.target_from_req(req);
        self.consent_bridge
            .request_consent(&req.tool_name, &target)
            .await
    }

    pub async fn execute(&self, req: ExecutionRequest) -> ExecutionResult {
        eprintln!("[DEL] Request {} → {}", req.request_id, req.tool_name);

        // ── Strike check ──
        let strikes = self.journal.consecutive_denials(&req.agent_id);
        if strikes >= 3 {
            return ExecutionResult::gate2_denied(
                &req.request_id,
                &format!(
                    "Agent '{}' terminated after {} consecutive denials",
                    req.agent_id, strikes
                ),
            )
            .with_error_code(DelErrorCode::PermissionDenied {
                target: req.agent_id.clone(),
                reason: format!("terminated after {} consecutive denials", strikes),
            });
        }

        if let Err(e) = self.journal.log_request(&req) {
            eprintln!("[DEL] Journal write failed: {}", e);
        }

        // Gate 1
        let req = match Gate1Schema::validate(req) {
            Ok(r) => r,
            Err(result) => {
                self.journal.record_denied(&result.request_id);
                self.journal.log_result(&result).ok();
                return result;
            }
        };

        // Gate 2: Policy + MetaBrain + Consent
        if let Err(result) = self.policy.enforce(&req) {
            self.journal.record_denied(&req.agent_id);
            self.journal.log_result(&result).ok();
            return result;
        }

        // MetaBrain permission check
        let has_perm = {
            let brain = self.meta_brain.read().await;
            brain.has_permission_for_request(&req)
        };

        if !has_perm {
            if self.request_consent(&req).await {
                let target = self.target_from_req(&req);
                {
                    let mut brain = self.meta_brain.write().await;
                    match req.capability {
                        Capability::UrlOpen => {
                            let parsed_opt = url::Url::parse(&target);
                            let scheme = match &parsed_opt {
                                Ok(parsed) => Some(parsed.scheme().to_ascii_lowercase()),
                                Err(_) => {
                                    if let Some(colon_idx) = target.find(':') {
                                        let possible_scheme = &target[..colon_idx];
                                        if !possible_scheme.is_empty()
                                            && possible_scheme.chars().all(|c| {
                                                c.is_ascii_alphanumeric()
                                                    || c == '+'
                                                    || c == '-'
                                                    || c == '.'
                                            })
                                        {
                                            Some(possible_scheme.to_ascii_lowercase())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                            };
                            if let Some(s) = scheme {
                                if s == "file" {
                                    if let Ok(parsed) = &parsed_opt {
                                        let _ = brain.grant_path_permission(
                                            parsed.path(),
                                            PermissionFlags::FILE_READ,
                                        );
                                    }
                                } else if s == "http" || s == "https" {
                                    if let Ok(parsed) = &parsed_opt {
                                        if let Some(host) = parsed.host_str() {
                                            let _ = brain.grant_domain_permission(
                                                host,
                                                PermissionFlags::NET_OUTBOUND,
                                            );
                                        }
                                    }
                                }
                                let _ = brain
                                    .grant_url_scheme_permission(&s, PermissionFlags::URL_OPEN);
                            }
                        }
                        Capability::FileWrite => {
                            let _ =
                                brain.grant_path_permission(&target, PermissionFlags::FILE_WRITE);
                        }
                        Capability::FileDelete => {
                            let _ =
                                brain.grant_path_permission(&target, PermissionFlags::FILE_DELETE);
                        }
                        Capability::FileRead | Capability::FileExists => {
                            let _ =
                                brain.grant_path_permission(&target, PermissionFlags::FILE_READ);
                        }
                        Capability::OsProcessSpawn | Capability::AgentSpawn => {
                            let binary = req
                                .args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .and_then(|c| c.split_whitespace().next())
                                .unwrap_or(&target);
                            let _ = brain.grant_tool_permission(
                                binary,
                                PermissionFlags::TOOL_EXEC | PermissionFlags::EXEC,
                            );
                        }
                        Capability::AppOpen => {
                            let _ =
                                brain.grant_app_permission(&target, PermissionFlags::APP_LAUNCH);
                        }
                        _ => {
                            let _ =
                                brain.grant_path_permission(&target, PermissionFlags::FILE_READ);
                        }
                    }
                }
                let has_perm = {
                    let brain = self.meta_brain.read().await;
                    brain.has_permission_for_request(&req)
                };
                if !has_perm {
                    return ExecutionResult::gate2_denied(
                        &req.request_id,
                        &format!(
                            "Consent granted but MetaBrain permission still denied for: {}",
                            req.tool_name
                        ),
                    )
                    .with_error_code(DelErrorCode::PermissionDenied {
                        target: req.tool_name.clone(),
                        reason: "MetaBrain grant failed after consent".into(),
                    });
                }
            } else {
                return ExecutionResult::gate2_denied(
                    &req.request_id,
                    &format!("Consent denied for action: {}", req.tool_name),
                )
                .with_error_code(DelErrorCode::PermissionDenied {
                    target: req.tool_name.clone(),
                    reason: "User denied consent".into(),
                });
            }
        }

        // Gate 3
        let result = match Gate3Sandbox::execute(&req) {
            Ok(r) => r,
            Err(r) => r,
        };

        if result.success {
            self.journal.record_success(&req.agent_id);
        } else {
            self.journal.record_denied(&req.agent_id);
        }

        if let Err(e) = self.journal.log_result(&result) {
            eprintln!("[DEL] Journal write failed: {}", e);
        }

        result
    }
}
