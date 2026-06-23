use crate::business::models::{
    EngagementDecision, OpportunityRating, OpportunityScore, ThreadObjective,
};

pub struct OpportunityEvaluator;

impl OpportunityEvaluator {
    pub fn evaluate(opportunity_id: &str, message_content: &str) -> OpportunityScore {
        let lower = message_content.to_lowercase();

        // 1. Intent Score
        let mut intent_score = 2;
        if lower.contains("buy") || lower.contains("purchase") || lower.contains("order") {
            intent_score = 9;
        } else if lower.contains("demo")
            || lower.contains("meeting")
            || lower.contains("schedule")
            || lower.contains("book")
        {
            intent_score = 8;
        } else if lower.contains("interested") || lower.contains("want to") {
            intent_score = 6;
        }

        // 2. Urgency Score
        let mut urgency_score = 2;
        if lower.contains("asap")
            || lower.contains("urgently")
            || lower.contains("now")
            || lower.contains("immediate")
        {
            urgency_score = 10;
        } else if lower.contains("today") || lower.contains("tonight") {
            urgency_score = 8;
        } else if lower.contains("soon") || lower.contains("this week") {
            urgency_score = 5;
        }

        // 3. Budget Score
        let mut budget_score = 2;
        if lower.contains("budget")
            || lower.contains("price")
            || lower.contains("cost")
            || lower.contains("how much")
            || lower.contains("pricing")
        {
            budget_score = 8;
        } else if lower.contains("quote") || lower.contains("rate") {
            budget_score = 6;
        }

        // 4. Qualification Score
        let mut qualification_score = 2;
        if lower.contains("enterprise")
            || lower.contains("business")
            || lower.contains("corporate")
            || lower.contains("company")
        {
            qualification_score = 9;
        } else if lower.contains("plan") || lower.contains("tier") || lower.contains("subscription")
        {
            qualification_score = 7;
        }

        let total_score = intent_score + urgency_score + budget_score + qualification_score;

        let rating = if lower.contains("critical")
            || lower.contains("emergency")
            || urgency_score >= 10
            || total_score >= 30
        {
            OpportunityRating::Critical
        } else if total_score >= 22 {
            OpportunityRating::Hot
        } else if total_score >= 12 {
            OpportunityRating::Warm
        } else {
            OpportunityRating::Cold
        };

        OpportunityScore {
            opportunity_id: opportunity_id.to_string(),
            intent_score,
            urgency_score,
            budget_score,
            qualification_score,
            total_score,
            rating,
            updated_at: chrono::Utc::now().timestamp(),
        }
    }
}

pub struct BusinessPolicy;

impl BusinessPolicy {
    pub fn evaluate(
        intent: &str,
        score: &OpportunityScore,
        objective: &ThreadObjective,
    ) -> EngagementDecision {
        // Rule 1: Booking requires authorization
        if intent == "booking" || intent == "schedule" {
            EngagementDecision::RequestAuthorization
        }
        // Rule 2: Refund/Support requests or Critical opportunities escalate
        else if intent == "refund"
            || intent == "support"
            || score.rating == OpportunityRating::Critical
        {
            EngagementDecision::Escalate
        }
        // Rule 3: Pricing questions may auto-answer
        else if intent == "pricing" {
            EngagementDecision::AnswerOnly
        }
        // Rule 4: Close thread
        else if intent == "close" {
            EngagementDecision::CloseThread
        }
        // Rule 5: Hot opportunity + QualifyLead or ScheduleConsultation objectives proposes engagement
        else if score.rating == OpportunityRating::Hot
            && (*objective == ThreadObjective::QualifyLead
                || *objective == ThreadObjective::ScheduleConsultation)
        {
            EngagementDecision::ProposeEngagement
        }
        // Default: Answer conversational questions
        else {
            EngagementDecision::AnswerOnly
        }
    }
}
