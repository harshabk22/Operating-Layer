#![allow(clippy::all)]
// moat_core/src/lib.rs
//
// Three-Layer Architecture:
//   Agent  — reasoning, planning, LLM interaction, consent, orchestration
//   Memory — knowledge graph, context, sessions, profiles, rules
//   DEL    — 3-gate execution pipeline, tools, security, gateway
//   (cross-cutting: system.rs — bootstrap)
//

pub mod agent;
pub mod api;
pub mod business;
pub mod communication;
pub mod config;
pub mod hooks;
pub mod integration;
pub mod manifest;
pub mod memory;
pub mod planner;
pub mod security;
pub mod session;
pub mod package;

// ============================================================
// § SHARED — Error Taxonomy & Result Alias
// ============================================================

#[derive(Debug, thiserror::Error)]
pub enum MoatError {
    #[error("meta-brain graph error: {0}")]
    Graph(String),

    #[error("meta-brain database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("VSA engine error: {0}")]
    Vsa(String),

    #[error("orchestrator error: {0}")]
    Orchestrator(String),

    #[error("security violation: {0}")]
    SecurityViolation(String),

    #[error("invariant violation: {0}")]
    InvariantViolation(String),

    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("LLM API error: {0}")]
    Llm(String),

    #[error("context injection error: {0}")]
    Injection(String),

    #[error("settings error: {0}")]
    Settings(String),
}

pub type MoatResult<T> = Result<T, MoatError>;

// Legacy system code removed.
