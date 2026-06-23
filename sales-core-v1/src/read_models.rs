use crate::events::{Event, EventData};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct PipelineReadModel {
    pub opportunities_by_stage: HashMap<String, Vec<Uuid>>,
    pub total_pipeline_value: i64,
}

impl PipelineReadModel {
    pub fn apply_event(&mut self, event: &Event) {
        match &event.data {
            EventData::OpportunityCreated { .. } => {
                self.opportunities_by_stage
                    .entry("New".to_string())
                    .or_default()
                    .push(event.aggregate_id);
            }
            EventData::OpportunityQualified { budget, .. } => {
                self.move_stage(event.aggregate_id, "New", "Qualified");
                self.total_pipeline_value += budget.amount;
            }
            EventData::SolutionProposed { .. } => {
                self.move_stage(event.aggregate_id, "Qualified", "ProposalActive");
            }
            EventData::ApprovalRequested { .. } => {
                self.move_stage(event.aggregate_id, "ProposalActive", "PendingApproval");
            }
            EventData::QuoteApproved { .. } => {
                self.move_stage(event.aggregate_id, "PendingApproval", "Approved");
            }
            EventData::OpportunityClosedWon { .. } => {
                self.move_stage(event.aggregate_id, "Approved", "ClosedWon");
                // In a real system, you might deduct from pipeline value as it becomes revenue
            }
            EventData::OpportunityClosedLost { .. } => {
                // Remove from active pipeline stages
                for stage in [
                    "New",
                    "Qualified",
                    "ProposalActive",
                    "PendingApproval",
                    "Approved",
                ] {
                    if let Some(list) = self.opportunities_by_stage.get_mut(stage) {
                        list.retain(|id| *id != event.aggregate_id);
                    }
                }
            }
        }
    }

    fn move_stage(&mut self, id: Uuid, from: &str, to: &str) {
        if let Some(list) = self.opportunities_by_stage.get_mut(from) {
            list.retain(|i| *i != id);
        }
        self.opportunities_by_stage
            .entry(to.to_string())
            .or_default()
            .push(id);
    }
}
