use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingStatus {
    pub state: super::session::SessionState,
    pub qr_code: Option<String>,
}
