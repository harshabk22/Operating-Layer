use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityStatus {
    Discovered,
    Qualified,
    Disqualified,
    EngagementPending,
    Converted,
    Archived,
}

impl OpportunityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Qualified => "qualified",
            Self::Disqualified => "disqualified",
            Self::EngagementPending => "engagement_pending",
            Self::Converted => "converted",
            Self::Archived => "archived",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn can_transition_to(&self, next: OpportunityStatus) -> bool {
        match (self, next) {
            (a, b) if a == &b => true,
            (Self::Discovered, Self::Qualified | Self::Disqualified | Self::Archived) => true,
            (
                Self::Qualified,
                Self::EngagementPending | Self::Converted | Self::Disqualified | Self::Archived,
            ) => true,
            (Self::Disqualified, Self::Qualified | Self::Archived) => true,
            (
                Self::EngagementPending,
                Self::Converted | Self::Qualified | Self::Disqualified | Self::Archived,
            ) => true,
            (Self::Converted, Self::Archived) => true,
            (Self::Archived, Self::Discovered | Self::Qualified) => true,
            _ => false,
        }
    }
}

impl std::str::FromStr for OpportunityStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "discovered" => Ok(Self::Discovered),
            "qualified" => Ok(Self::Qualified),
            "disqualified" => Ok(Self::Disqualified),
            "engagement_pending" => Ok(Self::EngagementPending),
            "converted" => Ok(Self::Converted),
            "archived" => Ok(Self::Archived),
            other => Err(format!("invalid OpportunityStatus: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngagementStatus {
    Proposed,
    PendingAuthorization,
    Authorized,
    Rejected,
    Cancelled,
    Completed,
}

impl EngagementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::PendingAuthorization => "pending_authorization",
            Self::Authorized => "authorized",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for EngagementStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "proposed" => Ok(Self::Proposed),
            "pending_authorization" => Ok(Self::PendingAuthorization),
            "authorized" => Ok(Self::Authorized),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            "completed" => Ok(Self::Completed),
            other => Err(format!("invalid EngagementStatus: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    Active,
    AwaitingAuthorization,
    Escalated,
    Closed,
}

impl ThreadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::AwaitingAuthorization => "awaiting_authorization",
            Self::Escalated => "escalated",
            Self::Closed => "closed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn can_transition_to(&self, next: ThreadStatus) -> bool {
        match (self, next) {
            (a, b) if a == &b => true,
            (Self::Active, Self::AwaitingAuthorization | Self::Escalated | Self::Closed) => true,
            (Self::AwaitingAuthorization, Self::Active | Self::Escalated | Self::Closed) => true,
            (Self::Escalated, Self::Active | Self::Closed) => true,
            (Self::Closed, Self::Active) => true,
            _ => false,
        }
    }
}

impl std::str::FromStr for ThreadStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "awaiting_authorization" => Ok(Self::AwaitingAuthorization),
            "escalated" => Ok(Self::Escalated),
            "closed" => Ok(Self::Closed),
            other => Err(format!("invalid ThreadStatus: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

impl AuthorizationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for AuthorizationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            other => Err(format!("invalid AuthorizationStatus: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalContact {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactIdentityMapping {
    pub id: String,
    pub contact_id: String,
    pub channel: String, // whatsapp, instagram, messenger, webchat
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionThread {
    pub id: String,
    pub contact_id: String,
    pub status: ThreadStatus,
    pub opportunity_status: OpportunityStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub control_mode: ThreadControlMode,
    pub assigned_operator: Option<String>,
    pub is_paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub id: String,
    pub thread_id: String,
    pub sender: String, // contact, business, system
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Opportunity {
    pub id: String,
    pub contact_id: String,
    pub thread_id: String,
    pub status: OpportunityStatus,
    pub value: Option<f64>,
    pub notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub pipeline_stage: PipelineStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledEngagement {
    pub id: String,
    pub contact_id: String,
    pub thread_id: String,
    pub title: String,
    pub status: EngagementStatus,
    pub start_time: i64,
    pub end_time: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorEscalation {
    pub id: String,
    pub thread_id: String,
    pub reason: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationRequest {
    pub id: String,
    pub thread_id: String,
    pub engagement_id: String,
    pub proposed_by: String,
    pub status: AuthorizationStatus,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeQueryResult {
    pub document_id: String,
    pub title: String,
    pub content: String,
    pub relevance: f64,
    pub category: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpportunityRating {
    Cold,
    Warm,
    Hot,
    Critical,
}

impl OpportunityRating {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cold => "cold",
            Self::Warm => "warm",
            Self::Hot => "hot",
            Self::Critical => "critical",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for OpportunityRating {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cold" => Ok(Self::Cold),
            "warm" => Ok(Self::Warm),
            "hot" => Ok(Self::Hot),
            "critical" => Ok(Self::Critical),
            other => Err(format!("invalid OpportunityRating: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityScore {
    pub opportunity_id: String,
    pub intent_score: i32,
    pub urgency_score: i32,
    pub budget_score: i32,
    pub qualification_score: i32,
    pub total_score: i32,
    pub rating: OpportunityRating,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngagementDecision {
    AnswerOnly,
    ProposeEngagement,
    RequestAuthorization,
    Escalate,
    CloseThread,
}

impl EngagementDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AnswerOnly => "answer_only",
            Self::ProposeEngagement => "propose_engagement",
            Self::RequestAuthorization => "request_authorization",
            Self::Escalate => "escalate",
            Self::CloseThread => "close_thread",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for EngagementDecision {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "answer_only" => Ok(Self::AnswerOnly),
            "propose_engagement" => Ok(Self::ProposeEngagement),
            "request_authorization" => Ok(Self::RequestAuthorization),
            "escalate" => Ok(Self::Escalate),
            "close_thread" => Ok(Self::CloseThread),
            other => Err(format!("invalid EngagementDecision: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadObjective {
    QualifyLead,
    ScheduleConsultation,
    SupportCustomer,
    RecoverOpportunity,
}

impl ThreadObjective {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QualifyLead => "qualify_lead",
            Self::ScheduleConsultation => "schedule_consultation",
            Self::SupportCustomer => "support_customer",
            Self::RecoverOpportunity => "recover_opportunity",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for ThreadObjective {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "qualify_lead" => Ok(Self::QualifyLead),
            "schedule_consultation" => Ok(Self::ScheduleConsultation),
            "support_customer" => Ok(Self::SupportCustomer),
            "recover_opportunity" => Ok(Self::RecoverOpportunity),
            other => Err(format!("invalid ThreadObjective: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadObjectiveRecord {
    pub thread_id: String,
    pub objective: ThreadObjective,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorQueueItem {
    pub thread_id: String,
    pub customer_id: String,
    pub reason: String,
    pub priority: String,
    pub created_at: i64,
    pub customer_summary: Option<String>,
    pub thread_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerAuditRecord {
    pub id: String,
    pub action: String,
    pub target_id: String,
    pub performed_by: String,
    pub performed_at: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerMetricsDashboard {
    pub pending_authorizations: u64,
    pub pending_escalations: u64,
    pub bookings_today: u64,
    pub bookings_approved: u64,
    pub bookings_rejected: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadControlMode {
    Ai,
    Human,
    Hybrid,
}

impl ThreadControlMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ai => "ai",
            Self::Human => "human",
            Self::Hybrid => "hybrid",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for ThreadControlMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ai" => Ok(Self::Ai),
            "human" => Ok(Self::Human),
            "hybrid" => Ok(Self::Hybrid),
            other => Err(format!("invalid ThreadControlMode: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    New,
    Qualified,
    ProposalSent,
    AwaitingApproval,
    Booked,
    Won,
    Lost,
}

impl PipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Qualified => "qualified",
            Self::ProposalSent => "proposal_sent",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Booked => "booked",
            Self::Won => "won",
            Self::Lost => "lost",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for PipelineStage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "new" => Ok(Self::New),
            "qualified" => Ok(Self::Qualified),
            "proposal_sent" => Ok(Self::ProposalSent),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "booked" => Ok(Self::Booked),
            "won" => Ok(Self::Won),
            "lost" => Ok(Self::Lost),
            other => Err(format!("invalid PipelineStage: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSuggestion {
    pub thread_id: String,
    pub suggested_reply: String,
    pub confidence: f64,
    pub reasoning_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpportunityPipelineHistory {
    pub id: String,
    pub opportunity_id: String,
    pub from_stage: PipelineStage,
    pub to_stage: PipelineStage,
    pub changed_by: String,
    pub changed_at: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessAnalytics {
    pub average_response_time: f64,
    pub average_authorization_time: f64,
    pub escalation_rate: f64,
    pub booking_conversion_rate: f64,
    pub opportunity_conversion_rate: f64,
    pub human_takeover_rate: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BusinessIntegration {
    pub id: String,
    pub provider: String,
    pub config: String,
    pub created_at: i64,
    pub updated_at: i64,
}
