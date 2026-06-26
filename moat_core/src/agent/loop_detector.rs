use crate::agent::types::{ActionIntent, TaskStatus, ToolExecution};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::LazyLock;

pub(crate) const LOOP_WINDOW: usize = 8;
pub(crate) const HARD_STOP_SUCCESS: usize = 3;
pub(crate) const HARD_STOP_FAILURE: usize = 3;
pub(crate) const SOFT_WARN_SAME_CALL: usize = 3;

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
            window: VecDeque::new(),
            hard_stop_success: HARD_STOP_SUCCESS,
            hard_stop_failure: HARD_STOP_FAILURE,
            soft_warn_same_call: SOFT_WARN_SAME_CALL,
            loop_window: LOOP_WINDOW,
        }
    }

    pub fn with_limits(limits: LoopDetectorLimits) -> Self {
        Self {
            window: VecDeque::new(),
            hard_stop_success: limits.hard_stop_success.unwrap_or(HARD_STOP_SUCCESS),
            hard_stop_failure: limits.hard_stop_failure.unwrap_or(HARD_STOP_FAILURE),
            soft_warn_same_call: limits.soft_warn_same_call.unwrap_or(SOFT_WARN_SAME_CALL),
            loop_window: limits.loop_window.unwrap_or(LOOP_WINDOW),
        }
    }

    pub fn record_execution(&mut self, intent: &ActionIntent, status: &TaskStatus) {
        let is_success = matches!(status, TaskStatus::Success { .. });

        // Hash normalized command call
        let call_str = format!(
            "action: {}, target: {}, payload: {}",
            intent.action,
            intent.target,
            intent.payload.as_ref().unwrap_or(&serde_json::Value::Null)
        );
        let mut hasher = Sha256::new();
        hasher.update(call_str.as_bytes());
        let call_hash: [u8; 32] = hasher.finalize().into();

        // Hash normalized output result
        let result_str = match status {
            TaskStatus::Success { stdout } => normalize_result_string(stdout),
            TaskStatus::SoftDenial { reason } => normalize_result_string(reason),
            TaskStatus::Failed { stderr, .. } => normalize_result_string(stderr),
            TaskStatus::Blocked { blocked_by } => format!("blocked_by: {}", blocked_by),
            TaskStatus::Cancelled { stage } => format!("cancelled: {}", stage),
            TaskStatus::YieldedForConsent { request_id } => format!("yield: {}", request_id),
        };
        let mut hasher = Sha256::new();
        hasher.update(result_str.as_bytes());
        let result_hash: [u8; 32] = hasher.finalize().into();

        self.window.push_back(ToolExecution {
            call_hash,
            result_hash,
            is_success,
            action: intent.action.clone(),
            reason: intent.reason.clone(),
        });

        if self.window.len() > self.loop_window {
            self.window.pop_front();
        }
    }

    pub fn analyze(&self) -> LoopVerdict {
        if self.window.is_empty() {
            return LoopVerdict::Clear;
        }

        // 1. Check for exact consecutive calls: Same call + Same result
        let mut consecutive_repeats = 0;
        let last = self.window.back().unwrap();
        for item in self.window.iter().rev().skip(1) {
            if item.call_hash == last.call_hash && item.result_hash == last.result_hash {
                consecutive_repeats += 1;
            } else {
                break;
            }
        }

        if consecutive_repeats >= self.soft_warn_same_call {
            let limit = if last.is_success {
                self.hard_stop_success
            } else {
                self.hard_stop_failure
            };
            if consecutive_repeats >= limit {
                return LoopVerdict::HardStop {
                    message: format!(
                        "Detected infinite loop of same call+result: action={}.",
                        last.action
                    ),
                };
            }
            return LoopVerdict::SoftWarn {
                message: format!(
                    "Repeated action '{}' with identical result. Avoid looping.",
                    last.action
                ),
            };
        }

        // 2. Check for cycle of period 2 (A -> B -> A -> B)
        if self.window.len() >= 4 {
            let w = &self.window;
            let len = w.len();
            if w[len - 1].call_hash == w[len - 3].call_hash
                && w[len - 2].call_hash == w[len - 4].call_hash
                && w[len - 1].result_hash == w[len - 3].result_hash
                && w[len - 2].result_hash == w[len - 4].result_hash
            {
                return LoopVerdict::HardStop {
                    message: format!(
                        "Detected cycle loop (period 2) between actions: {} and {}.",
                        w[len - 2].action,
                        w[len - 1].action
                    ),
                };
            }
        }

        // 3. Check for cycle of period 3 (A -> B -> C -> A -> B -> C)
        if self.window.len() >= 6 {
            let w = &self.window;
            let len = w.len();
            if w[len - 1].call_hash == w[len - 4].call_hash
                && w[len - 2].call_hash == w[len - 5].call_hash
                && w[len - 3].call_hash == w[len - 6].call_hash
                && w[len - 1].result_hash == w[len - 4].result_hash
                && w[len - 2].result_hash == w[len - 5].result_hash
                && w[len - 3].result_hash == w[len - 6].result_hash
            {
                return LoopVerdict::HardStop {
                    message: format!(
                        "Detected cycle loop (period 3) between actions: {}, {}, and {}.",
                        w[len - 3].action,
                        w[len - 2].action,
                        w[len - 1].action
                    ),
                };
            }
        }

        LoopVerdict::Clear
    }

    pub fn snapshot(&self) -> Vec<ToolExecution> {
        self.window.iter().cloned().collect()
    }
}
