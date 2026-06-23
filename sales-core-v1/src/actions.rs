use crate::events::{Event, EventData};

pub trait Action {
    fn execute(&self, event: &Event);
}

pub struct NotifyManagerAction;

impl Action for NotifyManagerAction {
    fn execute(&self, event: &Event) {
        if let EventData::ApprovalRequested { approval_id } = &event.data {
            println!(
                "Action: Notify manager about approval request {}",
                approval_id
            );
        }
    }
}

pub struct IssueContractAction;

impl Action for IssueContractAction {
    fn execute(&self, event: &Event) {
        if let EventData::OpportunityClosedWon { .. } = &event.data {
            println!(
                "Action: Issue contract for opportunity {}",
                event.aggregate_id
            );
        }
    }
}
