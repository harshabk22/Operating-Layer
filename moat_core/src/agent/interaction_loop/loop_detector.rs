use crate::agent::interaction_orchestrator::{ActionIntent, TaskStatus};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::LazyLock;

use super::{HARD_STOP_FAILURE, HARD_STOP_SUCCESS, LOOP_WINDOW, SOFT_WARN_SAME_CALL};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecution {
    pub call_hash: [u8; 32],
    pub result_hash: [u8; 32],
    pub is_success: bool,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Optional limits that override module-level loop-detector constants.
/// Mirrors the loop-related fields of `AgentLimits` (moat_core/src/limits.rs).
#[derive(Debug, Clone)]
pub struct LoopDetectorLimits {
    pub hard_stop_success: Option<usize>,
    pub hard_stop_failure: Option<usize>,
    pub soft_warn_same_call: Option<usize>,
    pub loop_window: Option<usize>,
}

pub struct LoopDetector {
    window: VecDeque<ToolExecution>,
    hard_stop_success: usize,
    hard_stop_failure: usize,
    soft_warn_same_call: usize,
    loop_window: usize,
}

#[derive(Debug, Clone)]
pub enum LoopVerdict {
    Clear,
    SoftWarn { message: String },
    HardStop { message: String },
}

static NORMALIZE_PATTERNS: LazyLock<Vec<(Regex, &str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})")
                .unwrap(),
            "<TIMESTAMP>",
        ),
        (
            Regex::new(
                r"(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)\s+\w+\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}(?:\s+\d{4})?",
            )
            .unwrap(),
            "<TIMESTAMP>",
        ),
        (
            Regex::new(r"\b\d{2}:\d{2}:\d{2}(?:\.\d+)?\b").unwrap(),
            "<TIMESTAMP>",
        ),
        (
            Regex::new(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
            )
            .unwrap(),
            "<UUID>",
        ),
        (Regex::new(r"0x[0-9a-fA-F]{6,}").unwrap(), "<ADDR>"),
        (Regex::new(r"(?i)\bpid[= ]\d+").unwrap(), "<PID>"),
        (Regex::new(r"(?i)\bprocess\s+\d+").unwrap(), "process <PID>"),
        (
            Regex::new(r"(?i)\b(?:attempt|try|retry)\s+\d+(?:\s*[/of]+\s*\d+)?").unwrap(),
            "<COUNTER>",
        ),
        (
            Regex::new(r"\b\d+(?:\.\d+)?\s*(?:ms|milliseconds?|s|seconds?)\b").unwrap(),
            "<ELAPSED>",
        ),
        (Regex::new(r"(?i)\bfd[= ]\d+\b").unwrap(), "fd <FD>"),
        (
            Regex::new(r"(?m)^Delegated task: .+$").unwrap(),
            "Delegated task: <TASK>",
        ),
    ]
});

pub fn normalize_result_string(s: &str) -> String {
    let mut out = s.to_owned();
    for (re, rep) in NORMALIZE_PATTERNS.iter() {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Default for LoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl LoopDetector {
    pub fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(LOOP_WINDOW),
            hard_stop_success: HARD_STOP_SUCCESS,
            hard_stop_failure: HARD_STOP_FAILURE,
            soft_warn_same_call: SOFT_WARN_SAME_CALL,
            loop_window: LOOP_WINDOW,
        }
    }

    pub fn with_limits(limits: LoopDetectorLimits) -> Self {
        Self {
            window: VecDeque::with_capacity(limits.loop_window.unwrap_or(LOOP_WINDOW)),
            hard_stop_success: limits.hard_stop_success.unwrap_or(HARD_STOP_SUCCESS),
            hard_stop_failure: limits.hard_stop_failure.unwrap_or(HARD_STOP_FAILURE),
            soft_warn_same_call: limits.soft_warn_same_call.unwrap_or(SOFT_WARN_SAME_CALL),
            loop_window: limits.loop_window.unwrap_or(LOOP_WINDOW),
        }
    }

    pub fn from_window(window: Vec<ToolExecution>) -> Self {
        Self {
            window: window.into_iter().collect(),
            hard_stop_success: HARD_STOP_SUCCESS,
            hard_stop_failure: HARD_STOP_FAILURE,
            soft_warn_same_call: SOFT_WARN_SAME_CALL,
            loop_window: LOOP_WINDOW,
        }
    }

    pub fn snapshot(&self) -> Vec<ToolExecution> {
        self.window.iter().cloned().collect()
    }

    pub(crate) fn analyze(&mut self, execution: ToolExecution) -> LoopVerdict {
        if let Some(ref r) = execution.reason {
            if !r.trim().is_empty() {
                if self.window.len() == self.loop_window {
                    self.window.pop_front();
                }
                self.window.push_back(execution);
                return LoopVerdict::Clear;
            }
        }
        let is_success = execution.is_success;
        let same_call = self
            .window
            .iter()
            .filter(|prev| prev.call_hash == execution.call_hash)
            .count()
            + 1;
        let identical = self
            .window
            .iter()
            .filter(|prev| {
                prev.call_hash == execution.call_hash && prev.result_hash == execution.result_hash
            })
            .count()
            + 1;

        let is_polling = matches!(
            execution.action.as_str(),
            "browser:get_tabs"
                | "browser:get_active_tab"
                | "fs:read"
                | "fs:list"
                | "memory:search"
                | "session:search"
                | "meta:query_brain"
                | "os:applescript"
                | "browser:run_js"
                | "browser:navigate"
                | "os:open_url"
        );

        let hard_stop_threshold = if is_polling {
            if is_success {
                6
            } else {
                5
            }
        } else if is_success {
            self.hard_stop_success
        } else {
            self.hard_stop_failure
        };

        let soft_warn_threshold = if is_polling {
            5
        } else {
            self.soft_warn_same_call
        };

        if self.window.len() == self.loop_window {
            self.window.pop_front();
        }
        self.window.push_back(execution);

        if identical >= hard_stop_threshold {
            LoopVerdict::HardStop {
                message:
                    "Loop detector tripped: identical call/result pair repeated. Escalate to done."
                        .into(),
            }
        } else if self.detect_ping_pong() {
            // OpenJarvis-style ping-pong detection: A-B-A-B or A-B-C-A-B-C cycling
            LoopVerdict::HardStop {
                message: "Loop detector tripped: repetitive tool-calling pattern detected (ping-pong cycle). Change approach or emit done.".into(),
            }
        } else if same_call >= soft_warn_threshold {
            LoopVerdict::SoftWarn {
                message:
                    "Loop detector warning: same call is repeating. Change strategy or finish."
                        .into(),
            }
        } else {
            LoopVerdict::Clear
        }
    }

    /// Detect A-B-A-B or A-B-C-A-B-C cycling patterns in the call hash sequence.
    /// Modeled after OpenJarvis's `_detect_ping_pong` which checks periods 2 and 3.
    fn detect_ping_pong(&self) -> bool {
        let hashes: Vec<_> = self.window.iter().map(|e| &e.call_hash).collect();
        let n = hashes.len();
        for period in [2, 3] {
            if n >= period * 2 {
                let tail = &hashes[n - period * 2..];
                let pattern = &tail[..period];
                if pattern.iter().all(|h| *h == pattern[0]) {
                    continue;
                }
                if tail
                    .iter()
                    .enumerate()
                    .all(|(i, h)| *h == pattern[i % period])
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn hash_execution(intent: &ActionIntent, status: &TaskStatus) -> ToolExecution {
        let mut call_hasher = Sha256::new();
        call_hasher.update(intent.action.as_bytes());
        if !intent.target.is_empty() {
            call_hasher.update(intent.target.as_bytes());
        }
        if let Some(payload) = &intent.payload {
            call_hasher.update(payload.to_string().as_bytes());
        }
        let call_hash: [u8; 32] = call_hasher.finalize().into();

        let is_success;
        let mut result_hasher = Sha256::new();
        match status {
            TaskStatus::Success { stdout } => {
                is_success = true;
                result_hasher.update([1_u8]);
                let normalized = normalize_result_string(stdout.trim());
                result_hasher.update(normalized.as_bytes());
            }
            TaskStatus::SoftDenial { reason } => {
                is_success = false;
                result_hasher.update([3_u8]);
                let normalized = normalize_result_string(reason.trim());
                result_hasher.update(normalized.as_bytes());
            }
            TaskStatus::Failed { exit_code, stderr } => {
                is_success = false;
                result_hasher.update([0_u8]);
                let normalized = normalize_result_string(stderr.trim());
                result_hasher.update(normalized.as_bytes());
                if let Some(code) = exit_code {
                    result_hasher.update(code.to_le_bytes());
                }
            }
            TaskStatus::Blocked { blocked_by } => {
                is_success = false;
                result_hasher.update([0_u8]);
                result_hasher.update(format!("blocked:{blocked_by}").as_bytes());
            }
            TaskStatus::Cancelled { stage } => {
                is_success = false;
                result_hasher.update([2_u8]);
                result_hasher.update(stage.as_bytes());
            }
            TaskStatus::YieldedForConsent { request_id } => {
                is_success = false;
                result_hasher.update([4_u8]);
                result_hasher.update(request_id.as_bytes());
            }
        }
        let result_hash: [u8; 32] = result_hasher.finalize().into();

        ToolExecution {
            call_hash,
            result_hash,
            is_success,
            action: intent.action.clone(),
            reason: intent.reason.clone(),
        }
    }
}
