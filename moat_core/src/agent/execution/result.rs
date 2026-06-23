use serde::{Deserialize, Serialize};

/// Typed DEL error codes for structured error handling by the orchestrator.
/// Each variant carries semantic meaning about whether the error is terminal,
/// retryable, or requires external action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelErrorCode {
    /// Tool execution succeeded
    Success,
    /// Permission permanently denied — terminal, loop MUST stop
    PermissionDenied { target: String, reason: String },
    /// Path outside allowed workspace or blocked by denylist
    PathBlocked { path: String, reason: String },
    /// AppleScript blocked by hardline sanitizer
    ScriptBlocked { pattern: String },
    /// Command blocked by hardline list (e.g., rm -rf /)
    CommandBlocked { command: String, reason: String },
    /// Network operation timed out — retryable with backoff
    NetworkTimeout { url: String, elapsed_secs: u64 },
    /// File not found on read operation
    FileNotFound { path: String },
    /// Sandbox violation (Seatbelt profile rejection)
    SandboxViolation { detail: String },
    /// Internal DEL error (should not happen in production)
    InternalError { detail: String },
    /// Consent is required — loop MUST yield and persist state
    YieldedForConsent {
        request_id: String,
        tool_name: String,
        target: String,
    },
    /// Unknown or unclassified error
    Unknown { detail: String },
}

impl DelErrorCode {
    /// Returns true if this error is terminal — the agent must stop, not retry
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::PermissionDenied { .. }
                | Self::CommandBlocked { .. }
                | Self::ScriptBlocked { .. }
                | Self::SandboxViolation { .. }
        )
    }

    /// Returns true if the agent loop should yield and persist state
    pub fn requires_yield(&self) -> bool {
        matches!(self, Self::YieldedForConsent { .. })
    }

    /// Returns true if retry is possible
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkTimeout { .. } | Self::FileNotFound { .. }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub request_id: String,
    pub success: bool,
    pub gate1_passed: bool,
    pub gate2_passed: bool,
    pub gate3_passed: bool,
    pub output: String,
    pub exit_code: Option<i32>,
    pub denied_reason: Option<String>,
    pub execution_time_ms: u64,
    pub error_code: DelErrorCode,
}

impl ExecutionResult {
    pub fn gate1_denied(request_id: &str, reason: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            success: false,
            gate1_passed: false,
            gate2_passed: false,
            gate3_passed: false,
            output: String::new(),
            exit_code: None,
            denied_reason: Some(reason.to_string()),
            execution_time_ms: 0,
            error_code: DelErrorCode::InternalError {
                detail: reason.to_string(),
            },
        }
    }

    pub fn gate2_denied(request_id: &str, reason: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            success: false,
            gate1_passed: true,
            gate2_passed: false,
            gate3_passed: false,
            output: String::new(),
            exit_code: None,
            denied_reason: Some(reason.to_string()),
            execution_time_ms: 0,
            error_code: DelErrorCode::PermissionDenied {
                target: String::new(),
                reason: reason.to_string(),
            },
        }
    }

    pub fn gate3_denied(request_id: &str, reason: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            success: false,
            gate1_passed: true,
            gate2_passed: true,
            gate3_passed: false,
            output: String::new(),
            exit_code: None,
            denied_reason: Some(reason.to_string()),
            execution_time_ms: 0,
            error_code: DelErrorCode::SandboxViolation {
                detail: reason.to_string(),
            },
        }
    }

    pub fn success(
        request_id: &str,
        output: String,
        exit_code: Option<i32>,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            success: true,
            gate1_passed: true,
            gate2_passed: true,
            gate3_passed: true,
            output,
            exit_code,
            denied_reason: None,
            execution_time_ms: elapsed_ms,
            error_code: DelErrorCode::Success,
        }
    }

    /// Override the error_code field. Useful for gate sites that need
    /// a more specific error code than the constructor's default.
    pub fn with_error_code(mut self, code: DelErrorCode) -> Self {
        self.error_code = code;
        self
    }
}
