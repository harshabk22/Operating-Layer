use moat_core::MoatResult;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use std::time::SystemTime;

pub struct DirectorySync {
    db: Arc<TokioMutex<Connection>>,
    business_id: String,
    connector_type: String,
    session_dir: String,
    file_hashes: Mutex<HashMap<String, SystemTime>>,
}

impl DirectorySync {
    pub fn new(
        db: Arc<TokioMutex<Connection>>,
        business_id: String,
        connector_type: String,
        session_dir: String,
    ) -> Self {
        Self {
            db,
            business_id,
            connector_type,
            session_dir,
            file_hashes: Mutex::new(HashMap::new()),
        }
    }

    /// Populate the local directory from SQLite
    pub async fn populate_directory(&self) -> MoatResult<()> {
        let conn = self.db.lock().await;
        let files = moat_core::communication::db::load_session_files(
            &conn,
            &self.business_id,
            &self.connector_type,
        )?;

        let path = Path::new(&self.session_dir);
        if !path.exists() {
            fs::create_dir_all(path).map_err(|e| {
                moat_core::MoatError::Orchestrator(format!("Failed to create session dir: {}", e))
            })?;
        }

        let mut hashes = self.file_hashes.lock().unwrap();
        hashes.clear();

        for (name, content) in files {
            let file_path = path.join(&name);
            fs::write(&file_path, &content).map_err(|e| {
                moat_core::MoatError::Orchestrator(format!("Failed to write session file: {}", e))
            })?;

            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Ok(modified) = metadata.modified() {
                    hashes.insert(name, modified);
                }
            }
        }

        Ok(())
    }

    /// Sync modifications back to SQLite
    pub async fn sync_to_db(&self) -> MoatResult<()> {
        let path = Path::new(&self.session_dir);
        if !path.exists() {
            return Ok(());
        }

        let mut current_files = HashMap::new();

        // Read files currently in directory
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_file() {
                    if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') || name == "bridge.pid" {
                            continue; // skip hidden files and process PIDs
                        }
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                current_files.insert(name.to_string(), modified);
                            }
                        }
                    }
                }
            }
        }

        // Determine what to write and delete while holding the hashes lock briefly.
        let (to_write, to_delete) = {
            let hashes = self.file_hashes.lock().unwrap();
            let mut to_write = Vec::new();
            let mut to_delete = Vec::new();

            // 1. Detect new or modified files
            for (name, modified) in &current_files {
                let should_sync = match hashes.get(name) {
                    Some(prev_mod) => prev_mod != modified,
                    None => true,
                };

                if should_sync {
                    let file_path = path.join(name);
                    if let Ok(content) = fs::read(&file_path) {
                        to_write.push((name.clone(), content, *modified));
                    }
                }
            }

            // 2. Detect deleted files
            for name in hashes.keys() {
                if !current_files.contains_key(name) {
                    to_delete.push(name.clone());
                }
            }

            (to_write, to_delete)
        };

        if to_write.is_empty() && to_delete.is_empty() {
            return Ok(());
        }

        let conn = self.db.lock().await;

        // Perform writes
        for (name, content, modified) in to_write {
            moat_core::communication::db::save_session_file(
                &conn,
                &self.business_id,
                &self.connector_type,
                &name,
                &content,
            )?;
            let mut hashes = self.file_hashes.lock().unwrap();
            hashes.insert(name, modified);
        }

        // Perform deletions
        for name in to_delete {
            moat_core::communication::db::delete_session_file(
                &conn,
                &self.business_id,
                &self.connector_type,
                &name,
            )?;
            let mut hashes = self.file_hashes.lock().unwrap();
            hashes.remove(&name);
        }

        Ok(())
    }
}
