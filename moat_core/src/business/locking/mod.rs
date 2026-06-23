use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub struct LockGuard {
    manager: Arc<LockManager>,
    key: String,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Ok(mut locks) = self.manager.locks.lock() {
            locks.remove(&self.key);
        }
    }
}

pub struct LockManager {
    locks: Mutex<HashSet<String>>,
}

impl Default for LockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LockManager {
    pub fn new() -> Self {
        Self {
            locks: Mutex::new(HashSet::new()),
        }
    }

    pub fn try_lock(self: &Arc<Self>, key: &str) -> Option<LockGuard> {
        let mut locks = self.locks.lock().ok()?;
        if locks.contains(key) {
            None
        } else {
            locks.insert(key.to_string());
            Some(LockGuard {
                manager: self.clone(),
                key: key.to_string(),
            })
        }
    }
}
