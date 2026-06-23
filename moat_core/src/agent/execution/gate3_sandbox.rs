#![allow(unused_imports)]
use super::gate2_policy::normalize_workspace_path;
use super::request::ExecutionRequest;
use super::result::ExecutionResult;
use anyhow::{Context, Result};
use std::fs;
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub struct Gate3Sandbox;

impl Gate3Sandbox {
    pub fn execute(req: &ExecutionRequest) -> Result<ExecutionResult, ExecutionResult> {
        let start = std::time::Instant::now();

        match req.tool_name.as_str() {
            "read_file" | "fs_read" => Self::execute_read_file(req, start),
            "write_file" | "fs_write" => Self::execute_write_file(req, start),
            "fs_exists" => Self::execute_fs_exists(req, start),
            "fs_delete" => Self::execute_fs_delete(req, start),
            "fs_mkdir" => Self::execute_mkdir(req, start),
            "list_files" => Self::execute_list_files(req, start),
            _ => Err(ExecutionResult::gate3_denied(
                &req.request_id,
                &format!("Unknown sandbox tool: {}", req.tool_name),
            )),
        }
    }

    fn execute_read_file(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let raw = req.args["path"].as_str().unwrap();
        let normalized = normalize_workspace_path(raw);
        let path = Self::resolve_path(&normalized, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;

        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| {
                ExecutionResult::gate3_denied(
                    &req.request_id,
                    &format!(
                        "Read open failed (O_NOFOLLOW): {}. Symlink traversal denied.",
                        e
                    ),
                )
            })?;

        let content = std::io::read_to_string(file).map_err(|e| {
            ExecutionResult::gate3_denied(&req.request_id, &format!("Read failed: {}", e))
        })?;

        let elapsed = start.elapsed().as_millis() as u64;
        eprintln!("[Gate3] ✓ read_file succeeded ({:.1}ms)", elapsed as f64);
        Ok(ExecutionResult::success(
            &req.request_id,
            content,
            None,
            elapsed,
        ))
    }

    fn execute_write_file(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let raw = req.args["path"].as_str().unwrap();
        let content = req.args["content"].as_str().unwrap();
        let normalized = normalize_workspace_path(raw);
        let path = Self::resolve_path(&normalized, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;

        if let Some(reason) = crate::security::denylist::is_write_denied(&path) {
            return Err(ExecutionResult::gate3_denied(
                &req.request_id,
                &format!(
                    "Write denied: {} is a protected {} — blocked by denylist",
                    path.display(),
                    reason
                ),
            ));
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                ExecutionResult::gate3_denied(&req.request_id, &format!("mkdir failed: {}", e))
            })?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| {
                ExecutionResult::gate3_denied(
                    &req.request_id,
                    &format!(
                        "Write open failed (O_NOFOLLOW): {}. Symlink traversal denied.",
                        e
                    ),
                )
            })?;

        use std::io::Write;
        file.write_all(content.as_bytes()).map_err(|e| {
            ExecutionResult::gate3_denied(&req.request_id, &format!("Write failed: {}", e))
        })?;

        let elapsed = start.elapsed().as_millis() as u64;
        eprintln!("[Gate3] ✓ write_file succeeded ({:.1}ms)", elapsed as f64);
        Ok(ExecutionResult::success(
            &req.request_id,
            format!("Wrote {} bytes to {:?}", content.len(), path),
            None,
            elapsed,
        ))
    }

    fn execute_fs_exists(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let raw = req.args["path"].as_str().unwrap_or("");
        let normalized = normalize_workspace_path(raw);
        let path = Self::resolve_path(&normalized, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;

        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(ExecutionResult::gate3_denied(
                    &req.request_id,
                    "fs_exists denied: symlink traversal not allowed",
                ));
            }
        }

        let exists = path.exists();
        let elapsed = start.elapsed().as_millis() as u64;
        Ok(ExecutionResult::success(
            &req.request_id,
            format!("[fs:exists] {} → {}", raw, exists),
            None,
            elapsed,
        ))
    }

    fn execute_fs_delete(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let raw = req.args["path"].as_str().unwrap_or("");
        let normalized = normalize_workspace_path(raw);
        let path = Self::resolve_path(&normalized, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;

        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() {
                return Err(ExecutionResult::gate3_denied(
                    &req.request_id,
                    "fs_delete denied: symlink traversal not allowed",
                ));
            }
        }

        let trash_dir = Self::runtime_root(Some(&req.working_dir))
            .map_err(|e| {
                ExecutionResult::gate3_denied(
                    &req.request_id,
                    &format!("Cannot determine runtime directory: {}", e),
                )
            })?
            .join("trash");
        fs::create_dir_all(&trash_dir).map_err(|e| {
            ExecutionResult::gate3_denied(
                &req.request_id,
                &format!("trash dir creation failed: {}", e),
            )
        })?;

        let original_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let trash_name = format!("{}_{}", uuid::Uuid::new_v4(), original_name);
        let trash_path = trash_dir.join(&trash_name);

        fs::rename(&path, &trash_path).map_err(|e| {
            ExecutionResult::gate3_denied(&req.request_id, &format!("Delete failed: {}", e))
        })?;

        let elapsed = start.elapsed().as_millis() as u64;
        Ok(ExecutionResult::success(
            &req.request_id,
            format!(
                "[fs:delete] Deleted {} (moved to trash: {:?})",
                path.display(),
                trash_path
            ),
            None,
            elapsed,
        ))
    }

    fn execute_mkdir(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let path_str = normalize_workspace_path(req.args["path"].as_str().unwrap());
        let path = Self::resolve_path(&path_str, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;
        fs::create_dir_all(&path).map_err(|e| {
            ExecutionResult::gate3_denied(&req.request_id, &format!("mkdir failed: {}", e))
        })?;
        let elapsed = start.elapsed().as_millis() as u64;
        eprintln!("[Gate3] ✓ mkdir succeeded ({:.1}ms)", elapsed as f64);
        Ok(ExecutionResult::success(
            &req.request_id,
            format!("Created directory {:?}", path),
            None,
            elapsed,
        ))
    }

    fn execute_list_files(
        req: &ExecutionRequest,
        start: std::time::Instant,
    ) -> Result<ExecutionResult, ExecutionResult> {
        let path_str = normalize_workspace_path(req.args["path"].as_str().unwrap());
        let path = Self::resolve_path(&path_str, &req.working_dir)
            .map_err(|e| ExecutionResult::gate3_denied(&req.request_id, &e))?;
        let entries: Vec<String> = std::fs::read_dir(&path)
            .map_err(|e| {
                ExecutionResult::gate3_denied(&req.request_id, &format!("list_files failed: {}", e))
            })?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        let elapsed = start.elapsed().as_millis() as u64;
        let output = format!("{} entries:\n{}", entries.len(), entries.join("\n"));
        Ok(ExecutionResult::success(
            &req.request_id,
            output,
            None,
            elapsed,
        ))
    }

    fn runtime_root(_fallback: Option<&Path>) -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("MOAT_RUNTIME_ROOT") {
            return Ok(PathBuf::from(path));
        }

        #[cfg(test)]
        {
            let root = std::env::var_os("MOAT_TEST_ROOT")
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .or_else(|| _fallback.map(Path::to_path_buf))
                .unwrap_or_else(|| PathBuf::from("."));
            Ok(root.join("test_scratch").join("runtime"))
        }

        #[cfg(not(test))]
        {
            let home = dirs::home_dir().context("Cannot determine home directory")?;
            Ok(home.join(".ageex"))
        }
    }

    fn resolve_path(path_str: &str, working_dir: &Path) -> Result<PathBuf, String> {
        let expanded = if path_str.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                path_str.replacen("~", &home.display().to_string(), 1)
            } else {
                path_str.to_string()
            }
        } else {
            path_str.to_string()
        };
        let path = PathBuf::from(&expanded);
        let resolved = if path.is_absolute() {
            path
        } else {
            working_dir.join(path)
        };
        match fs::canonicalize(&resolved) {
            Ok(canonical) => Ok(canonical),
            Err(_) => {
                let mut current = resolved.clone();
                loop {
                    if let Some(parent) = current.parent() {
                        current = parent.to_path_buf();
                        match fs::canonicalize(&current) {
                            Ok(canonical_parent) => {
                                let remaining =
                                    resolved.strip_prefix(&current).unwrap_or(&resolved);
                                return Ok(canonical_parent.join(remaining));
                            }
                            Err(_) => {
                                if current.parent().is_none() {
                                    return Ok(resolved);
                                }
                            }
                        }
                    } else {
                        return Ok(resolved);
                    }
                }
            }
        }
    }
}
