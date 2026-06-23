use super::request::ExecutionRequest;
use super::result::{DelErrorCode, ExecutionResult};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct ExecutionPolicy {
    pub allow_network: bool,
    pub allow_process_spawn: bool,
    pub max_file_write_size_mb: usize,
    pub blocked_commands: Vec<String>,
    pub allowed_paths: Vec<String>,
    pub require_approval_for: Vec<String>,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        let workspace = home.join(".moatageex").join("workspace");
        Self {
            allow_network: true,
            allow_process_spawn: true,
            max_file_write_size_mb: 10,
            blocked_commands: vec![
                "rm -rf /".into(),
                "rm -rf ~".into(),
                "rm -Rf".into(),
                "sudo".into(),
                "mkfs".into(),
                "dd if=".into(),
                "shutdown".into(),
                "reboot".into(),
                "halt".into(),
                "poweroff".into(),
                "chmod 777 /".into(),
                "chown -R".into(),
                ":(){ :|:& };:".into(),
                "diskutil eraseDisk".into(),
                "diskutil unmount".into(),
                "diskutil unmountDisk".into(),
                "format C:".into(),
            ],
            allowed_paths: vec![
                "~/Desktop".into(),
                "~/Desktop/ageex-workspace".into(),
                "~/Downloads".into(),
                "~/Documents".into(),
                workspace.to_string_lossy().to_string(),
            ],
            require_approval_for: vec!["ProcessSpawn".into(), "FileWrite".into()],
        }
    }
}

#[derive(Clone)]
pub struct Gate2Policy {
    policy: ExecutionPolicy,
}

impl Gate2Policy {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read policy file: {:?}", path))?;
        let policy: ExecutionPolicy =
            toml::from_str(&contents).with_context(|| "Failed to parse TOML policy")?;
        Ok(Self { policy })
    }

    pub fn default() -> Self {
        Self {
            policy: ExecutionPolicy::default(),
        }
    }

    pub fn enforce(&self, req: &ExecutionRequest) -> Result<(), ExecutionResult> {
        let start = std::time::Instant::now();

        if let Some(command) = req.args.get("command").and_then(|v| v.as_str()) {
            for blocked in &self.policy.blocked_commands {
                let matched = if blocked.contains(' ') {
                    command.starts_with(blocked.as_str())
                } else {
                    let first_token = command.split_whitespace().next().unwrap_or("");
                    first_token == blocked.as_str()
                };
                if matched {
                    return Err(ExecutionResult::gate2_denied(
                        &req.request_id,
                        &format!("Blocked command pattern: {}", blocked),
                    )
                    .with_error_code(DelErrorCode::CommandBlocked {
                        command: command.to_string(),
                        reason: format!("Command matches blocked pattern: {}", blocked),
                    }));
                }
            }
        }

        if let Some(path_str) = req.args.get("path").and_then(|v| v.as_str()) {
            let normalized = Self::expand_tilde(&normalize_workspace_path(path_str));
            let mut path = PathBuf::from(&normalized);
            if !path.is_absolute() {
                path = req.working_dir.join(&path);
            }
            let resolved = match fs::canonicalize(&path) {
                Ok(canonical) => canonical,
                Err(_) => {
                    let parent = path.parent().unwrap_or(&path);
                    match fs::canonicalize(parent) {
                        Ok(canonical_parent) => {
                            canonical_parent.join(path.file_name().unwrap_or_default())
                        }
                        Err(_) => path.clone(),
                    }
                }
            };
            if !Self::is_path_allowed(&resolved, &self.policy.allowed_paths) {
                return Err(ExecutionResult::gate2_denied(
                    &req.request_id,
                    &format!("Path not in allowlist: {:?}", resolved),
                )
                .with_error_code(DelErrorCode::PathBlocked {
                    path: resolved.to_string_lossy().to_string(),
                    reason: "Path not in allowlist".to_string(),
                }));
            }
        }

        if req.tool_name == "write_file" || req.tool_name == "fs_write" {
            if let Some(content) = req.args.get("content").and_then(|v| v.as_str()) {
                let size_mb = content.len() / (1024 * 1024);
                if size_mb > self.policy.max_file_write_size_mb {
                    return Err(ExecutionResult::gate2_denied(
                        &req.request_id,
                        &format!(
                            "File too large: {}MB > {}MB limit",
                            size_mb, self.policy.max_file_write_size_mb
                        ),
                    )
                    .with_error_code(DelErrorCode::InternalError {
                        detail: format!(
                            "File size {}MB exceeds policy limit {}MB",
                            size_mb, self.policy.max_file_write_size_mb
                        ),
                    }));
                }
            }
        }

        if !self.policy.allow_network
            && (req.tool_name == "web_request"
                || req.tool_name == "url_open"
                || req.tool_name == "net_curl")
        {
            return Err(ExecutionResult::gate2_denied(
                &req.request_id,
                "Network access disabled by policy",
            )
            .with_error_code(DelErrorCode::PermissionDenied {
                target: req.tool_name.clone(),
                reason: "Network access disabled by policy".to_string(),
            }));
        }

        if !self.policy.allow_process_spawn
            && (req.tool_name == "process_spawn"
                || req.tool_name == "app_open"
                || req.tool_name == "applescript"
                || req.tool_name == "net_curl")
        {
            return Err(ExecutionResult::gate2_denied(
                &req.request_id,
                "Process spawning disabled by policy",
            )
            .with_error_code(DelErrorCode::PermissionDenied {
                target: req.tool_name.clone(),
                reason: "Process spawning disabled by policy".to_string(),
            }));
        }

        eprintln!(
            "[Gate2] ✓ Policy checks passed ({:.1}ms)",
            start.elapsed().as_secs_f64() * 1000.0
        );
        Ok(())
    }

    fn is_path_allowed(path: &Path, allowed: &[String]) -> bool {
        for allowed_pattern in allowed {
            let allowed_path = PathBuf::from(Self::expand_tilde(allowed_pattern));
            if path.starts_with(&allowed_path) {
                return true;
            }
        }
        false
    }

    /// Check if a path falls within the default allowed paths (workspace).
    /// Used for auto-granting permissions without consent.
    pub fn is_path_allowed_fast(path: &Path) -> bool {
        let workspace = dirs::home_dir()
            .map(|h| h.join(".moatageex").join("workspace"))
            .unwrap_or_default();
        let ageex_workspace = dirs::home_dir()
            .map(|h| h.join("Desktop").join("ageex-workspace"))
            .unwrap_or_default();
        let desktop = dirs::home_dir()
            .map(|h| h.join("Desktop"))
            .unwrap_or_default();
        let downloads = dirs::home_dir()
            .map(|h| h.join("Downloads"))
            .unwrap_or_default();
        let documents = dirs::home_dir()
            .map(|h| h.join("Documents"))
            .unwrap_or_default();
        path.starts_with(&workspace)
            || path.starts_with(&ageex_workspace)
            || path.starts_with(&desktop)
            || path.starts_with(&downloads)
            || path.starts_with(&documents)
    }

    pub fn expand_tilde(path: &str) -> String {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return path.replacen("~", &home.display().to_string(), 1);
            }
        }
        path.to_string()
    }
}

pub(crate) fn normalize_workspace_path(path_str: &str) -> String {
    if let Some(suffix) = path_str.strip_prefix("/workspace/") {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        return home
            .join("Desktop")
            .join("ageex-workspace")
            .join(suffix)
            .to_string_lossy()
            .to_string();
    }
    if path_str == "/workspace" {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        return home
            .join("Desktop")
            .join("ageex-workspace")
            .to_string_lossy()
            .to_string();
    }
    path_str.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::capabilities::Capability;

    #[test]
    fn test_blocked_command_denied() {
        let gate = Gate2Policy::default();
        let req = ExecutionRequest::new(
            "business_runtime",
            "process_spawn",
            Capability::OsProcessSpawn,
            serde_json::json!({"command": "rm -rf / --no-preserve-root"}),
        );
        assert!(gate.enforce(&req).is_err());
    }

    #[test]
    fn test_allowed_path_passes() {
        let gate = Gate2Policy::default();
        let req = ExecutionRequest::new(
            "business_runtime",
            "write_file",
            Capability::FileWrite,
            serde_json::json!({
                "path": "~/Desktop/ageex-workspace/test.txt",
                "content": "hello"
            }),
        );
        assert!(gate.enforce(&req).is_ok());
    }

    #[test]
    fn test_disallowed_path_denied() {
        let gate = Gate2Policy::default();
        let req = ExecutionRequest::new(
            "business_runtime",
            "write_file",
            Capability::FileWrite,
            serde_json::json!({
                "path": "/etc/passwd",
                "content": "pwned"
            }),
        );
        assert!(gate.enforce(&req).is_err());
    }
}
