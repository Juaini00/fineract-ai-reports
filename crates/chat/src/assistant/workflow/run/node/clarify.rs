use crate::assistant::{ClarificationKind, ClarificationPayload};

pub fn is_interrupt(payload: &ClarificationPayload) -> bool {
    matches!(
        payload.kind,
        ClarificationKind::SelectEntity
            | ClarificationKind::SelectOption
            | ClarificationKind::CollectFields
            | ClarificationKind::FreeText
    )
}
