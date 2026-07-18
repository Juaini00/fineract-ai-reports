use crate::assistant::AssistantDomain;

pub(super) fn extract_domain(message: &str) -> Option<AssistantDomain> {
    if message.contains("client") {
        Some(AssistantDomain::Client)
    } else if message.contains("office") || message.contains("organization") {
        Some(AssistantDomain::Organization)
    } else if message.contains("saving") {
        Some(AssistantDomain::Savings)
    } else {
        None
    }
}

pub(super) fn extract_metric(message: &str) -> Option<&'static str> {
    if message.contains("most savings account") || message.contains("number of savings account") {
        Some("savings_account_count")
    } else if message.contains("highest balance") || message.contains("savings balance") {
        Some("savings_balance")
    } else if message.contains("deposit volume") || message.contains("deposited the most") {
        Some("deposit_volume")
    } else {
        None
    }
}
