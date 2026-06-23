/// Per-agent budget profile for context window allocation.
pub struct AgentBudgetProfile {
    pub query_share: f64,
    pub memory_share: f64,
    pub tool_share: f64,
    pub session_share: f64,
    pub delegate_share: f64,
    pub reserve_share: f64,
    pub min_memory_tokens: u32,
    pub min_session_tokens: u32,
    pub max_total_tokens: u32,
    pub observation_cap: u32,
}

impl AgentBudgetProfile {
    pub fn for_agent(name: &str) -> Self {
        match name {
            "spotlight" => Self {
                query_share: 0.60,
                memory_share: 0.10,
                tool_share: 0.05,
                session_share: 0.05,
                delegate_share: 0.00,
                reserve_share: 0.20,
                min_memory_tokens: 200,
                min_session_tokens: 100,
                max_total_tokens: 128_000,
                observation_cap: 300,
            },
            "coder" => Self {
                query_share: 0.15,
                memory_share: 0.10,
                tool_share: 0.35,
                session_share: 0.05,
                delegate_share: 0.15,
                reserve_share: 0.20,
                min_memory_tokens: 300,
                min_session_tokens: 200,
                max_total_tokens: 128_000,
                observation_cap: 1200,
            },
            "core_os" => Self {
                query_share: 0.20,
                memory_share: 0.10,
                tool_share: 0.25,
                session_share: 0.10,
                delegate_share: 0.15,
                reserve_share: 0.20,
                min_memory_tokens: 300,
                min_session_tokens: 300,
                max_total_tokens: 128_000,
                observation_cap: 1000,
            },
            "connector" => Self {
                query_share: 0.40,
                memory_share: 0.10,
                tool_share: 0.05,
                session_share: 0.20,
                delegate_share: 0.05,
                reserve_share: 0.20,
                min_memory_tokens: 400,
                min_session_tokens: 800,
                max_total_tokens: 128_000,
                observation_cap: 500,
            },
            _ => Self {
                query_share: 0.30,
                memory_share: 0.15,
                tool_share: 0.10,
                session_share: 0.15,
                delegate_share: 0.10,
                reserve_share: 0.20,
                min_memory_tokens: 500,
                min_session_tokens: 500,
                max_total_tokens: 128_000,
                observation_cap: 800,
            },
        }
    }

    pub fn compute_budget(&self) -> ContextBudget {
        let total = self.max_total_tokens;
        let reserve = (total as f64 * self.reserve_share) as u32;
        let remaining = total - reserve;
        let memory = ((remaining as f64 * self.memory_share) as u32).max(self.min_memory_tokens);
        let session = ((remaining as f64 * self.session_share) as u32).max(self.min_session_tokens);
        let after_floors = remaining.saturating_sub(memory + session);
        let query_denom = self.query_share + self.tool_share + self.delegate_share;
        let query = if query_denom > 0.0 {
            (after_floors as f64 * self.query_share / query_denom) as u32
        } else {
            0
        };
        let tool = if query_denom > 0.0 {
            (after_floors as f64 * self.tool_share / query_denom) as u32
        } else {
            0
        };
        let delegate = after_floors.saturating_sub(query + tool);
        ContextBudget {
            persona_tokens: 2500,
            memory_tokens: memory,
            skill_tokens: 500,
            session_tokens: session,
            query_tokens: query,
            tool_tokens: tool,
            delegate_tokens: delegate,
            reserve_tokens: reserve,
            max_total_tokens: total,
            observation_cap: self.observation_cap,
        }
    }
}

pub struct ContextBudget {
    pub persona_tokens: u32,
    pub memory_tokens: u32,
    pub skill_tokens: u32,
    pub session_tokens: u32,
    pub query_tokens: u32,
    pub tool_tokens: u32,
    pub delegate_tokens: u32,
    pub reserve_tokens: u32,
    pub max_total_tokens: u32,
    pub observation_cap: u32,
}

impl ContextBudget {
    pub fn available(&self, used: u32) -> u32 {
        self.max_total_tokens.saturating_sub(used)
    }

    pub fn can_fit_observation(&self, current: u32, observation_size: u32) -> bool {
        current + observation_size < self.max_total_tokens
    }

    pub fn truncate_observation(text: &str, cap: u32) -> String {
        use crate::agent::context::TokenBudget;
        let tokens = TokenBudget::count_tokens(text).unwrap_or(text.len());
        if tokens <= cap as usize {
            return text.to_string();
        }
        let chars = text
            .chars()
            .take((cap as usize).max(50))
            .collect::<String>();
        format!("{chars}...[truncated]")
    }
}
