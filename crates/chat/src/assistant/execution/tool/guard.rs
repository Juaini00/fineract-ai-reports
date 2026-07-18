use app_core::auth::model::PrincipalContext;

use crate::{
    assistant::execution::plan::{ExecutionPlan, PolicyDecision, evaluate_policy},
    knowledge::model::KnowledgeCatalog,
};

pub(super) fn guard_selected_capability(
    client: &PrincipalContext,
    catalog: &KnowledgeCatalog,
    plan: &ExecutionPlan,
) -> PolicyDecision {
    evaluate_policy(client, Some(plan), catalog)
}
