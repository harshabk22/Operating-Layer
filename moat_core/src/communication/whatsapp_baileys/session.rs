use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Unpaired,
    Pairing,
    Connected,
    Reconnecting,
    Disconnected,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SessionState::Unpaired => "unpaired",
            SessionState::Pairing => "pairing",
            SessionState::Connected => "connected",
            SessionState::Reconnecting => "reconnecting",
            SessionState::Disconnected => "disconnected",
        };
        write!(f, "{}", s)
    }
}
